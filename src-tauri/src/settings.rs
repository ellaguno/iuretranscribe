//! Ajustes persistentes de la aplicación (JSON en el directorio de configuración).

use crate::vocab::Correction;
use iurefficient_connect::lang;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Instrucciones predeterminadas (en español) de versiones anteriores: un ajuste
/// guardado con este texto exacto se migra a vacío (= predeterminado del idioma).
const LEGACY_SUMMARY_PROMPT: &str = "Eres un asistente que resume transcripciones de audio en español. Responde en español, en Markdown, de forma clara y concisa. No inventes información que no esté en la transcripción.";
const LEGACY_MINUTES_PROMPT: &str = "Eres un asistente que redacta minutas de reuniones a partir de transcripciones en español. Responde en español, en Markdown. Estructura la minuta con estas secciones: Participantes (si se pueden identificar), Temas tratados, Decisiones tomadas, Acuerdos y compromisos (con responsable y fecha si se mencionan), Pendientes / próximos pasos. No inventes información que no esté en la transcripción.";

const DEFAULT_SUMMARY_PROMPT_EN: &str = "You are an assistant that summarizes audio transcriptions. Respond in Markdown, clearly and concisely, in the same language as the transcription. Do not make up information that is not in the transcription.";
const DEFAULT_SUMMARY_PROMPT_ES: &str = "Eres un asistente que resume transcripciones de audio. Responde en Markdown, de forma clara y concisa, en el mismo idioma que la transcripción. No inventes información que no esté en la transcripción.";
const DEFAULT_MINUTES_PROMPT_EN: &str = "You are an assistant that writes meeting minutes from transcriptions. Respond in Markdown, in the same language as the transcription (including the section headings). Structure the minutes with these sections: Participants (if they can be identified), Topics discussed, Decisions made, Agreements and commitments (with owner and date if mentioned), Open items / next steps. Do not make up information that is not in the transcription.";
const DEFAULT_MINUTES_PROMPT_ES: &str = "Eres un asistente que redacta minutas de reuniones a partir de transcripciones. Responde en Markdown, en el mismo idioma que la transcripción (también los títulos de las secciones). Estructura la minuta con estas secciones: Participantes (si se pueden identificar), Temas tratados, Decisiones tomadas, Acuerdos y compromisos (con responsable y fecha si se mencionan), Pendientes / próximos pasos. No inventes información que no esté en la transcripción.";

/// Instrucciones predeterminadas del resumen en el idioma actual de la interfaz.
pub fn default_summary_prompt() -> &'static str {
    lang::pick(DEFAULT_SUMMARY_PROMPT_EN, DEFAULT_SUMMARY_PROMPT_ES)
}

