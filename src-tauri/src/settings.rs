//! Ajustes persistentes de la aplicación (JSON en el directorio de configuración).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_SUMMARY_PROMPT: &str = "Eres un asistente que resume transcripciones de audio en español. Responde en español, en Markdown, de forma clara y concisa. No inventes información que no esté en la transcripción.";
pub const DEFAULT_MINUTES_PROMPT: &str = "Eres un asistente que redacta minutas de reuniones a partir de transcripciones en español. Responde en español, en Markdown. Estructura la minuta con estas secciones: Participantes (si se pueden identificar), Temas tratados, Decisiones tomadas, Acuerdos y compromisos (con responsable y fecha si se mencionan), Pendientes / próximos pasos. No inventes información que no esté en la transcripción.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Id del modelo del catálogo (ver models.rs).
    pub model_id: String,
    /// Código ISO del idioma, o "auto" para detección automática.
    pub language: String,
    /// Traducir al inglés en lugar de transcribir.
    pub translate: bool,
    /// Formatos de salida: "srt", "vtt", "txt", "json".
    pub formats: Vec<String>,
    /// "same" = junto al archivo original, "custom" = `output_dir`.
    pub output_mode: String,
    pub output_dir: Option<String>,
    pub use_gpu: bool,
    /// 0 = automático.
    pub threads: u32,
    /// 1 = greedy (rápido), >1 = beam search.
    pub beam_size: u32,
    pub auto_summary: bool,
    pub auto_minutes: bool,
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    pub summary_prompt: String,
    pub minutes_prompt: String,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Carpeta donde se guardan las grabaciones (None = Música/IureTranscribe).
    pub recordings_dir: Option<String>,
    pub record_mic: bool,
    pub record_system: bool,
    /// Id del micrófono (cpal) o None = predeterminado.
    pub mic_device: Option<String>,
    pub auto_transcribe_recording: bool,
    /// Transcribir en vivo mientras se graba.
    pub live_transcription: bool,
    /// Segundos por bloque de la transcripción en vivo.
    pub live_chunk_secs: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_id: "large-v3-turbo".into(),
            language: "es".into(),
            translate: false,
            formats: vec!["srt".into(), "txt".into()],
            output_mode: "same".into(),
            output_dir: None,
            use_gpu: true,
            threads: 0,
            beam_size: 5,
            auto_summary: true,
            auto_minutes: true,
            openrouter_api_key: String::new(),
            openrouter_model: "google/gemini-2.5-flash".into(),
            summary_prompt: DEFAULT_SUMMARY_PROMPT.into(),
            minutes_prompt: DEFAULT_MINUTES_PROMPT.into(),
            theme: "system".into(),
            recordings_dir: None,
            record_mic: true,
            record_system: true,
            mic_device: None,
            auto_transcribe_recording: true,
            live_transcription: true,
            live_chunk_secs: 8.0,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s = std::fs::read_to_string(path)
            .ok()
            .and_then(|txt| serde_json::from_str::<Settings>(&txt).ok())
            .unwrap_or_default();
        if s.summary_prompt.trim().is_empty() {
            s.summary_prompt = DEFAULT_SUMMARY_PROMPT.into();
        }
        if s.minutes_prompt.trim().is_empty() {
            s.minutes_prompt = DEFAULT_MINUTES_PROMPT.into();
        }
        // Primera ejecución: hereda la llave del script `transcribe` de Linux si existe.
        if s.openrouter_api_key.is_empty() {
            if let Some((key, model)) = legacy_transcribe_config() {
                s.openrouter_api_key = key;
                if let Some(m) = model {
                    s.openrouter_model = m;
                }
            }
        }
        s
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Lee `~/.config/transcribe/config` (formato bash `VAR=valor`) si existe.
fn legacy_transcribe_config() -> Option<(String, Option<String>)> {
    let base: PathBuf = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
    let txt = std::fs::read_to_string(base.join("transcribe").join("config")).ok()?;
    let mut key = None;
    let mut model = None;
    for line in txt.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
            match k.trim() {
                "OPENROUTER_API_KEY" if !v.is_empty() => key = Some(v),
                "OPENROUTER_MODEL" if !v.is_empty() => model = Some(v),
                _ => {}
            }
        }
    }
    key.map(|k| (k, model))
}
