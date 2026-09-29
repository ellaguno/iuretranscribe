//! Servidor MCP local: `IureTranscribe --mcp`.
//!
//! Claude Desktop, Claude Code, Copilot en VS Code o cualquier cliente MCP lanza este
//! mismo ejecutable con `--mcp` y le habla por stdin/stdout. No se abre ventana ni se
//! arranca Tauri: sólo el motor de Whisper.
//!
//! El reparto es el que hace útil esto: la transcripción corre aquí, en el equipo (el
//! audio nunca sale), y el modelo del cliente —el que el usuario ya paga— lee el texto
//! y hace el resumen o la minuta. Así no hace falta llave de OpenRouter.
//!
//! Una transcripción larga tarda más de lo que un cliente espera una herramienta. Por
//! eso `transcribe_file` espera como mucho `wait_seconds` y, si no ha terminado, devuelve
//! el `jobId` para seguir con `transcription_status`; el trabajo continúa mientras tanto.
//!
//! El protocolo lo pone `iurefficient_connect::mcp_server`. El registro va a stderr y a
//! `iuretranscribe-mcp.log`, nunca a stdout.

use crate::settings::Settings;
use crate::subtitles::{self, Segment};
use crate::transcribe::{Engine, EngineEvent};
use crate::{audio, gpu, models, transcribe, vocab};
use anyhow::{anyhow, Result};
use iurefficient_connect::mcp_server::{self, cut, Call, CallError, Handler, Reply, ServerInfo};
use iurefficient_connect::{lang, tr};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_MAX_CHARS: usize = 20_000;
const MAX_MAX_CHARS: usize = 200_000;
/// Cuánto espera una llamada por omisión antes de devolver el `jobId`. Menos que el
/// minuto que suelen tolerar los clientes.
const DEFAULT_WAIT_SECS: u64 = 50;
const MAX_WAIT_SECS: u64 = 110;

pub struct Paths {
    pub settings_path: PathBuf,
    pub models_dir: PathBuf,
    pub bundled_models_dir: Option<PathBuf>,
    pub recordings_default: PathBuf,
}

/// Una transcripción en curso o terminada.
struct Job {
    input: PathBuf,
    cancel: Arc<AtomicBool>,
    percent: AtomicI32,
    stage: Mutex<String>,
    /// `Some` al terminar.
    done: Mutex<Option<std::result::Result<Done, String>>>,
    finished: Condvar,
}

/// Transcripción terminada: los datos y el texto con y sin horas.
struct Done {
    summary: Value,
    plain: String,
    stamped: String,
}

struct Server {
    settings: Mutex<Settings>,
    paths: Paths,
    engine: Arc<Engine>,
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    seq: AtomicU64,
}

pub fn serve(settings: Settings, paths: Paths) -> i32 {
    mcp_server::serve(Server {
        settings: Mutex::new(settings),
        paths,
        engine: Arc::new(Engine::default()),
        jobs: Mutex::new(HashMap::new()),
        seq: AtomicU64::new(1),
    })
}

impl Handler for Server {
    fn info(&self) -> ServerInfo {
        ServerInfo {
            name: "iuretranscribe",
            title: "IureTranscribe",
            version: env!("CARGO_PKG_VERSION"),
            instructions: lang::pick(
                "Local speech-to-text (Whisper) for audio and video files. Everything runs on this computer; the audio never leaves it. \
                 Paths must be absolute. transcribe_file writes the transcript (SRT/TXT… as set in the app) next to the audio and returns the text; \
                 if it is still running it returns a jobId: call transcription_status with it until status is done. \
                 You write the summary or minutes from the returned text. list_recordings shows the user's latest recordings.",
                "Transcripción local (Whisper) de archivos de audio y video. Todo corre en este equipo; el audio nunca sale de él. \
                 Las rutas deben ser absolutas. transcribe_file escribe la transcripción (SRT/TXT… según la app) junto al audio y devuelve el texto; \
                 si sigue en curso devuelve un jobId: llama a transcription_status con él hasta que status sea done. \
                 El resumen o la minuta los redactas tú a partir del texto. list_recordings muestra las últimas grabaciones del usuario.",
            )
            .into(),
        }
    }

