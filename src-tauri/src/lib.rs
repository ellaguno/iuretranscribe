mod audio;
mod llm;
mod models;
mod settings;
mod subtitles;
mod transcribe;

use models::{Downloads, ModelInfo};
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
    models_dir: PathBuf,
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
    models::list(&state.models_dir, &state.downloads)
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
    models::delete(&state.models_dir, &id)
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
    let model_path = models::model_path(&state.models_dir, &settings.model_id);
    if !model_path.is_file() {
        return Err(format!("El modelo «{}» no está descargado. Descárgalo en la sección Modelos.", settings.model_id));
    }
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

    let output_dir = resolve_output_dir(&settings, &input);
    std::fs::create_dir_all(&output_dir).map_err(|e| format!("No se pudo crear la carpeta de salida: {e}"))?;
    let base_name = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "transcripcion".into());
    let mut outputs = Vec::new();
    let mut formats = settings.formats.clone();
    if formats.is_empty() {
        formats.push("srt".into());
    }
    for fmt in formats {
        let (ext, content) = match fmt.as_str() {
            "srt" => ("srt", subtitles::to_srt(&out.segments)),
            "vtt" => ("vtt", subtitles::to_vtt(&out.segments)),
            "txt" => ("txt", subtitles::to_txt(&out.segments)),
            "json" => ("json", subtitles::to_json(&out.segments)),
            _ => continue,
        };
        let path = output_dir.join(format!("{base_name}.{ext}"));
        std::fs::write(&path, content).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
        outputs.push(OutputFile { format: fmt, path: path.to_string_lossy().into_owned() });
    }

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
    let user = format!("{instruction}\n\n{}", request.text);
    let content = llm::chat(&settings.openrouter_api_key, &settings.openrouter_model, &system, &user).await?;
    let path = PathBuf::from(&request.output_dir).join(format!("{}{suffix}", request.base_name));
    std::fs::write(&path, format!("{content}\n")).map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
    Ok(DocumentResult { kind: request.kind, content, path: path.to_string_lossy().into_owned() })
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
            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);
            app.manage(AppState {
                settings_path,
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
            cancel_job,
            generate_document,
            open_path,
            reveal_path,
            read_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar IureTranscribe");
}
