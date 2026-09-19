//! Catálogo de modelos GGML de whisper.cpp y su descarga desde Hugging Face.

use futures_util::StreamExt;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub size_mb: u64,
    pub description: String,
    /// "alta" | "media" | "baja"
    pub quality: String,
    pub recommended: bool,
    pub downloaded: bool,
    pub downloading: bool,
    /// Viene incluido en el instalador (no se puede borrar).
    pub bundled: bool,
    pub path: String,
}

struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    size_mb: u64,
    quality: &'static str,
    recommended: bool,
    description: &'static str,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry { id: "large-v3-turbo", name: "Large v3 Turbo", size_mb: 1620, quality: "alta", recommended: true,
        description: "El mejor equilibrio entre calidad y velocidad. Equivale al modelo que usa el script original." },
    CatalogEntry { id: "large-v3-turbo-q8_0", name: "Large v3 Turbo (Q8)", size_mb: 874, quality: "alta", recommended: false,
        description: "Turbo cuantizado a 8 bits: casi la misma calidad con la mitad de tamaño." },
    CatalogEntry { id: "large-v3-turbo-q5_0", name: "Large v3 Turbo (Q5)", size_mb: 574, quality: "alta", recommended: true,
        description: "Turbo cuantizado a 5 bits: ideal para equipos sin GPU o con poca memoria." },
    CatalogEntry { id: "large-v3", name: "Large v3", size_mb: 3100, quality: "alta", recommended: false,
        description: "Máxima precisión, pero mucho más lento. Requiere GPU con memoria suficiente." },
    CatalogEntry { id: "large-v3-q5_0", name: "Large v3 (Q5)", size_mb: 1080, quality: "alta", recommended: false,
        description: "Large v3 cuantizado. Muy preciso; lento sin GPU." },
    CatalogEntry { id: "medium", name: "Medium", size_mb: 1530, quality: "media", recommended: false,
        description: "Buena calidad en español; más lento que Turbo con calidad similar." },
    CatalogEntry { id: "medium-q5_0", name: "Medium (Q5)", size_mb: 539, quality: "media", recommended: false,
        description: "Medium cuantizado a 5 bits." },
    CatalogEntry { id: "small", name: "Small", size_mb: 488, quality: "media", recommended: false,
        description: "Rápido y ligero. Calidad aceptable para audio claro." },
    CatalogEntry { id: "base", name: "Base", size_mb: 148, quality: "baja", recommended: false,
        description: "Incluido en el instalador: funciona de inmediato sin descargar nada. Muy rápido; calidad básica, ideal para borradores." },
    CatalogEntry { id: "tiny", name: "Tiny", size_mb: 78, quality: "baja", recommended: false,
        description: "El más pequeño. Sólo para pruebas rápidas." },
];

pub fn file_name(id: &str) -> String {
    format!("ggml-{id}.bin")
}

pub fn model_url(id: &str) -> String {
    format!("{HF_BASE}/{}", file_name(id))
}

pub fn model_path(models_dir: &Path, id: &str) -> PathBuf {
    models_dir.join(file_name(id))
}

pub fn is_known(id: &str) -> bool {
    CATALOG.iter().any(|e| e.id == id)
}

fn file_ok(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 1_000_000).unwrap_or(false)
}

/// Ruta utilizable del modelo: la descargada o, si no existe, la incluida en el instalador.
pub fn resolve(models_dir: &Path, bundled_dir: Option<&Path>, id: &str) -> Option<PathBuf> {
    let downloaded = model_path(models_dir, id);
    if file_ok(&downloaded) {
        return Some(downloaded);
    }
    let bundled = bundled_dir?.join(file_name(id));
    file_ok(&bundled).then_some(bundled)
}