/// Instrucciones predeterminadas de la minuta en el idioma actual de la interfaz.
pub fn default_minutes_prompt() -> &'static str {
    lang::pick(DEFAULT_MINUTES_PROMPT_EN, DEFAULT_MINUTES_PROMPT_ES)
}

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
    /// Nombres y términos propios que se dan a Whisper como prompt inicial.
    pub vocabulary: String,
    /// Correcciones que se aplican al texto transcrito.
    pub corrections: Vec<Correction>,
    pub auto_summary: bool,
    pub auto_minutes: bool,
    pub openrouter_api_key: String,
    pub openrouter_model: String,
    /// Vacío = instrucciones predeterminadas del idioma de la interfaz.
    pub summary_prompt: String,
    pub minutes_prompt: String,
    /// Idioma de la interfaz: "auto" (el del sistema), "en" o "es".
    pub ui_language: String,
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
    /// Al detener, usar la transcripción en vivo como resultado final (sin repetir).
    pub live_is_final: bool,
    /// Cuenta de Iurefficient (fase 0: WebDAV con contraseña de aplicación `iurdav_…`).
    pub iure_domain: String,
    pub iure_email: String,
    pub iure_app_password: String,
    /// Subir también el audio/video original al guardar en Iurefficient.
    pub iure_upload_media: bool,
    /// Última carpeta WebDAV usada al guardar (ruta relativa, p. ej. "Clientes/Acme/Proyecto X").
    pub iure_last_folder: Option<String>,
    /// Al guardar en un proyecto, generar la minuta con el motor de Iurefficient automáticamente.
    pub iure_auto_compose: bool,
    /// Al grabar micrófono y sistema, transcribir cada fuente por separado («quién habló»).
    pub speaker_split: bool,
    /// Nombre del usuario para etiquetar el micrófono.
    pub my_name: String,
    /// Consultar en GitHub si hay una versión nueva al arrancar.
    pub check_updates: bool,
    /// Resultado de la sonda de GPU de las variantes CUDA/Vulkan: "gpu" (funciona),
    /// "cpu" (sólo el procesador) o "none" (ni siquiera arranca). Vacío = sin probar.
    pub gpu_probe_result: String,
    /// Versión de la app con la que se hizo la sonda (se repite al actualizar).
    pub gpu_probe_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_id: "base".into(),
            language: "es".into(),
            translate: false,
            formats: vec!["srt".into(), "txt".into()],
            output_mode: "same".into(),
            output_dir: None,
            use_gpu: true,
            threads: 0,
            beam_size: 5,
            vocabulary: String::new(),
            corrections: Vec::new(),
            auto_summary: true,
            auto_minutes: true,
            openrouter_api_key: String::new(),
            openrouter_model: "google/gemini-2.5-flash".into(),
            summary_prompt: String::new(),
            minutes_prompt: String::new(),
            ui_language: "auto".into(),
            theme: "system".into(),
            recordings_dir: None,
            record_mic: true,
            record_system: true,
            mic_device: None,
            auto_transcribe_recording: true,
            live_transcription: true,
            live_chunk_secs: 8.0,
            live_is_final: true,
            iure_domain: String::new(),
            iure_email: String::new(),
            iure_app_password: String::new(),
            iure_upload_media: true,
            iure_last_folder: None,
            iure_auto_compose: true,
            speaker_split: true,
            my_name: String::new(),
            check_updates: true,
            gpu_probe_result: String::new(),
            gpu_probe_version: String::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s = std::fs::read_to_string(path)
            .ok()
            .and_then(|txt| serde_json::from_str::<Settings>(&txt).ok())
            .unwrap_or_default();
        // Las instrucciones predeterminadas antiguas (sólo en español) pasan a vacío para
        // seguir el idioma de la interfaz; las personalizadas no se tocan.
        if s.summary_prompt.trim().is_empty() || s.summary_prompt.trim() == LEGACY_SUMMARY_PROMPT {
            s.summary_prompt.clear();
        }
        if s.minutes_prompt.trim().is_empty() || s.minutes_prompt.trim() == LEGACY_MINUTES_PROMPT {
            s.minutes_prompt.clear();
        }
        if s.ui_language.trim().is_empty() {
            s.ui_language = "auto".into();
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

    /// Instrucciones del resumen que se usan de verdad (las del usuario o las predeterminadas).
    pub fn effective_summary_prompt(&self) -> String {
        match self.summary_prompt.trim() {
            "" => default_summary_prompt().into(),
            p => p.into(),
        }
    }

    pub fn effective_minutes_prompt(&self) -> String {
        match self.minutes_prompt.trim() {
            "" => default_minutes_prompt().into(),
            p => p.into(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_prompts_migrate_to_default() {
        let dir = std::env::temp_dir().join(format!("iuret-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, serde_json::json!({ "summaryPrompt": LEGACY_SUMMARY_PROMPT, "minutesPrompt": "Mi minuta" }).to_string()).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.summary_prompt, "");
        assert_eq!(s.minutes_prompt, "Mi minuta");
        assert_eq!(s.ui_language, "auto");
        assert_eq!(s.effective_minutes_prompt(), "Mi minuta");
        assert!(!s.effective_summary_prompt().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
