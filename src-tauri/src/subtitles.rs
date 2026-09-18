//! Formateo de segmentos a SRT / VTT / TXT / JSON.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
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
            s.text.trim()
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
            s.text.trim()
        ));
    }
    out
}

pub fn to_txt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for s in segments {
        out.push_str(s.text.trim());
        out.push('\n');
    }
    out
}

/// Texto corrido (una sola línea) para enviarlo al LLM.
pub fn to_plain(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn to_json(segments: &[Segment]) -> String {
    serde_json::to_string_pretty(segments).unwrap_or_else(|_| "[]".into())
}
