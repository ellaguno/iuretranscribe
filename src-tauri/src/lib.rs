mod audio;
mod iurefficient;
mod llm;
mod models;
mod recorder;
mod settings;
mod subtitles;
mod transcribe;

use models::{Downloads, ModelInfo};
use recorder::Recorder;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use subtitles::Segment;
use tauri::{AppHandle, Emitter, Manager, State};
use transcribe::EngineEvent;
use transcribe::Engine;

pub struct AppState {
    settings_path: PathBuf,
    jobs_path: PathBuf,
    models_dir: PathBuf,
    /// Carpeta de modelos incluidos en el instalador (recursos), si existe.
    bundled_models_dir: Option<PathBuf>,
    recordings_default: PathBuf,
    recorder: Arc<Recorder>,
    settings: Mutex<Settings>,
    downloads: Arc<Downloads>,
    engine: Arc<Engine>,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo {
    backend: &'static str,
    cpu_threads: usize,
    platform: &'static str,
    models_dir: String,
    settings_path: String,
    ffmpeg_available: bool,
    recordings_dir: String,
    version: &'static str,
    supported_extensions: &'static [&'static str],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileProbe {
    path: String,
    name: String,
    size_bytes: u64,
    duration_secs: Option<f64>,
    supported: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscribeRequest {
    job_id: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputFile {
    format: String,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptResult {
    job_id: String,
    segments: Vec<Segment>,
    text: String,
    audio_secs: f64,
    elapsed_secs: f64,
    outputs: Vec<OutputFile>,
    output_dir: String,
    base_name: String,
    detected_language: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentRequest {
    /// "summary" | "minutes"
    kind: String,
    text: String,
    output_dir: String,
    base_name: String,
    /// Datos aportados por el usuario (participantes, fecha, lugar…).
    #[serde(default)]
    context: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentResult {
    kind: String,
    content: String,
    path: String,
}

fn ffmpeg_available() -> bool {
    audio::hidden_command("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
fn system_info(state: State<'_, AppState>) -> SystemInfo {
    SystemInfo {
        backend: transcribe::backend_name(),
        cpu_threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        platform: std::env::consts::OS,
        models_dir: state.models_dir.to_string_lossy().into_owned(),
        settings_path: state.settings_path.to_string_lossy().into_owned(),
        ffmpeg_available: ffmpeg_available(),
        recordings_dir: recordings_dir(&state).to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION"),
        supported_extensions: audio::SUPPORTED_EXTENSIONS,
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    settings.save(&state.settings_path).map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn list_models(state: State<'_, AppState>) -> Vec<ModelInfo> {
    models::list(&state.models_dir, state.bundled_models_dir.as_deref(), &state.downloads)
}

#[tauri::command]
async fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    models::download(app, state.models_dir.clone(), id, state.downloads.clone()).await
}

#[tauri::command]
fn cancel_download(state: State<'_, AppState>, id: String) -> bool {
    state.downloads.cancel(&id)
}

#[tauri::command]
fn delete_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.unload();
    models::delete(&state.models_dir, &id)?;
    if let Some(b) = state.bundled_models_dir.as_deref() {
        if b.join(models::file_name(&id)).is_file() {
            return Err("Ese modelo viene incluido en el instalador; se eliminó la copia descargada pero seguirá disponible.".into());
        }
    }
    Ok(())
}

#[tauri::command]
fn unload_model(state: State<'_, AppState>) {
    state.engine.unload();
}

#[tauri::command]
async fn probe_files(paths: Vec<String>) -> Vec<FileProbe> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let path = PathBuf::from(&p);
                let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
                let supported = audio::SUPPORTED_EXTENSIONS.contains(&ext.as_str());
                FileProbe {
                    name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.clone()),
                    size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                    duration_secs: if supported { audio::probe_duration(&path) } else { None },
                    supported,
                    path: p,
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn resolve_output_dir(settings: &Settings, input: &Path) -> PathBuf {
    if settings.output_mode == "custom" {
        if let Some(d) = settings.output_dir.as_deref().filter(|d| !d.trim().is_empty()) {
            return PathBuf::from(d);
        }
    }
    input.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

#[tauri::command]
async fn transcribe_file(app: AppHandle, state: State<'_, AppState>, request: TranscribeRequest) -> Result<TranscriptResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let input = PathBuf::from(&request.path);
    if !input.is_file() {
        return Err(format!("No existe el archivo {}", input.display()));
    }
    if !models::is_known(&settings.model_id) {
        return Err(format!("Modelo desconocido: {}", settings.model_id));
    }
    let Some(model_path) = models::resolve(&state.models_dir, state.bundled_models_dir.as_deref(), &settings.model_id) else {
        return Err(format!("El modelo «{}» no está descargado. Descárgalo en la sección Modelos.", settings.model_id));
    };
    let cancel = Arc::new(AtomicBool::new(false));
    state.jobs.lock().unwrap().insert(request.job_id.clone(), cancel.clone());

    let engine = state.engine.clone();
    let app2 = app.clone();
    let job_id = request.job_id.clone();
    let opts = transcribe::Options {
        job_id: job_id.clone(),
        input: input.clone(),
        model_path,
        language: settings.language.clone(),
        translate: settings.translate,
        use_gpu: settings.use_gpu,
        threads: settings.threads,
        beam_size: settings.beam_size.max(1),
        initial_prompt: None,
    };
    let started = std::time::Instant::now();
    let sink: transcribe::EventSink = Arc::new(move |ev: EngineEvent| {
        let _ = match ev {
            EngineEvent::Progress(p) => app2.emit("job-progress", p),
            EngineEvent::Segment(s) => app2.emit("job-segment", s),
        };
    });
    let result = tauri::async_runtime::spawn_blocking(move || engine.run(sink, opts, cancel))
        .await
        .map_err(|e| format!("Fallo interno: {e}"))?;
    state.jobs.lock().unwrap().remove(&job_id);
    let out = result.map_err(|e| format!("{e:#}"))?;
    let elapsed_secs = started.elapsed().as_secs_f64();

    let (outputs, output_dir, base_name) = write_outputs(&settings, &input, &out.segments)?;

    Ok(TranscriptResult {
        job_id,
        text: subtitles::to_plain(&out.segments),
        segments: out.segments,
        audio_secs: out.audio_secs,
        elapsed_secs,
        outputs,
        output_dir: output_dir.to_string_lossy().into_owned(),
        base_name,
        detected_language: out.detected_language,
    })
}

/// Escribe los formatos de salida configurados junto al archivo (o en la carpeta fija).
fn write_outputs(settings: &Settings, input: &Path, segments: &[Segment]) -> Result<(Vec<OutputFile>, PathBuf, String), String> {
    let output_dir = resolve_output_dir(settings, input);
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("No se pudo crear la carpeta de salida: {e}"))?;
    let base_name = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "transcripcion".into());
    let mut outputs = Vec::new();
    let mut formats = settings.formats.clone();
    if formats.is_empty() {
        formats.push("srt".into());
    }
    for fmt in formats {
        let (ext, content) = match fmt.as_str() {
            "srt" => ("srt", subtitles::to_srt(segments)),
            "vtt" => ("vtt", subtitles::to_vtt(segments)),
            "txt" => ("txt", subtitles::to_txt(segments)),
            "json" => ("json", subtitles::to_json(segments)),
            _ => continue,
        };
        let path = output_dir.join(format!("{base_name}.{ext}"));
        std::fs::write(&path, content).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
        outputs.push(OutputFile { format: fmt, path: path.to_string_lossy().into_owned() });
    }
    Ok((outputs, output_dir, base_name))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LiveTranscriptRequest {
    job_id: String,
    path: String,
    segments: Vec<Segment>,
    audio_secs: f64,
    elapsed_secs: f64,
}

/// Convierte la transcripción en vivo de una grabación en el resultado final
/// (escribe SRT/TXT/… sin volver a transcribir).
#[tauri::command]
fn save_live_transcript(state: State<'_, AppState>, request: LiveTranscriptRequest) -> Result<TranscriptResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let input = PathBuf::from(&request.path);
    if request.segments.is_empty() {
        return Err("La transcripción en vivo está vacía.".into());
    }
    let (outputs, output_dir, base_name) = write_outputs(&settings, &input, &request.segments)?;
    Ok(TranscriptResult {
        job_id: request.job_id,
        text: subtitles::to_plain(&request.segments),
        segments: request.segments,
        audio_secs: request.audio_secs,
        elapsed_secs: request.elapsed_secs,
        outputs,
        output_dir: output_dir.to_string_lossy().into_owned(),
        base_name,
        detected_language: None,
    })
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: String) -> bool {
    match state.jobs.lock().unwrap().get(&job_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

#[tauri::command]
async fn generate_document(state: State<'_, AppState>, request: DocumentRequest) -> Result<DocumentResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let (system, instruction, suffix) = match request.kind.as_str() {
        "summary" => (settings.summary_prompt.clone(), "Resume la siguiente transcripción:", "_resumen.md"),
        "minutes" => (settings.minutes_prompt.clone(), "Redacta la minuta de la siguiente transcripción:", "_minuta.md"),
        other => return Err(format!("Tipo de documento desconocido: {other}")),
    };
    if request.text.trim().is_empty() {
        return Err("La transcripción está vacía.".into());
    }
    let user = match request.context.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(ctx) => format!(
            "{instruction}\n\nDatos proporcionados por el usuario sobre la reunión (úsalos y dales prioridad sobre lo que se infiera del audio):\n{ctx}\n\nTranscripción:\n\n{}",
            request.text
        ),
        None => format!("{instruction}\n\n{}", request.text),
    };
    let content = llm::chat(&settings.openrouter_api_key, &settings.openrouter_model, &system, &user).await?;
    let path = PathBuf::from(&request.output_dir).join(format!("{}{suffix}", request.base_name));
    std::fs::write(&path, format!("{content}\n")).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
    Ok(DocumentResult { kind: request.kind, content, path: path.to_string_lossy().into_owned() })
}

/// Lista de trabajos serializada por el frontend (se restaura al reiniciar).
#[tauri::command]
fn load_jobs(state: State<'_, AppState>) -> String {
    std::fs::read_to_string(&state.jobs_path).unwrap_or_else(|_| "[]".into())
}

#[tauri::command]
fn save_jobs(state: State<'_, AppState>, json: String) -> Result<(), String> {
    let tmp = state.jobs_path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &state.jobs_path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentsOnDisk {
    summary: Option<DocumentResult>,
    minutes: Option<DocumentResult>,
}

/// Recupera resumen y minuta ya generados junto a la transcripción, si existen.
#[tauri::command]
fn load_documents(output_dir: String, base_name: String) -> DocumentsOnDisk {
    let read = |suffix: &str, kind: &str| {
        let path = PathBuf::from(&output_dir).join(format!("{base_name}{suffix}"));
        std::fs::read_to_string(&path)
            .ok()
            .filter(|c| !c.trim().is_empty())
            .map(|content| DocumentResult { kind: kind.into(), content, path: path.to_string_lossy().into_owned() })
    };
    DocumentsOnDisk { summary: read("_resumen.md", "summary"), minutes: read("_minuta.md", "minutes") }
}

fn recordings_dir(state: &AppState) -> PathBuf {
    let settings = state.settings.lock().unwrap();
    match settings.recordings_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => state.recordings_default.clone(),
    }
}

#[tauri::command]
fn list_audio_devices() -> recorder::DeviceList {
    recorder::list_devices()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StartRecordingInfo {
    path: String,
    live: bool,
    live_note: Option<String>,
}

#[tauri::command]
async fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<StartRecordingInfo, String> {
    let settings = state.settings.lock().unwrap().clone();
    let mut live_note = None;
    let live = if settings.live_transcription {
        if let Some(model_path) = models::resolve(&state.models_dir, state.bundled_models_dir.as_deref(), &settings.model_id) {
            let app = app.clone();
            Some(recorder::LiveOptions {
                engine: state.engine.clone(),
                options: transcribe::Options {
                    job_id: "live".into(),
                    input: PathBuf::new(),
                    model_path,
                    language: settings.language.clone(),
                    translate: settings.translate,
                    use_gpu: settings.use_gpu,
                    threads: settings.threads,
                    beam_size: 1,
                    initial_prompt: None,
                },
                chunk_secs: settings.live_chunk_secs.clamp(3.0, 30.0),
                on_segment: Arc::new(move |seg| {
                    let _ = app.emit("live-segment", seg);
                }),
            })
        } else {
            live_note = Some("Transcripción en vivo desactivada: el modelo seleccionado no está descargado.".into());
            None
        }
    } else {
        None
    };
    let is_live = live.is_some();
    let opts = recorder::StartOptions {
        capture_mic: settings.record_mic,
        mic_device: settings.mic_device.clone(),
        capture_system: settings.record_system,
        output_dir: recordings_dir(&state),
        live,
    };
    let rec = state.recorder.clone();
    tauri::async_runtime::spawn_blocking(move || rec.start(opts))
        .await
        .map_err(|e| e.to_string())?
        .map(|p| StartRecordingInfo { path: p.to_string_lossy().into_owned(), live: is_live, live_note })
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn stop_recording(state: State<'_, AppState>) -> Result<recorder::RecordingResult, String> {
    let rec = state.recorder.clone();
    tauri::async_runtime::spawn_blocking(move || rec.stop())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn recording_status(state: State<'_, AppState>) -> recorder::RecordingStatus {
    state.recorder.status()
}

// ---------------------------------------------------------------------------
// Iurefficient (fase 0: WebDAV con contraseña de aplicación)
// ---------------------------------------------------------------------------
fn iure_account(state: &AppState) -> Result<iurefficient::Account, String> {
    let s = state.settings.lock().unwrap().clone();
    iurefficient::Account::new(&s.iure_domain, &s.iure_email, &s.iure_app_password).map_err(|e| e.to_string())
}

#[tauri::command]
async fn iure_test_connection(state: State<'_, AppState>) -> Result<iurefficient::ConnectionInfo, String> {
    let acc = iure_account(&state)?;
    iurefficient::test_connection(&acc).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn iure_list(state: State<'_, AppState>, folder: String) -> Result<iurefficient::Listing, String> {
    let acc = iure_account(&state)?;
    iurefficient::list(&acc, &folder).await.map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureUploadRequest {
    job_id: String,
    folder: String,
    /// Rutas locales a subir, en orden.
    files: Vec<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct IureUploadProgress {
    job_id: String,
    file_name: String,
    index: usize,
    total_files: usize,
    sent: u64,
    total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureUploadResult {
    folder: String,
    web_url: String,
    uploaded: Vec<iurefficient::Uploaded>,
}

#[tauri::command]
async fn iure_upload(app: AppHandle, state: State<'_, AppState>, request: IureUploadRequest) -> Result<IureUploadResult, String> {
    let acc = iure_account(&state)?;
    let total_files = request.files.len();
    let mut uploaded = Vec::new();
    for (index, f) in request.files.iter().enumerate() {
        let local = PathBuf::from(f);
        let file_name = local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let app2 = app.clone();
        let job_id = request.job_id.clone();
        let fname = file_name.clone();
        let last = Arc::new(Mutex::new(std::time::Instant::now()));
        let progress = move |sent: u64, total: u64| {
            let mut last = last.lock().unwrap();
            if last.elapsed().as_millis() > 120 || sent == total {
                *last = std::time::Instant::now();
                let _ = app2.emit("iure-upload-progress", IureUploadProgress { job_id: job_id.clone(), file_name: fname.clone(), index, total_files, sent, total });
            }
        };
        let res = match iurefficient::upload(&acc, &request.folder, &local, None, progress.clone()).await {
            Ok(r) => r,
            // Extensión rechazada (p. ej. .srt): reintenta como .txt para no perder la transcripción.
            Err(e) if e.to_string().contains("403") => match iurefficient::fallback_name(&file_name) {
                Some(alt) => {
                    let mut r = iurefficient::upload(&acc, &request.folder, &local, Some(&alt), progress).await.map_err(|e| format!("{e:#}"))?;
                    r.renamed_from = Some(file_name.clone());
                    r
                }
                None => return Err(format!("{e:#}")),
            },
            Err(e) => return Err(format!("{e:#}")),
        };
        uploaded.push(res);
    }
    // Recuerda la carpeta para la próxima vez.
    {
        let mut s = state.settings.lock().unwrap();
        s.iure_last_folder = Some(request.folder.clone());
        let _ = s.save(&state.settings_path);
    }
    Ok(IureUploadResult { folder: request.folder, web_url: acc.web_url(), uploaded })
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("No se pudo leer {path}: {e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,whisper_rs=warn")).init();
    whisper_rs::install_logging_hooks();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let bundled_models_dir = app.path().resource_dir().ok().map(|r| r.join("models")).filter(|d| d.is_dir());
            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);
            app.manage(AppState {
                settings_path,
                jobs_path: data_dir.join("jobs.json"),
                bundled_models_dir,
                recordings_default: dirs::audio_dir().unwrap_or_else(|| data_dir.clone()).join("IureTranscribe"),
                recorder: Arc::new(Recorder::default()),
                models_dir,
                settings: Mutex::new(settings),
                downloads: Arc::new(Downloads::default()),
                engine: Arc::new(Engine::default()),
                jobs: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_info,
            get_settings,
            save_settings,
            list_models,
            download_model,
            cancel_download,
            delete_model,
            unload_model,
            probe_files,
            transcribe_file,
            save_live_transcript,
            cancel_job,
            generate_document,
            load_jobs,
            save_jobs,
            load_documents,
            iure_test_connection,
            iure_list,
            iure_upload,
            list_audio_devices,
            start_recording,
            stop_recording,
            recording_status,
            open_path,
            reveal_path,
            read_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar IureTranscribe");
}
