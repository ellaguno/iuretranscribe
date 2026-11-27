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

// ------------------------------------------------------------------ lectura

/// Máximo de segmentos que se leen de un archivo (un archivo hostil no agota la memoria).
const MAX_SEGMENTS: usize = 200_000;

/// Segmentos de un `.json` escrito por `to_json`. `None` si no tiene esa forma.
pub fn from_json(text: &str) -> Option<Vec<Segment>> {
    let mut segments: Vec<Segment> = serde_json::from_str(text).ok()?;
    segments.truncate(MAX_SEGMENTS);
    Some(segments)
}

/// Segmentos de un SRT o VTT: cada bloque con una línea `inicio --> fin` y su texto.
/// Los bloques sin horas válidas se descartan.
pub fn from_timed(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut lines = text.lines().map(str::trim);
    while let Some(l) = lines.next() {
        if out.len() >= MAX_SEGMENTS {
            break;
        }
        let Some((a, b)) = l.split_once("-->") else { continue };
        // En VTT la hora de fin puede llevar ajustes de posición después de un espacio.
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start_ms), Some(end_ms)) = (parse_ts(a), parse_ts(b)) else { continue };
        let mut body = Vec::new();
        for t in lines.by_ref() {
            if t.is_empty() {
                break;
            }
            body.push(t);
        }
        let text = body.join(" ");
        if !text.is_empty() {
            out.push(Segment { start_ms, end_ms, text, speaker: None });
        }
    }
    out
}

/// Segmentos de un `.txt` (una línea por segmento, sin horas).
pub fn from_txt(text: &str) -> Vec<Segment> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(MAX_SEGMENTS)
        .map(|l| Segment { start_ms: 0, end_ms: 0, text: l.to_string(), speaker: None })
        .collect()
}

/// `hh:mm:ss,mmm`, `hh:mm:ss.mmm` o `mm:ss.mmm` → milisegundos.
fn parse_ts(s: &str) -> Option<i64> {
    let s = s.trim();
    let (hms, ms) = s.split_once([',', '.']).unwrap_or((s, "0"));
    let ms: i64 = ms.get(..ms.len().min(3))?.parse().ok()?;
    let parts: Vec<i64> = hms.split(':').map(|p| p.parse::<i64>().ok()).collect::<Option<Vec<_>>>()?;
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return None,
    };
    if h < 0 || !(0..60).contains(&m) || !(0..60).contains(&sec) || !(0..1000).contains(&ms) || h > 10_000 {
        return None;
    }
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: i64, b: i64, t: &str) -> Segment {
        Segment { start_ms: a, end_ms: b, text: t.into(), speaker: None }
    }

    #[test]
    fn srt_and_vtt_round_trip() {
        let segs = vec![seg(0, 1500, "Hola"), seg(1500, 3_723_004, "¿Qué tal?")];
        let srt = from_timed(&to_srt(&segs));
        assert_eq!(srt.len(), 2);
        assert_eq!((srt[1].start_ms, srt[1].end_ms, srt[1].text.as_str()), (1500, 3_723_004, "¿Qué tal?"));
        let vtt = from_timed(&to_vtt(&segs));
        assert_eq!(vtt.len(), 2);
        assert_eq!(vtt[0].text, "Hola");
    }

    #[test]
    fn hostile_timed_text_does_not_panic() {
        for t in ["-->", "a --> b\nx", "99:99:99,999 --> 1\nx", "00:00:01,ñ --> 00:00:02,000\nx", "\u{0}-->\u{0}", "00:00:01.5 --> 00:00:02.25 align:start\nok"] {
            let _ = from_timed(t);
        }
        assert_eq!(from_timed("00:00:01.5 --> 00:00:02.25 align:start\nok").len(), 1);
        assert!(from_json("{\"no\": 1}").is_none());
        assert_eq!(from_json(&to_json(&[seg(1, 2, "x")])).map(|v| v.len()), Some(1));
        assert_eq!(from_txt("a\n\n b \n").len(), 2);
    }
}