    fn tools(&self) -> Value {
        tools()
    }

    fn call(&self, name: &str, args: &Value, call: &Call) -> Result<Reply, CallError> {
        let data = match name {
            "transcribe_file" => self.transcribe_file(args, call)?,
            "transcription_status" => self.transcription_status(args, call)?,
            "read_transcript" => self.read_transcript(args)?,
            "audio_info" => self.audio_info(args)?,
            "list_recordings" => self.list_recordings(args)?,
            "transcription_setup" => self.transcription_setup()?,
            _ => return Err(CallError::Params(format!("unknown tool: {name}"))),
        };
        Ok(Reply::Data(data))
    }
}

impl Server {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn transcribe_file(&self, args: &Value, call: &Call) -> Result<Value> {
        let input = input_path(args)?;
        let mut settings = self.settings();
        if let Some(l) = str_arg(args, "language") {
            settings.language = l.to_ascii_lowercase();
        }
        if let Some(t) = args.get("translate").and_then(Value::as_bool) {
            settings.translate = t;
        }
        let model_id = settings.model_id.clone();
        let model_path = models::resolve(&self.paths.models_dir, self.paths.bundled_models_dir.as_deref(), &model_id).ok_or_else(|| {
            anyhow!(tr!(
                "The model “{model_id}” is not downloaded. Open IureTranscribe → Models to download it.",
                "El modelo «{model_id}» no está descargado. Abre IureTranscribe → Modelos para descargarlo."
            ))
        })?;
        let use_gpu = gpu::effective_use_gpu_headless(&self.settings, &self.paths.settings_path, &model_path, settings.use_gpu).map_err(|e| anyhow!(e))?;

        let job_id = format!("t{}", self.seq.fetch_add(1, Ordering::Relaxed));
        let job = Arc::new(Job {
            input: input.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
            percent: AtomicI32::new(0),
            stage: Mutex::new("decoding".into()),
            done: Mutex::new(None),
            finished: Condvar::new(),
        });
        self.jobs.lock().unwrap().insert(job_id.clone(), job.clone());

