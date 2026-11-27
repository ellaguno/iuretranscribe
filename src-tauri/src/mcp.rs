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
//! Grabar (para IureDesk: «grabar la junta» sin abrir esta ventana): `start_recording`,
//! `recording_status`, `pause_recording` y `stop_recording` usan el mismo grabador que la app,
//! con sus ajustes (micrófono, audio del sistema, carpeta). La grabación vive en este proceso:
//! quien la empieza mantiene vivo el cliente MCP hasta detenerla.
//!
//! Lo que se graba y transcribe aquí aparece en la lista de la ventana: cada cambio se deja
//! en el buzón de trabajos (`inbox`) y la ventana lo incorpora. `generate_document` escribe
//! el resumen o la minuta con las instrucciones y la llave de OpenRouter de la app (los mismos
//! que el botón de la ventana), y cada respuesta trae `openLink`
//! (`iuretranscribe://open?path=…`) para abrir esa grabación en la ventana.
//!
//! El protocolo lo pone `iurefficient_connect::mcp_server`. El registro va a stderr y a
//! `iuretranscribe-mcp.log`, nunca a stdout.

use crate::inbox::{self, Meta, Record};
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
/// Lo más que espera `generate_document` al modelo (el cliente HTTP corta a los 600 s).
const DOC_TIMEOUT: Duration = Duration::from_secs(660);
/// Máximo de caracteres del `context` de `generate_document`.
const MAX_CONTEXT_CHARS: usize = 20_000;

pub struct Paths {
    pub settings_path: PathBuf,
    pub models_dir: PathBuf,
    pub bundled_models_dir: Option<PathBuf>,
    pub recordings_default: PathBuf,
    /// Buzón de trabajos de la ventana (`None` si no se supo la carpeta de datos).
    pub inbox_dir: Option<PathBuf>,
    /// `jobs.json` de la ventana (sólo se lee, para los datos de la reunión).
    pub jobs_path: PathBuf,
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
    recorder: crate::recorder::Recorder,
    /// Datos de la reunión recibidos en `transcribe_file`, por ruta del audio.
    metas: Mutex<HashMap<PathBuf, Meta>>,
}

