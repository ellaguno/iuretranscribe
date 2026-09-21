mod audio;
mod llm;
mod models;
mod recorder;
mod settings;
mod subtitles;
mod transcribe;

use iurefficient_connect::{api, rest::{Login, Session, SessionExport}, secrets, webdav::{self, WebDav}, Account};
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
    /// Sesión REST de Iurefficient (se restaura del llavero al primer uso).
    iure_session: tokio::sync::Mutex<Option<Arc<Session>>>,
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
    /// Identificador de plataforma y variante para el actualizador (p. ej. `linux-x86_64-cuda`).
    update_target: String,
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
        update_target: update_target(),
        supported_extensions: audio::SUPPORTED_EXTENSIONS,
    }
}

/// `os-arch[-variante]`: las variantes con GPU se actualizan sólo con su propio instalador.
fn update_target() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let variant = match transcribe::backend_name() {
        "CUDA" => "-cuda",
        "Vulkan" => "-vulkan",
        _ => "",
    };
    format!("{os}-{}{variant}", std::env::consts::ARCH)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateNotice {
    version: String,
    url: String,
}

/// Aviso de versión nueva en GitHub (sin instalar): funciona para cualquier instalador.
#[tauri::command]
async fn check_update_notice() -> Result<Option<UpdateNotice>, String> {
    let r = iurefficient_connect::releases::consultar("ellaguno/iuretranscribe", env!("CARGO_PKG_VERSION"), &iure_user_agent())
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(r.map(|r| UpdateNotice { version: r.version, url: r.url }))
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    let mut s = state.settings.lock().unwrap().clone();
    if s.iure_app_password.is_empty() {
        if let Ok(acc) = Account::new(&s.iure_domain, &s.iure_email) {
            if let Ok(Some(p)) = secrets::leer(&acc, secrets::Kind::WebDav) {
                s.iure_app_password = p;
            }
        }
    }
    s
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let mut to_disk = settings.clone();
    // La contraseña WebDAV va al llavero del sistema (compartido con IureDav); en el
    // archivo sólo queda si el llavero no está disponible.
    if let Ok(acc) = Account::new(&settings.iure_domain, &settings.iure_email) {
        if settings.iure_app_password.trim().is_empty() {
            // Campo vaciado por el usuario: se elimina también del llavero.
            let _ = secrets::borrar(&acc, secrets::Kind::WebDav);
        } else if secrets::guardar(&acc, secrets::Kind::WebDav, settings.iure_app_password.trim()).is_ok() {
            to_disk.iure_app_password.clear();
        }
    }
    to_disk.save(&state.settings_path).map_err(|e| e.to_string())?;
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
async fn start_recording(app: AppHandle, state: State<'_, AppState>, speakers: Option<[String; 2]>) -> Result<StartRecordingInfo, String> {
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
                speakers: if settings.speaker_split { speakers } else { None },
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
fn iure_user_agent() -> String {
    iurefficient_connect::user_agent("IureTranscribe", env!("CARGO_PKG_VERSION"))
}

/// Cuenta configurada y contraseña WebDAV: primero el llavero del sistema
/// (compartido con IureDav), después el ajuste local por compatibilidad.
fn iure_webdav(state: &AppState) -> Result<WebDav, String> {
    let s = state.settings.lock().unwrap().clone();
    let acc = Account::new(&s.iure_domain, &s.iure_email).map_err(|e| e.to_string())?;
    let password = match secrets::leer(&acc, secrets::Kind::WebDav) {
        Ok(Some(p)) => p,
        _ => s.iure_app_password.clone(),
    };
    WebDav::new(acc, &password, &iure_user_agent()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn iure_test_connection(state: State<'_, AppState>) -> Result<iurefficient_connect::webdav::ConnectionInfo, String> {
    let dav = iure_webdav(&state)?;
    dav.test_connection().await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn iure_list(state: State<'_, AppState>, folder: String) -> Result<iurefficient_connect::webdav::Listing, String> {
    let dav = iure_webdav(&state)?;
    dav.list(&folder).await.map_err(|e| format!("{e:#}"))
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
    uploaded: Vec<iurefficient_connect::webdav::Uploaded>,
}

#[tauri::command]
async fn iure_upload(app: AppHandle, state: State<'_, AppState>, request: IureUploadRequest) -> Result<IureUploadResult, String> {
    let dav = iure_webdav(&state)?;
    let total_files = request.files.len();
    let mut uploaded = Vec::new();
    for (index, f) in request.files.iter().enumerate() {
        let local = PathBuf::from(f);
        let file_name = local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let app2 = app.clone();
        let job_id = request.job_id.clone();
        let last = Arc::new(Mutex::new(std::time::Instant::now()));
        let progress = move |sent: u64, total: u64| {
            let mut last = last.lock().unwrap();
            if last.elapsed().as_millis() > 120 || sent == total {
                *last = std::time::Instant::now();
                let _ = app2.emit("iure-upload-progress", IureUploadProgress { job_id: job_id.clone(), file_name: file_name.clone(), index, total_files, sent, total });
            }
        };
        let res = dav.upload_with_fallback(&request.folder, &local, progress).await.map_err(|e| format!("{e:#}"))?;
        uploaded.push(res);
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.iure_last_folder = Some(request.folder.clone());
        let _ = s.save(&state.settings_path);
    }
    Ok(IureUploadResult { folder: request.folder, web_url: dav.acc.web_url(), uploaded })
}

// ---------------------------------------------------------------------------
// Iurefficient (fase 1): sesión REST, proyectos, minutas con el motor, CRM, horas
// ---------------------------------------------------------------------------
fn iure_account(state: &AppState) -> Result<Account, String> {
    let s = state.settings.lock().unwrap().clone();
    Account::new(&s.iure_domain, &s.iure_email).map_err(|e| e.to_string())
}

fn persist_session(acc: &Account, sess: &Session) {
    if let Ok(json) = serde_json::to_string(&sess.export()) {
        let _ = secrets::guardar(acc, secrets::Kind::Session, &json);
    }
}

/// Sesión REST viva: la cacheada, o la restaurada del llavero.
async fn iure_session(state: &AppState) -> Result<Arc<Session>, String> {
    if let Some(s) = state.iure_session.lock().await.as_ref() {
        return Ok(s.clone());
    }
    let acc = iure_account(state)?;
    let saved = secrets::leer(&acc, secrets::Kind::Session)
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str::<SessionExport>(&j).ok())
        .ok_or_else(|| "Inicia sesión en Iurefficient desde Ajustes".to_string())?;
    let sess = Session::new(acc.clone(), &iure_user_agent()).map_err(|e| e.to_string())?;
    sess.import(&saved).await.map_err(|e| format!("{e:#}"))?;
    persist_session(&acc, &sess);
    let sess = Arc::new(sess);
    *state.iure_session.lock().await = Some(sess.clone());
    Ok(sess)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureLoginResult {
    logged_in: bool,
    requires_totp: bool,
    totp_token: Option<String>,
    name: Option<String>,
}

#[tauri::command]
async fn iure_login(state: State<'_, AppState>, password: String, totp_code: Option<String>, totp_token: Option<String>) -> Result<IureLoginResult, String> {
    let acc = iure_account(&state)?;
    let sess = Session::new(acc.clone(), &iure_user_agent()).map_err(|e| e.to_string())?;
    let user = match (totp_token, totp_code) {
        (Some(t), Some(code)) if !t.is_empty() => sess.verify_totp(&t, &code).await.map_err(|e| format!("{e:#}"))?,
        _ => match sess.login(&password).await.map_err(|e| format!("{e:#}"))? {
            Login::Ok(u) => u,
            Login::TotpRequired { totp_token } => {
                return Ok(IureLoginResult { logged_in: false, requires_totp: true, totp_token: Some(totp_token), name: None });
            }
        },
    };
    persist_session(&acc, &sess);
    *state.iure_session.lock().await = Some(Arc::new(sess));
    let name = user.name.clone().or_else(|| user.extra.get("full_name").and_then(|v| v.as_str()).map(str::to_string)).or(Some(user.email.clone()));
    Ok(IureLoginResult { logged_in: true, requires_totp: false, totp_token: None, name })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureSessionStatus {
    logged_in: bool,
    name: Option<String>,
    email: Option<String>,
    crm: bool,
    terminology: Option<api::Terminology>,
    error: Option<String>,
}

#[tauri::command]
async fn iure_session_status(state: State<'_, AppState>) -> Result<IureSessionStatus, String> {
    let sess = match iure_session(&state).await {
        Ok(s) => s,
        Err(e) => return Ok(IureSessionStatus { logged_in: false, name: None, email: None, crm: false, terminology: None, error: Some(e) }),
    };
    match sess.me().await {
        Ok(u) => {
            let crm = api::crm_available(&sess).await.unwrap_or(false);
            let terminology = api::terminology(&sess).await.ok();
            let name = u.name.clone().or_else(|| u.extra.get("full_name").and_then(|v| v.as_str()).map(str::to_string));
            Ok(IureSessionStatus { logged_in: true, name, email: Some(u.email), crm, terminology, error: None })
        }
        Err(e) => {
            *state.iure_session.lock().await = None;
            Ok(IureSessionStatus { logged_in: false, name: None, email: None, crm: false, terminology: None, error: Some(format!("{e:#}")) })
        }
    }
}

#[tauri::command]
async fn iure_logout(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(s) = state.iure_session.lock().await.take() {
        let _ = s.logout().await;
    }
    if let Ok(acc) = iure_account(&state) {
        let _ = secrets::borrar(&acc, secrets::Kind::Session);
    }
    Ok(())
}

/// Garantiza una contraseña de aplicación WebDAV: si no hay ninguna en el llavero ni en
/// los ajustes, la crea con la sesión iniciada (a nombre de esta máquina) y la guarda.
#[tauri::command]
async fn iure_ensure_webdav_password(state: State<'_, AppState>) -> Result<bool, String> {
    let acc = iure_account(&state)?;
    let existing = secrets::leer(&acc, secrets::Kind::WebDav).ok().flatten().filter(|p| !p.trim().is_empty())
        .or_else(|| Some(state.settings.lock().unwrap().iure_app_password.clone()).filter(|p| !p.trim().is_empty()));
    if existing.is_some() {
        return Ok(false);
    }
    let sess = iure_session(&state).await?;
    let host = hostname_label();
    let created = api::create_webdav_token(&sess, &format!("IureTranscribe en {host}"), None).await.map_err(|e| format!("{e:#}"))?;
    if secrets::guardar(&acc, secrets::Kind::WebDav, &created.secret).is_err() {
        // Sin llavero: se conserva en los ajustes.
        let mut s = state.settings.lock().unwrap();
        s.iure_app_password = created.secret.clone();
        let _ = s.save(&state.settings_path);
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.iure_app_password = created.secret;
    }
    Ok(true)
}

fn hostname_label() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_string()))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "este equipo".into())
}

#[tauri::command]
async fn iure_search_cases(state: State<'_, AppState>, query: String) -> Result<Vec<api::CaseSummary>, String> {
    let sess = iure_session(&state).await?;
    api::cases(&sess, Some(&query), 50).await.map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureCaseUploadRequest {
    job_id: String,
    /// None = documento sin proyecto (queda en «General»).
    case_id: Option<String>,
    files: Vec<String>,
    /// Ruta local de la transcripción (para marcarla como fuente de la minuta).
    transcript_path: Option<String>,
    /// Horas a registrar en el proyecto (None = no registrar).
    hours: Option<f64>,
    hours_description: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct IureDocRef {
    id: String,
    file_name: String,
    is_transcript: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureCaseUploadResult {
    documents: Vec<IureDocRef>,
    time_entry_id: Option<String>,
    web_url: String,
}

fn remote_name_for(local: &Path) -> Option<String> {
    let name = local.file_name()?.to_string_lossy().into_owned();
    Some(webdav::fallback_name(&name).unwrap_or(name))
}

#[tauri::command]
async fn iure_upload_to_case(app: AppHandle, state: State<'_, AppState>, request: IureCaseUploadRequest) -> Result<IureCaseUploadResult, String> {
    let sess = iure_session(&state).await?;
    let total_files = request.files.len();
    let mut documents = Vec::new();
    for (index, f) in request.files.iter().enumerate() {
        let local = PathBuf::from(f);
        let file_name = remote_name_for(&local).unwrap_or_default();
        let _ = app.emit("iure-upload-progress", IureUploadProgress { job_id: request.job_id.clone(), file_name: file_name.clone(), index, total_files, sent: 0, total: 0 });
        let is_transcript = request.transcript_path.as_deref() == Some(f.as_str());
        let opts = api::UploadOptions {
            case_id: request.case_id.clone(),
            file_name: Some(file_name.clone()),
            document_type: Some(if is_transcript { "transcript".into() } else { "other".into() }),
            tags: vec!["iuretranscribe".into()],
            ..Default::default()
        };
        let doc = api::upload_document(&sess, &local, &opts).await.map_err(|e| format!("{file_name}: {e:#}"))?;
        documents.push(IureDocRef { id: doc.id, file_name, is_transcript });
    }
    let time_entry_id = match (request.hours, request.case_id.as_deref()) {
        (Some(h), Some(case_id)) if h > 0.0 => Some(
            api::add_time_entry(&sess, case_id, h, request.hours_description.as_deref().unwrap_or("Reunión transcrita con IureTranscribe"), true, None)
                .await
                .map_err(|e| format!("Documentos subidos, pero no se registraron las horas: {e:#}"))?,
        ),
        _ => None,
    };
    Ok(IureCaseUploadResult { documents, time_entry_id, web_url: sess.account().web_url() })
}

#[tauri::command]
async fn iure_ai_options(state: State<'_, AppState>, document_id: String) -> Result<api::AiOptions, String> {
    let sess = iure_session(&state).await?;
    api::ai_options(&sess, &document_id).await.map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureComposeRequest {
    blueprint_id: String,
    case_id: Option<String>,
    source_document_ids: Vec<String>,
    title: Option<String>,
    attendees: Vec<String>,
    extra_instructions: Option<String>,
}

#[tauri::command]
async fn iure_compose(state: State<'_, AppState>, request: IureComposeRequest) -> Result<String, String> {
    let sess = iure_session(&state).await?;
    let req = api::ComposeRequest {
        case_id: request.case_id,
        source_document_ids: request.source_document_ids,
        title: request.title,
        extra_instructions: request.extra_instructions,
        attendees: request.attendees,
        ..Default::default()
    };
    api::compose(&sess, &request.blueprint_id, &req).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn iure_compose_status(state: State<'_, AppState>, task_id: String) -> Result<api::ComposeStatus, String> {
    let sess = iure_session(&state).await?;
    api::compose_status(&sess, &task_id).await.map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureDownloadRequest {
    document_id: String,
    /// Carpeta local donde guardar (normalmente la de salida del trabajo).
    target_dir: String,
    /// Nombre base sin extensión; la extensión se toma del documento real (.md, .docx…).
    base_name: String,
}

/// Descarga un documento de la instancia (p. ej. la minuta generada) a la carpeta de salida.
#[tauri::command]
async fn iure_download_document(state: State<'_, AppState>, request: IureDownloadRequest) -> Result<String, String> {
    let sess = iure_session(&state).await?;
    let dir = PathBuf::from(&request.target_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let info = api::document(&sess, &request.document_id).await.map_err(|e| format!("{e:#}"))?;
    let ext = Path::new(&info.file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .or_else(|| match info.mime_type.as_deref() {
            Some("text/markdown") => Some("md".into()),
            Some("text/plain") => Some("txt".into()),
            Some(m) if m.contains("wordprocessingml") => Some("docx".into()),
            Some("application/pdf") => Some("pdf".into()),
            _ => None,
        })
        .unwrap_or_else(|| "md".into());
    let safe: String = request.base_name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
    let path = dir.join(format!("{safe}.{ext}"));
    api::download_document(&sess, &request.document_id, &path).await.map_err(|e| format!("{e:#}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureSummaryRequest {
    document_id: String,
    output_dir: String,
    base_name: String,
}

/// Resumen con la IA de la instancia, usando las instrucciones de resumen de la app
/// como `system_prompt` y la transcripción subida como documento de contexto.
#[tauri::command]
async fn iure_summary_via_chat(state: State<'_, AppState>, request: IureSummaryRequest) -> Result<DocumentResult, String> {
    let sess = iure_session(&state).await?;
    let prompt = state.settings.lock().unwrap().summary_prompt.clone();
    let content = api::global_chat(
        &sess,
        "Resume la transcripción del documento adjunto siguiendo tus instrucciones. Responde sólo con el resumen en Markdown.",
        Some(&prompt),
        &[request.document_id.clone()],
    )
    .await
    .map_err(|e| format!("{e:#}"))?;
    let path = PathBuf::from(&request.output_dir).join(format!("{}_resumen.md", request.base_name));
    std::fs::write(&path, format!("{content}\n")).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
    Ok(DocumentResult { kind: "summary".into(), content, path: path.to_string_lossy().into_owned() })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureCommitments {
    commitments: Vec<api::Commitment>,
    case_id: Option<String>,
}

/// Compromisos detectados por la instancia en una minuta (documento de la instancia).
#[tauri::command]
async fn iure_commitments(state: State<'_, AppState>, document_id: String) -> Result<IureCommitments, String> {
    let sess = iure_session(&state).await?;
    let (commitments, case_id) = api::commitments(&sess, &document_id).await.map_err(|e| format!("{e:#}"))?;
    Ok(IureCommitments { commitments, case_id })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureApplyRequest {
    document_id: String,
    case_id: Option<String>,
    commitments: Vec<api::Commitment>,
}

/// Convierte compromisos en tareas del proyecto. Devuelve cuántas se crearon.
#[tauri::command]
async fn iure_apply_commitments(state: State<'_, AppState>, request: IureApplyRequest) -> Result<u64, String> {
    let sess = iure_session(&state).await?;
    api::apply_commitments(&sess, &request.document_id, &request.commitments, request.case_id.as_deref()).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn iure_crm_search(state: State<'_, AppState>, kind: String, query: String) -> Result<Vec<api::CrmItem>, String> {
    let sess = iure_session(&state).await?;
    let k = if kind == "lead" { api::CrmKind::Lead } else { api::CrmKind::Opportunity };
    api::crm_list(&sess, k, Some(&query), 100).await.map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IureCrmUploadRequest {
    job_id: String,
    kind: String,
    id: String,
    files: Vec<String>,
    activity_subject: Option<String>,
    activity_description: Option<String>,
    duration_minutes: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IureCrmUploadResult {
    documents: Vec<IureDocRef>,
    activity_id: Option<String>,
    web_url: String,
}

#[tauri::command]
async fn iure_upload_to_crm(app: AppHandle, state: State<'_, AppState>, request: IureCrmUploadRequest) -> Result<IureCrmUploadResult, String> {
    let sess = iure_session(&state).await?;
    let kind = if request.kind == "lead" { api::CrmKind::Lead } else { api::CrmKind::Opportunity };
    let total_files = request.files.len();
    let mut documents = Vec::new();
    for (index, f) in request.files.iter().enumerate() {
        let local = PathBuf::from(f);
        let file_name = remote_name_for(&local).unwrap_or_default();
        let _ = app.emit("iure-upload-progress", IureUploadProgress { job_id: request.job_id.clone(), file_name: file_name.clone(), index, total_files, sent: 0, total: 0 });
        let doc = api::crm_attach_file(&sess, kind, &request.id, &local, Some(&file_name)).await.map_err(|e| format!("{file_name}: {e:#}"))?;
        documents.push(IureDocRef { id: doc.id, file_name, is_transcript: false });
    }
    let activity_id = match request.activity_subject {
        Some(subject) if !subject.trim().is_empty() => {
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            Some(
                api::crm_activity(&sess, kind, &request.id, "meeting", &subject, request.activity_description.as_deref(), request.duration_minutes, Some(&now))
                    .await
                    .map_err(|e| format!("Archivos adjuntados, pero no se registró la actividad: {e:#}"))?,
            )
        }
        _ => None,
    };
    Ok(IureCrmUploadResult { documents, activity_id, web_url: sess.account().web_url() })
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let bundled_models_dir = app.path().resource_dir().ok().map(|r| r.join("models")).filter(|d| d.is_dir());
            let settings_path = config_dir.join("settings.json");
            let mut settings = Settings::load(&settings_path);
            if settings.iure_app_password.is_empty() {
                if let Ok(acc) = Account::new(&settings.iure_domain, &settings.iure_email) {
                    if let Ok(Some(p)) = secrets::leer(&acc, secrets::Kind::WebDav) {
                        settings.iure_app_password = p;
                    }
                }
            }
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
                iure_session: tokio::sync::Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_info,
            check_update_notice,
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
            iure_login,
            iure_session_status,
            iure_logout,
            iure_ensure_webdav_password,
            iure_search_cases,
            iure_upload_to_case,
            iure_ai_options,
            iure_compose,
            iure_compose_status,
            iure_download_document,
            iure_summary_via_chat,
            iure_commitments,
            iure_apply_commitments,
            iure_crm_search,
            iure_upload_to_crm,
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