        let opts = transcribe::Options {
            job_id: job_id.clone(),
            input: input.clone(),
            model_path,
            language: settings.language.clone(),
            translate: settings.translate,
            use_gpu,
            threads: settings.threads,
            beam_size: settings.beam_size.max(1),
            initial_prompt: vocab::initial_prompt(&settings),
        };
        let engine = self.engine.clone();
        let worker_job = job.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let sink_job = worker_job.clone();
            let sink: transcribe::EventSink = Arc::new(move |ev: EngineEvent| {
                if let EngineEvent::Progress(p) = ev {
                    sink_job.percent.store(p.percent, Ordering::Relaxed);
                    *sink_job.stage.lock().unwrap() = p.stage;
                }
            });
            let result = engine.run(sink, opts, worker_job.cancel.clone()).and_then(|mut out| {
                vocab::Corrector::from_settings(&settings).apply_segments(&mut out.segments);
                let outputs = crate::write_outputs(&settings, &input, &out.segments).map_err(|e| anyhow!(e))?.0;
                let summary = json!({
                    "status": "done",
                    "audioSecs": (out.audio_secs * 10.0).round() / 10.0,
                    "elapsedSecs": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                    "detectedLanguage": out.detected_language,
                    "segments": out.segments.len(),
                    "outputs": outputs.iter().map(|o| json!({"format": o.format, "path": o.path})).collect::<Vec<_>>(),
                });
                // Se guardan las dos versiones del texto; `timestamps` elige al leer.
                Ok(Done { summary, plain: transcript_text(&out.segments, false), stamped: transcript_text(&out.segments, true) })
            });
            let done = match result {
                Ok(d) => {
                    log::info!("MCP: transcripción {} lista", worker_job.input.display());
                    Ok(d)
                }
                // Al cancelar, whisper aborta con un error genérico: se dice lo que pasó.
                Err(_) if worker_job.cancel.load(Ordering::Relaxed) => Err(tr!("Cancelled", "Cancelado")),
                Err(e) => {
                    log::warn!("MCP: transcripción {}: {e:#}", worker_job.input.display());
                    Err(format!("{e:#}"))
                }
            };
            *worker_job.done.lock().unwrap() = Some(done);
            worker_job.finished.notify_all();
        });

        self.wait_and_report(&job_id, &job, args, call, 0)
    }

    fn transcription_status(&self, args: &Value, call: &Call) -> Result<Value> {
        let job_id = str_arg(args, "job_id").ok_or_else(|| anyhow!(tr!("Missing job_id", "Falta job_id")))?.to_string();
        let job = self.jobs.lock().unwrap().get(&job_id).cloned().ok_or_else(|| {
            anyhow!(tr!(
                "No transcription with id {job_id} (ids are lost when the assistant restarts). Use read_transcript on the written file, or transcribe again.",
                "No hay transcripción con id {job_id} (los ids se pierden al reiniciar el asistente). Usa read_transcript con el archivo escrito o vuelve a transcribir."
            ))
        })?;
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        self.wait_and_report(&job_id, &job, args, call, offset)
    }

    /// Espera hasta `wait_seconds` (enviando progreso) y devuelve el resultado o el estado.
    /// Si el cliente cancela la llamada mientras espera, se cancela la transcripción.
    fn wait_and_report(&self, job_id: &str, job: &Job, args: &Value, call: &Call, offset: usize) -> Result<Value> {
        let wait = Duration::from_secs(args.get("wait_seconds").and_then(Value::as_u64).unwrap_or(DEFAULT_WAIT_SECS).min(MAX_WAIT_SECS));
        let deadline = Instant::now() + wait;
        let mut last = -1;
        let mut done = job.done.lock().unwrap();
        while done.is_none() && Instant::now() < deadline {
            if call.cancelled() {
                job.cancel.store(true, Ordering::Relaxed);
                return Err(anyhow!(tr!("Cancelled", "Cancelado")));
            }
            let pct = job.percent.load(Ordering::Relaxed);
            if pct != last {
                last = pct;
                call.progress(pct as f64, Some(100.0), &stage_message(&job.stage.lock().unwrap(), pct));
            }
            done = job.finished.wait_timeout(done, Duration::from_millis(500)).unwrap().0;
        }
        match done.as_ref() {
            None => {
                let pct = job.percent.load(Ordering::Relaxed);
                Ok(json!({
                    "status": "running",
                    "jobId": job_id,
                    "percent": pct,
                    "stage": *job.stage.lock().unwrap(),
                    "next": tr!(
                        "Still transcribing ({pct}%). Call transcription_status with job_id \"{job_id}\" to keep waiting.",
                        "Sigue transcribiendo ({pct} %). Llama a transcription_status con job_id \"{job_id}\" para seguir esperando."
                    ),
                }))
            }
            Some(Err(e)) => Err(anyhow!(e.clone())),
            Some(Ok(d)) => {
                let stamped = args.get("timestamps").and_then(Value::as_bool).unwrap_or(false);
                let text = if stamped { &d.stamped } else { &d.plain };
                let mut v = d.summary.clone();
                v["jobId"] = json!(job_id);
                v["input"] = json!(job.input);
                if d.plain.trim().is_empty() {
                    v["note"] = json!(tr!(
                        "No speech was recognized. If the file does have speech, try again with the language set (language: \"es\", \"en\"…).",
                        "No se reconoció voz. Si el archivo sí tiene habla, vuelve a intentarlo indicando el idioma (language: \"es\", \"en\"…)."
                    ));
                }
                add_text(&mut v, text, offset, max_chars(args), &tr!(
                    "Call transcription_status with job_id \"{job_id}\" and offset {{next}} to read the rest.",
                    "Llama a transcription_status con job_id \"{job_id}\" y offset {{next}} para leer el resto."
                ));
                Ok(v)
            }
        }
    }

    fn read_transcript(&self, args: &Value) -> Result<Value> {
        let p = str_arg(args, "path").ok_or_else(|| anyhow!(tr!("Missing path", "Falta path")))?;
        let path = absolute(p)?;
        let ext = extension(&path);
        if !["txt", "srt", "vtt", "json", "md"].contains(&ext.as_str()) {
            return Err(anyhow!(tr!(
                "Only transcripts (.txt, .srt, .vtt, .json) and documents (.md) can be read; transcribe audio with transcribe_file.",
                "Sólo se leen transcripciones (.txt, .srt, .vtt, .json) y documentos (.md); el audio se transcribe con transcribe_file."
            )));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| anyhow!(tr!("Could not read {p}: {e}", "No se pudo leer {p}: {e}")))?;
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let mut v = json!({"path": path});
        add_text(&mut v, &text, offset, max_chars(args), &tr!(
            "Call read_transcript again with offset {{next}} to read the rest.",
            "Vuelve a llamar a read_transcript con offset {{next}} para leer el resto."
        ));
        Ok(v)
    }

    fn audio_info(&self, args: &Value) -> Result<Value> {
        let input = input_path(args)?;
        let bytes = std::fs::metadata(&input)?.len();
        let duration = audio::probe_duration(&input);
        Ok(json!({
            "path": input,
            "bytes": bytes,
            "durationSecs": duration.map(|d| d.round()),
            "transcripts": existing_transcripts(&self.settings(), &input),
        }))
    }

    fn list_recordings(&self, args: &Value) -> Result<Value> {
        let settings = self.settings();
        let dir = settings
            .recordings_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.paths.recordings_default.clone());
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10).clamp(1, 50) as usize;
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && audio::SUPPORTED_EXTENSIONS.contains(&extension(p).as_str()))
                    .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        let recordings: Vec<Value> = files
            .into_iter()
            .take(limit)
            .map(|(when, p)| {
                json!({
                    "path": p,
                    "modified": chrono::DateTime::<chrono::Local>::from(when).format("%Y-%m-%d %H:%M").to_string(),
                    "durationSecs": audio::probe_duration(&p).map(|d| d.round()),
                    "transcripts": existing_transcripts(&settings, &p),
                })
            })
            .collect();
        Ok(json!({"folder": dir, "recordings": recordings}))
    }

    fn transcription_setup(&self) -> Result<Value> {
        let s = self.settings();
        let downloaded: Vec<String> = models::list(&self.paths.models_dir, self.paths.bundled_models_dir.as_deref(), &models::Downloads::default())
            .into_iter()
            .filter(|m| m.downloaded)
            .map(|m| m.id)
            .collect();
        Ok(json!({
            "model": s.model_id,
            "modelReady": downloaded.contains(&s.model_id),
            "downloadedModels": downloaded,
            "language": s.language,
            "translate": s.translate,
            "formats": s.formats,
            "backend": transcribe::backend_name(),
            "vocabulary": !s.vocabulary.trim().is_empty(),
            "corrections": s.corrections.len(),
        }))
    }
}