pub fn serve(settings: Settings, paths: Paths) -> i32 {
    mcp_server::serve(Server {
        settings: Mutex::new(settings),
        paths,
        engine: Arc::new(Engine::default()),
        jobs: Mutex::new(HashMap::new()),
        seq: AtomicU64::new(1),
        recorder: crate::recorder::Recorder::default(),
        metas: Mutex::new(HashMap::new()),
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
                 Optional meeting details (title, date, place, participants, notes) go in transcribe_file. \
                 generate_document writes the summary or minutes with the user's own prompts and AI settings in IureTranscribe (next to the transcript, as <name>_summary.md / _minutes.md); \
                 if it is not configured it returns an error and you can write them yourself from the text. \
                 Everything recorded or transcribed here appears in IureTranscribe's list; openLink (iuretranscribe://open?path=…) opens that recording in the app. \
                 list_recordings shows the user's latest recordings.",
                "Transcripción local (Whisper) de archivos de audio y video. Todo corre en este equipo; el audio nunca sale de él. \
                 Las rutas deben ser absolutas. transcribe_file escribe la transcripción (SRT/TXT… según la app) junto al audio y devuelve el texto; \
                 si sigue en curso devuelve un jobId: llama a transcription_status con él hasta que status sea done. \
                 Los datos de la reunión (title, date, place, participants, notes) son opcionales y van en transcribe_file. \
                 generate_document escribe el resumen o la minuta con las instrucciones y los ajustes de IA del usuario en IureTranscribe (junto a la transcripción, como <nombre>_resumen.md / _minuta.md); \
                 si no está configurado devuelve un error y puedes redactarlos tú a partir del texto. \
                 Todo lo que se graba o transcribe aquí aparece en la lista de IureTranscribe; openLink (iuretranscribe://open?path=…) abre esa grabación en la app. \
                 list_recordings muestra las últimas grabaciones del usuario.",
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
            "start_recording" => self.start_recording(args)?,
            "generate_document" => self.generate_document(args, call)?,
            "recording_status" => serde_json::to_value(self.recorder.status()).map_err(anyhow::Error::from)?,
            "pause_recording" => {
                let paused = args.get("paused").and_then(Value::as_bool).unwrap_or(true);
                self.recorder.set_paused(paused)?;
                json!({"paused": paused})
            }
            "stop_recording" => self.stop_recording()?,
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

        let meta = Meta::from_args(args);
        if !meta.is_empty() {
            self.metas.lock().unwrap_or_else(|e| e.into_inner()).insert(input.clone(), meta.clone());
        }
        let inbox_dir = self.paths.inbox_dir.clone();
        let mut rec = Record::new(&input, Some("transcribing"));
        rec.size_bytes = std::fs::metadata(&input).ok().map(|m| m.len());
        rec.meta = Some(meta.clone()).filter(|m| !m.is_empty());
        inbox::post(inbox_dir.as_deref(), &rec);

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
                let (outputs, output_dir, base_name) = crate::write_outputs(&settings, &input, &out.segments).map_err(|e| anyhow!(e))?;
                let mut rec = Record::new(&input, Some("done"));
                rec.size_bytes = std::fs::metadata(&input).ok().map(|m| m.len());
                rec.duration_secs = Some(out.audio_secs);
                rec.meta = Some(meta.clone()).filter(|m| !m.is_empty());
                rec.result = Some(crate::TranscriptResult {
                    job_id: String::new(),
                    segments: out.segments.clone(),
                    text: subtitles::to_plain(&out.segments),
                    audio_secs: out.audio_secs,
                    elapsed_secs: started.elapsed().as_secs_f64(),
                    outputs: outputs.clone(),
                    output_dir: output_dir.to_string_lossy().into_owned(),
                    base_name,
                    detected_language: out.detected_language.clone(),
                });
                inbox::post(inbox_dir.as_deref(), &rec);
                let summary = json!({
                    "status": "done",
                    "audioSecs": (out.audio_secs * 10.0).round() / 10.0,
                    "elapsedSecs": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                    "detectedLanguage": out.detected_language,
                    "segments": out.segments.len(),
                    "outputs": outputs.iter().map(|o| json!({"format": o.format, "path": o.path})).collect::<Vec<_>>(),
                    "openLink": open_link(&input),
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
            if let Err(e) = &done {
                let mut rec = Record::new(&worker_job.input, Some("error"));
                rec.error = Some(e.clone());
                inbox::post(inbox_dir.as_deref(), &rec);
            }
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

    /// Carpeta de las grabaciones (la de los ajustes o la de omisión).
    fn recordings_dir(&self) -> PathBuf {
        self.settings()
            .recordings_dir
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.paths.recordings_default.clone())
    }

    /// Empieza a grabar con los ajustes de la app (sin transcripción en vivo: se transcribe al
    /// terminar). Error si ya hay una grabación en este proceso.
    fn start_recording(&self, args: &Value) -> Result<Value> {
        if self.recorder.status().active {
            return Err(anyhow!(tr!("A recording is already in progress.", "Ya hay una grabación en curso.")));
        }
        let settings = self.settings();
        let opts = crate::recorder::StartOptions {
            capture_mic: settings.record_mic,
            mic_device: settings.mic_device.clone(),
            capture_system: settings.record_system,
            output_dir: self.recordings_dir(),
            live: None,
            title: str_arg(args, "title").map(|t| t.chars().take(300).collect()),
        };
        let path = self.recorder.start(opts)?;
        log::info!("MCP: grabando en {}", path.display());
        let mut rec = Record::new(&path, Some("recording"));
        if let Some(t) = str_arg(args, "title").map(str::trim).filter(|t| !t.is_empty()) {
            rec.meta = Some(inbox::Meta { title: t.chars().take(300).collect(), ..Default::default() });
        }
        inbox::post(self.paths.inbox_dir.as_deref(), &rec);
        Ok(json!({"path": path.to_string_lossy(), "active": true, "openLink": open_link(&path)}))
    }

    /// Detiene la grabación; en la ventana queda como grabación sin transcribir.
    fn stop_recording(&self) -> Result<Value> {
        let res = self.recorder.stop()?;
        let path = PathBuf::from(&res.path);
        let mut rec = Record::new(&path, Some("recorded"));
        rec.size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
        rec.duration_secs = Some(res.duration_secs).filter(|d| d.is_finite());
        inbox::post(self.paths.inbox_dir.as_deref(), &rec);
        let mut v = serde_json::to_value(&res)?;
        v["openLink"] = json!(open_link(&path));
        Ok(v)
    }

    /// Resumen o minuta con las instrucciones, la llave y el modelo de OpenRouter de la app
    /// (lo mismo que el botón de la ventana). Espera al modelo hasta `DOC_TIMEOUT`.
    fn generate_document(&self, args: &Value, call: &Call) -> Result<Value> {
        let req = doc_args(args)?;
        let settings = self.settings();
        if settings.openrouter_api_key.trim().is_empty() || settings.openrouter_model.trim().is_empty() {
            return Err(anyhow!(tr!(
                "IureTranscribe has no AI configured for summaries and minutes. Open IureTranscribe → Settings and set the OpenRouter key and model, or write the document yourself from the transcript text.",
                "IureTranscribe no tiene IA configurada para resúmenes y minutas. Abre IureTranscribe → Ajustes y pon la llave y el modelo de OpenRouter, o redacta tú el documento a partir del texto de la transcripción."
            )));
        }
        let (audio, result) = locate_transcript(&settings, &req.path)?;
        let context = match req.context {
            Some(c) => Some(c),
            None => audio.as_deref().and_then(|a| self.stored_meta(a)).map(|m| m.context()).filter(|c| !c.trim().is_empty()),
        };
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let started = Instant::now();
        let label = match req.kind {
            "summary" => lang::pick("Writing the summary", "Redactando el resumen"),
            _ => lang::pick("Writing the minutes", "Redactando la minuta"),
        };
        let doc = rt.block_on(async {
            let fut = crate::write_document(&settings, req.kind, &result.text, Path::new(&result.output_dir), &result.base_name, context.as_deref());
            tokio::pin!(fut);
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    r = &mut fut => break r.map_err(|e| anyhow!(e)),
                    _ = tick.tick() => {
                        if call.cancelled() {
                            break Err(anyhow!(tr!("Cancelled", "Cancelado")));
                        }
                        if started.elapsed() > DOC_TIMEOUT {
                            break Err(anyhow!(tr!(
                                "The AI model took too long to answer. Try again later or with another model in IureTranscribe → Settings.",
                                "El modelo de IA tardó demasiado en responder. Inténtalo más tarde o con otro modelo en IureTranscribe → Ajustes."
                            )));
                        }
                        let secs = started.elapsed().as_secs();
                        call.progress(secs as f64, None, &format!("{label}… {secs} s"));
                    }
                }
            }
        })?;
        log::info!("MCP: {} escrito en {}", req.kind, doc.path);
        // La ventana lo muestra en el trabajo de esa grabación.
        let mut rec = Record::new(audio.as_deref().unwrap_or(&req.path), None);
        if audio.is_none() {
            rec.result = Some(result.clone());
        }
        match req.kind {
            "summary" => rec.summary_path = Some(doc.path.clone()),
            _ => rec.minutes_path = Some(doc.path.clone()),
        }
        inbox::post(self.paths.inbox_dir.as_deref(), &rec);
        let mut v = json!({"kind": req.kind, "path": doc.path, "content": doc.content});
        if let Some(a) = &audio {
            v["openLink"] = json!(open_link(a));
        }
        Ok(v)
    }

    /// Datos de la reunión de ese audio: los de `transcribe_file` en este proceso o, si no,
    /// los que la ventana guardó en `jobs.json`.
    fn stored_meta(&self, audio: &Path) -> Option<Meta> {
        let mem = self.metas.lock().unwrap_or_else(|e| e.into_inner()).get(audio).cloned();
        mem.or_else(|| inbox::stored_meta(&self.paths.jobs_path, audio))
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

/// Enlace que abre esa grabación en la ventana de IureTranscribe.
fn open_link(path: &Path) -> String {
    format!("iuretranscribe://open?path={}", percent_encode(&path.to_string_lossy()))
}

/// Codifica para el valor de un parámetro de URL (todo salvo `A-Z a-z 0-9 - _ . ~ /`).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// La transcripción de `path` (un audio o una transcripción) y el audio, si se encuentra.
fn locate_transcript(settings: &Settings, path: &Path) -> Result<(Option<PathBuf>, crate::TranscriptResult)> {
    let ext = extension(path);
    let shown = path.display();
    if crate::TRANSCRIPT_EXTENSIONS.contains(&ext.as_str()) {
        let (Some(dir), Some(base)) = (path.parent(), path.file_stem().map(|s| s.to_string_lossy().into_owned())) else {
            return Err(anyhow!(tr!("Invalid path: {shown}", "Ruta no válida: {shown}")));
        };
        let result = crate::transcript_in(dir, &base).ok_or_else(|| anyhow!(tr!("Could not read the transcript {shown}.", "No se pudo leer la transcripción {shown}.")))?;
        let audio = audio::SUPPORTED_EXTENSIONS.iter().map(|e| dir.join(format!("{base}.{e}"))).find(|p| p.is_file());
        return Ok((audio, result));
    }
    if !audio::SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(anyhow!(tr!(
            "Give the path of the audio or of its transcript (.txt, .srt, .vtt, .json).",
            "Indica la ruta del audio o de su transcripción (.txt, .srt, .vtt, .json)."
        )));
    }
    let result = crate::saved_transcript(settings, path).ok_or_else(|| {
        anyhow!(tr!(
            "{shown} has not been transcribed yet: transcribe it first with transcribe_file.",
            "{shown} todavía no está transcrito: transcríbelo primero con transcribe_file."
        ))
    })?;
    Ok((Some(path.to_path_buf()), result))
}

/// Argumentos de `generate_document`, ya comprobados.
#[derive(Debug)]
struct DocArgs {
    kind: &'static str,
    path: PathBuf,
    context: Option<String>,
}

fn doc_args(args: &Value) -> Result<DocArgs> {
    let kind = match str_arg(args, "kind").map(str::to_ascii_lowercase).as_deref() {
        Some("summary" | "resumen") => "summary",
        Some("minutes" | "minuta" | "acta") => "minutes",
        Some(other) => return Err(anyhow!(tr!("Unknown kind “{other}”: use summary or minutes.", "Tipo «{other}» desconocido: usa summary o minutes."))),
        None => return Err(anyhow!(tr!("Missing kind (summary or minutes)", "Falta kind (summary o minutes)"))),
    };
    let p = str_arg(args, "path").ok_or_else(|| anyhow!(tr!("Missing path", "Falta path")))?;
    let path = absolute(p)?;
    if !path.is_file() {
        return Err(anyhow!(tr!("File not found: {p}", "No existe el archivo {p}")));
    }
    let context = str_arg(args, "context").map(|c| c.chars().take(MAX_CONTEXT_CHARS).collect());
    Ok(DocArgs { kind, path, context })
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
    let text = |en: &'static str, es: &'static str| json!({"type": "string", "description": lang::pick(en, es)});
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
                    "title": text("Meeting title (optional; shown in the app and used for the summary and minutes).", "Título de la reunión (opcional; se ve en la app y se usa para el resumen y la minuta)."),
                    "date": text("Meeting date and time, as text (optional).", "Fecha y hora de la reunión, como texto (opcional)."),
                    "place": text("Place or platform of the meeting (optional).", "Lugar o plataforma de la reunión (opcional)."),
                    "participants": {
                        "description": lang::pick("Participants (optional): a list of names, or one text separated by commas.", "Participantes (opcional): lista de nombres, o un texto separado por comas."),
                        "anyOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}}],
                    },
                    "notes": text("Additional notes about the meeting (optional).", "Notas adicionales de la reunión (opcional)."),
                },
                "required": ["path"],
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
        },
        {
            "name": "generate_document",
            "title": lang::pick("Write summary or minutes", "Redactar resumen o minuta"),
            "description": lang::pick(
                "Writes the summary or the minutes of an already transcribed recording with IureTranscribe's own prompts and the user's OpenRouter model (the same as the app's button), saves it next to the transcript (<name>_summary.md / <name>_minutes.md) and returns {kind, path, content}. It can take a few minutes. The meeting details given to transcribe_file (or saved in the app) are used unless context is given. Fails if IureTranscribe has no AI configured: then write it yourself from the transcript.",
                "Redacta el resumen o la minuta de una grabación ya transcrita con las instrucciones de IureTranscribe y el modelo de OpenRouter del usuario (lo mismo que el botón de la app), lo guarda junto a la transcripción (<nombre>_resumen.md / <nombre>_minuta.md) y devuelve {kind, path, content}. Puede tardar unos minutos. Usa los datos de la reunión dados a transcribe_file (o guardados en la app) salvo que pases context. Falla si IureTranscribe no tiene IA configurada: entonces redáctalo tú a partir de la transcripción."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": lang::pick(
                        "Absolute path of the audio or of its transcript (.txt, .srt, .vtt, .json).",
                        "Ruta absoluta del audio o de su transcripción (.txt, .srt, .vtt, .json).")},
                    "kind": {"type": "string", "enum": ["summary", "minutes"], "description": lang::pick("summary or minutes.", "summary (resumen) o minutes (minuta).")},
                    "context": text(
                        "Meeting details for the model (date, place, participants…), replacing the stored ones (optional).",
                        "Datos de la reunión para el modelo (fecha, lugar, participantes…), en lugar de los guardados (opcional)."),
                },
                "required": ["path", "kind"],
            },
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true},
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
            "name": "start_recording",
            "title": lang::pick("Start recording", "Empezar a grabar"),
            "description": lang::pick(
                "Starts recording a meeting (microphone and/or system audio, as set in the app) into the recordings folder. Only one at a time. The recording lives in this process: keep the client open and call stop_recording when done.",
                "Empieza a grabar una reunión (micrófono y/o audio del sistema, según la app) en la carpeta de grabaciones. Una a la vez. La grabación vive en este proceso: mantén abierto el cliente y llama a stop_recording al terminar."),
            "inputSchema": {"type": "object", "properties": {
                "title": {"type": "string", "description": lang::pick(
                    "Meeting name: the audio file is named after it (instead of «Recording <date>»).",
                    "Nombre de la reunión: el audio se llama así (en vez de «Grabación <fecha>»).")},
            }},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false},
        },
        {
            "name": "recording_status",
            "title": lang::pick("Recording status", "Estado de la grabación"),
            "description": lang::pick(
                "Whether it is recording, elapsed seconds (pauses excluded), input levels, paused and the file path.",
                "Si está grabando, segundos transcurridos (sin las pausas), niveles de entrada, si está en pausa y la ruta del archivo."),
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": read_only,
        },
        {
            "name": "pause_recording",
            "title": lang::pick("Pause or resume", "Pausar o seguir"),
            "description": lang::pick("Pauses (paused: true) or resumes (paused: false) the recording.", "Pausa (paused: true) o reanuda (paused: false) la grabación."),
            "inputSchema": {"type": "object", "properties": {"paused": {"type": "boolean"}}, "required": ["paused"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
        },
        {
            "name": "stop_recording",
            "title": lang::pick("Stop recording", "Detener la grabación"),
            "description": lang::pick(
                "Stops and saves the recording; returns its path and duration (then transcribe it with transcribe_file).",
                "Detiene y guarda la grabación; devuelve su ruta y duración (después transcríbela con transcribe_file)."),
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false},
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_tools_are_published() {
        let names: Vec<String> = tools()
            .as_array()
            .map(|a| a.iter().filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string)).collect())
            .unwrap_or_default();
        for n in ["start_recording", "recording_status", "pause_recording", "stop_recording", "transcribe_file", "generate_document"] {
            assert!(names.iter().any(|x| x == n), "falta {n}");
        }
    }

    #[test]
    fn generate_document_schema_requires_path_and_kind() {
        let tools = tools();
        let doc = tools.as_array().and_then(|a| a.iter().find(|t| t.get("name").and_then(Value::as_str) == Some("generate_document"))).cloned().unwrap_or_default();
        let required: Vec<&str> = doc.pointer("/inputSchema/required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        assert_eq!(required, ["path", "kind"]);
        let transcribe = tools.as_array().and_then(|a| a.iter().find(|t| t.get("name").and_then(Value::as_str) == Some("transcribe_file"))).cloned().unwrap_or_default();
        for k in ["title", "date", "place", "participants", "notes"] {
            assert!(transcribe.pointer(&format!("/inputSchema/properties/{k}")).is_some(), "falta {k}");
        }
    }

    #[test]
    fn generate_document_arguments_are_validated() {
        let file = std::env::temp_dir().join(format!("iuret-doc-args-{}.wav", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let f = file.to_string_lossy().into_owned();
        assert!(doc_args(&json!({"path": f})).is_err(), "sin kind");
        assert!(doc_args(&json!({"path": f, "kind": "poema"})).is_err());
        assert!(doc_args(&json!({"path": "relativo.wav", "kind": "summary"})).is_err());
        assert!(doc_args(&json!({"path": "/no/existe/nada.wav", "kind": "summary"})).is_err());
        assert!(doc_args(&json!({"kind": "summary"})).is_err());
        assert!(doc_args(&json!({"path": 5, "kind": "summary"})).is_err());
        let ok = doc_args(&json!({"path": f, "kind": "Minuta", "context": "x".repeat(MAX_CONTEXT_CHARS + 10)})).unwrap();
        assert_eq!(ok.kind, "minutes");
        assert_eq!(ok.context.map(|c| c.chars().count()), Some(MAX_CONTEXT_CHARS));
        assert_eq!(doc_args(&json!({"path": f, "kind": "summary", "context": "  "})).unwrap().context, None);
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn locate_transcript_needs_an_existing_transcript() {
        let dir = std::env::temp_dir().join(format!("iuret-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("Junta.wav");
        std::fs::write(&audio, b"x").unwrap();
        let settings = Settings::default();
        assert!(locate_transcript(&settings, &audio).is_err());
        std::fs::write(dir.join("Junta.srt"), "1\n00:00:00,000 --> 00:00:02,000\nHola\n\n").unwrap();
        let (a, r) = locate_transcript(&settings, &audio).unwrap();
        assert_eq!(a.as_deref(), Some(audio.as_path()));
        assert_eq!(r.base_name, "Junta");
        assert_eq!(r.text, "Hola");
        // Desde la transcripción se encuentra el audio.
        let (a, _) = locate_transcript(&settings, &dir.join("Junta.srt")).unwrap();
        assert_eq!(a.as_deref(), Some(audio.as_path()));
        assert!(locate_transcript(&settings, &dir.join("Junta.pdf")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_link_encodes_the_path() {
        assert_eq!(open_link(Path::new("/home/ana/Grabación 1.wav")), "iuretranscribe://open?path=/home/ana/Grabaci%C3%B3n%201.wav");
        assert_eq!(percent_encode("C:\\a&b=c?#"), "C%3A%5Ca%26b%3Dc%3F%23");
    }

    #[test]
    fn pausing_without_a_recording_is_an_error_not_a_panic() {
        let r = crate::recorder::Recorder::default();
        assert!(!r.status().active);
        assert!(r.set_paused(true).is_err());
        assert!(r.stop().is_err());
    }
}
