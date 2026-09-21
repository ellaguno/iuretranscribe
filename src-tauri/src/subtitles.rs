//! Formateo de segmentos a SRT / VTT / TXT / JSON.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    /// Quién habla (p. ej. «Eduardo» o «Interlocutor»), si se distinguió la fuente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
}

/// Texto del segmento con el hablante como prefijo («Eduardo: …») cuando lo hay.
pub fn line(s: &Segment) -> String {
    match s.speaker.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => format!("{n}: {}", s.text.trim()),
        None => s.text.trim().to_string(),
    }
}

fn ts(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        sep,
        ms % 1000
    )
}

pub fn to_srt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for (i, s) in segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            ts(s.start_ms, ','),
            ts(s.end_ms, ','),
            line(s)
        ));
    }
    out
}

pub fn to_vtt(segments: &[Segment]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for s in segments {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            ts(s.start_ms, '.'),
            ts(s.end_ms, '.'),
            line(s)
        ));
    }
    out
}

pub fn to_txt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for s in segments {
        out.push_str(&line(s));
        out.push('\n');
    }
    out
}

/// Texto corrido (una sola línea) para enviarlo al LLM.
pub fn to_plain(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(line)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn to_json(segments: &[Segment]) -> String {
    serde_json::to_string_pretty(segments).unwrap_or_else(|_| "[]".into())
}