#[derive(Default)]
pub struct Downloads {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Downloads {
    fn start(&self, id: &str) -> Option<Arc<AtomicBool>> {
        let mut map = self.active.lock().unwrap();
        if map.contains_key(id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(id.to_string(), flag.clone());
        Some(flag)
    }
    fn finish(&self, id: &str) {
        self.active.lock().unwrap().remove(id);
    }
    pub fn cancel(&self, id: &str) -> bool {
        match self.active.lock().unwrap().get(id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }
    fn is_active(&self, id: &str) -> bool {
        self.active.lock().unwrap().contains_key(id)
    }
}

pub fn list(models_dir: &Path, bundled_dir: Option<&Path>, downloads: &Downloads) -> Vec<ModelInfo> {
    CATALOG
        .iter()
        .map(|e| {
            let bundled = bundled_dir.map(|d| file_ok(&d.join(file_name(e.id)))).unwrap_or(false);
            let path = resolve(models_dir, bundled_dir, e.id).unwrap_or_else(|| model_path(models_dir, e.id));
            let downloaded = file_ok(&path);
            ModelInfo {
                id: e.id.into(),
                name: e.name.into(),
                file_name: file_name(e.id),
                size_mb: e.size_mb,
                description: e.description.into(),
                quality: e.quality.into(),
                recommended: e.recommended,
                downloaded,
                downloading: downloads.is_active(e.id),
                bundled,
                path: path.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// "downloading" | "done" | "cancelled" | "error"
    pub status: String,
    pub message: Option<String>,
}

fn emit(app: &AppHandle, p: DownloadProgress) {
    let _ = app.emit("model-download-progress", p);
}

/// Descarga (o reanuda) un modelo. Emite eventos `model-download-progress`.
pub async fn download(app: AppHandle, models_dir: PathBuf, id: String, downloads: Arc<Downloads>) -> Result<(), String> {
    if !is_known(&id) {
        return Err(format!("Modelo desconocido: {id}"));
    }
    let Some(cancel) = downloads.start(&id) else {
        return Err("Ese modelo ya se está descargando".into());
    };
    let result = download_inner(&app, &models_dir, &id, cancel).await;
    downloads.finish(&id);
    match &result {
        Ok(()) => emit(&app, DownloadProgress { id: id.clone(), downloaded: 0, total: None, status: "done".into(), message: None }),
        Err(e) if e == "cancelled" => emit(&app, DownloadProgress { id: id.clone(), downloaded: 0, total: None, status: "cancelled".into(), message: None }),
        Err(e) => emit(&app, DownloadProgress { id: id.clone(), downloaded: 0, total: None, status: "error".into(), message: Some(e.clone()) }),
    }
    result.map_err(|e| if e == "cancelled" { "Descarga cancelada".to_string() } else { e })
}

async fn download_inner(app: &AppHandle, models_dir: &Path, id: &str, cancel: Arc<AtomicBool>) -> Result<(), String> {
    tokio::fs::create_dir_all(models_dir).await.map_err(|e| e.to_string())?;
    let final_path = model_path(models_dir, id);
    let part_path = models_dir.join(format!("{}.part", file_name(id)));
    let existing = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);

    let client = reqwest::Client::builder()
        .user_agent("IureTranscribe/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(model_url(id));
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await.map_err(|e| format!("No se pudo conectar con Hugging Face: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Hugging Face respondió {status}"));
    }
    let resuming = status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0;
    let mut downloaded = if resuming { existing } else { 0 };
    let total = resp.content_length().map(|l| l + if resuming { existing } else { 0 });

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resuming)
        .truncate(!resuming)
        .open(&part_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut stream = resp.bytes_stream();
    let mut last_emit = std::time::Instant::now();
    emit(app, DownloadProgress { id: id.into(), downloaded, total, status: "downloading".into(), message: None });
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            let _ = file.flush().await;
            return Err("cancelled".into());
        }
        let chunk = chunk.map_err(|e| format!("Error de red durante la descarga: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| format!("No se pudo escribir el archivo: {e}"))?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed().as_millis() > 150 {
            last_emit = std::time::Instant::now();
            emit(app, DownloadProgress { id: id.into(), downloaded, total, status: "downloading".into(), message: None });
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    if let Some(t) = total {
        if downloaded < t {
            return Err(format!("Descarga incompleta ({downloaded} de {t} bytes). Vuelve a intentarlo para reanudar."));
        }
    }
    tokio::fs::rename(&part_path, &final_path).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(models_dir: &Path, id: &str) -> Result<(), String> {
    let p = model_path(models_dir, id);
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    let part = models_dir.join(format!("{}.part", file_name(id)));
    if part.exists() {
        let _ = std::fs::remove_file(part);
    }
    Ok(())
}