/// Texto de la transcripción, una línea por segmento (con el hablante si se distinguió) y,
/// con `stamped`, la hora de inicio de cada una.
fn transcript_text(segments: &[Segment], stamped: bool) -> String {
    if !stamped {
        return subtitles::to_txt(segments);
    }
    segments
        .iter()
        .map(|s| {
            let secs = s.start_ms.max(0) / 1000;
            format!("[{:02}:{:02}:{:02}] {}", secs / 3600, (secs / 60) % 60, secs % 60, subtitles::line(s))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Añade `text` desde `offset` (en caracteres) hasta `max`, con `next` si queda más.
/// `hint` lleva `{next}` donde va el siguiente offset.
fn add_text(v: &mut Value, text: &str, offset: usize, max: usize, hint: &str) {
    let total = text.chars().count();
    let rest: String = text.chars().skip(offset).collect();
    let (part, truncated) = cut(&rest, max);
    let next = offset + part.chars().count();
    v["text"] = json!(part);
    v["totalChars"] = json!(total);
    v["offset"] = json!(offset);
    if truncated {
        v["nextOffset"] = json!(next);
        v["next"] = json!(hint.replace("{next}", &next.to_string()));
    }
}

fn stage_message(stage: &str, pct: i32) -> String {
    match stage {
        "decoding" => lang::pick("Reading the audio", "Leyendo el audio").into(),
        "loading" => lang::pick("Loading the model", "Cargando el modelo").into(),
        _ => tr!("Transcribing: {pct}%", "Transcribiendo: {pct} %"),
    }
}

/// Transcripciones que ya existen junto al audio (o en la carpeta de salida).
fn existing_transcripts(settings: &Settings, input: &Path) -> Vec<String> {
    let dir = crate::resolve_output_dir(settings, input);
    let Some(stem) = input.file_stem().map(|s| s.to_string_lossy().into_owned()) else { return vec![] };
    ["txt", "srt", "vtt", "json"]
        .iter()
        .map(|ext| dir.join(format!("{stem}.{ext}")))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

// ------------------------------------------------------------------ argumentos

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn max_chars(args: &Value) -> usize {
    args.get("max_chars").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(DEFAULT_MAX_CHARS).min(MAX_MAX_CHARS)
}

/// Rutas absolutas y nada más: el proceso lo lanza el cliente con un directorio de
/// trabajo que nadie controla.
fn absolute(p: &str) -> Result<PathBuf> {
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return Err(anyhow!(tr!("The path must be absolute: {p}", "La ruta debe ser absoluta: {p}")));
    }
    Ok(path)
}

fn input_path(args: &Value) -> Result<PathBuf> {
    let p = str_arg(args, "path").ok_or_else(|| anyhow!(tr!("Missing path", "Falta path")))?;
    let path = absolute(p)?;
    if !path.is_file() {
        return Err(anyhow!(tr!("File not found: {p}", "No existe el archivo {p}")));
    }
    if !audio::SUPPORTED_EXTENSIONS.contains(&extension(&path).as_str()) {
        return Err(anyhow!(tr!(
            "Unsupported format. Audio and video: {}",
            "Formato no admitido. Audio y video: {}",
            audio::SUPPORTED_EXTENSIONS.join(", ")
        )));
    }
    Ok(path)
}

fn extension(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

// ------------------------------------------------------------------ esquema

/// Los nombres son identificadores en inglés; las descripciones salen en el idioma de la
/// interfaz, porque son lo que el modelo lee para decidir cuándo usar cada herramienta.
fn tools() -> Value {
    let path = json!({"type": "string", "description": lang::pick(
        "Absolute path of the audio or video file.", "Ruta absoluta del archivo de audio o video.")});
    let max_chars = json!({"type": "integer", "description": lang::pick(
        "Maximum characters of text to return (default 20000); the response says how to read the rest.",
        "Máximo de caracteres de texto a devolver (por omisión 20000); la respuesta dice cómo leer el resto.")});
    let timestamps = json!({"type": "boolean", "description": lang::pick(
        "Prefix each line with its start time [hh:mm:ss] (useful for minutes).",
        "Anteponer a cada línea su hora de inicio [hh:mm:ss] (útil para minutas).")});
    let wait = json!({"type": "integer", "description": lang::pick(
        "Seconds to wait for the result before returning the job status (default 50, max 110).",
        "Segundos a esperar el resultado antes de devolver el estado del trabajo (por omisión 50, máx. 110).")});
    let read_only = json!({"readOnlyHint": true, "openWorldHint": false});
    json!([
        {
            "name": "transcribe_file",
            "title": lang::pick("Transcribe audio", "Transcribir audio"),
            "description": lang::pick(
                "Transcribes an audio or video file with Whisper on this computer (the audio never leaves it). Writes the transcript files next to the audio (formats as set in the app) and returns the text. Long files keep running in the background: if status is \"running\", call transcription_status with the jobId.",
                "Transcribe un archivo de audio o video con Whisper en este equipo (el audio nunca sale de él). Escribe los archivos de la transcripción junto al audio (formatos según la app) y devuelve el texto. Los archivos largos siguen en segundo plano: si status es \"running\", llama a transcription_status con el jobId."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": path,
                    "language": {"type": "string", "description": lang::pick(
                        "ISO language code (es, en…) or auto. Defaults to the app setting.",
                        "Código ISO del idioma (es, en…) o auto. Por omisión, el de la app.")},
                    "translate": {"type": "boolean", "description": lang::pick(
                        "Translate to English instead of transcribing.", "Traducir al inglés en lugar de transcribir.")},
                    "timestamps": timestamps,
                    "wait_seconds": wait,
                    "max_chars": max_chars,
                },
                "required": ["path"],
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
        },
        {
            "name": "transcription_status",
            "title": lang::pick("Transcription status", "Estado de la transcripción"),
            "description": lang::pick(
                "Waits for a transcription started with transcribe_file and returns its text when done, or its progress. Also reads the rest of a long transcript with offset.",
                "Espera una transcripción iniciada con transcribe_file y devuelve su texto al terminar, o su avance. También lee el resto de una transcripción larga con offset."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job_id": {"type": "string", "description": "jobId"},
                    "offset": {"type": "integer", "description": lang::pick("Character where the text starts (default 0).", "Carácter desde el que empieza el texto (por omisión 0).")},
                    "timestamps": timestamps,
                    "wait_seconds": wait,
                    "max_chars": max_chars,
                },
                "required": ["job_id"],
            },
            "annotations": read_only,
        },
        {
            "name": "read_transcript",
            "title": lang::pick("Read a transcript", "Leer una transcripción"),
            "description": lang::pick(
                "Reads an existing transcript (.txt, .srt, .vtt, .json) or a summary/minutes (.md) written by IureTranscribe, in parts with offset.",
                "Lee una transcripción existente (.txt, .srt, .vtt, .json) o un resumen o minuta (.md) de IureTranscribe, por partes con offset."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": lang::pick("Absolute path of the file.", "Ruta absoluta del archivo.")},
                    "offset": {"type": "integer", "description": lang::pick("Character where the text starts (default 0).", "Carácter desde el que empieza el texto (por omisión 0).")},
                    "max_chars": max_chars,
                },
                "required": ["path"],
            },
            "annotations": read_only,
        },
        {
            "name": "audio_info",
            "title": lang::pick("Audio info", "Datos del audio"),
            "description": lang::pick(
                "Duration and size of an audio or video file, and the transcripts that already exist for it.",
                "Duración y tamaño de un archivo de audio o video, y las transcripciones que ya existen de él."),
            "inputSchema": {"type": "object", "properties": {"path": path}, "required": ["path"]},
            "annotations": read_only,
        },
        {
            "name": "list_recordings",
            "title": lang::pick("Latest recordings", "Últimas grabaciones"),
            "description": lang::pick(
                "The user's latest recordings made with IureTranscribe (meetings, calls), newest first, with duration and whether they are already transcribed.",
                "Las últimas grabaciones del usuario hechas con IureTranscribe (reuniones, llamadas), de la más reciente a la más antigua, con su duración y si ya están transcritas."),
            "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "description": lang::pick("How many (1-50, default 10).", "Cuántas (1-50, por omisión 10).")}}},
            "annotations": read_only,
        },
        {
            "name": "transcription_setup",
            "title": lang::pick("Transcription settings", "Ajustes de transcripción"),
            "description": lang::pick(
                "The Whisper model and language IureTranscribe uses, the downloaded models and the output formats.",
                "El modelo de Whisper y el idioma que usa IureTranscribe, los modelos descargados y los formatos de salida."),
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": read_only,
        },
    ])
}
