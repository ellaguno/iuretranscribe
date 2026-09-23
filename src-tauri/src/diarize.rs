//! Identificación de hablantes: segmentación pyannote 3.0 + huellas de voz WeSpeaker
//! + agrupamiento, con sherpa-onnx. Es acústica: no depende del idioma.
//!
//! Corre en un ejecutable aparte (`iuretranscribe-diarize`, sidecar del instalador):
//! onnxruntime estático y whisper.cpp no conviven en el mismo binario (instancias
//! de `std::regex` incompatibles en Linux, CRT MT/MD en Windows).

use crate::subtitles::Segment;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// Tramo en el que habla una persona (hablante numerado desde 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: i32,
}

const SIDECAR: &str = "iuretranscribe-diarize";

/// El sidecar va junto al ejecutable de la app; en desarrollo, en su propio target.
fn sidecar_path() -> Option<PathBuf> {
    let name = format!("{SIDECAR}{}", std::env::consts::EXE_SUFFIX);
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("diarizer/target/release").join(&name);
    [exe_dir.join(&name), dev].into_iter().find(|p| p.is_file())
}

/// Separa el audio (PCM mono 16 kHz) en turnos de habla. `num_speakers` fija cuántas
/// personas hay; sin él se estiman con el umbral de agrupamiento.
pub fn diarize(segmentation: &Path, embedding: &Path, samples: &[f32], num_speakers: Option<u32>) -> Result<Vec<Turn>> {
    let exe = sidecar_path().ok_or_else(|| anyhow!("No se encontró {SIDECAR} junto a la aplicación; reinstala IureTranscribe"))?;
    let pcm = std::env::temp_dir().join(format!("iuretranscribe-diar-{}.f32", std::process::id()));
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(&pcm, bytes).context("No se pudo preparar el audio")?;
    let mut cmd = crate::audio::hidden_command(exe.to_string_lossy().as_ref());
    cmd.arg(segmentation).arg(embedding).arg(&pcm);
    if let Some(n) = num_speakers.filter(|n| *n > 0) {
        cmd.arg(n.to_string());
    }
    let out = cmd.output();
    let _ = std::fs::remove_file(&pcm);
    let out = out.with_context(|| format!("No se pudo ejecutar {}", exe.display()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("Falló la identificación de hablantes ({}): {}", out.status, err.trim()));
    }
    parse_turns(&String::from_utf8_lossy(&out.stdout))
}

fn parse_turns(text: &str) -> Result<Vec<Turn>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut it = l.split_whitespace();
            let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) else {
                return Err(anyhow!("salida inesperada: {l}"));
            };
            let secs = |v: &str| v.parse::<f64>().map(|x| (x * 1000.0) as i64);
            Ok(Turn { start_ms: secs(a)?, end_ms: secs(b)?, speaker: c.parse()? })
        })
        .collect()
}

/// Pone a cada segmento el hablante que más tiempo coincide con él. Los hablantes se
/// numeran por orden de aparición («Hablante 1» es el primero que habla). Devuelve
/// cuántos hablantes distintos quedaron.
pub fn assign(segments: &mut [Segment], turns: &[Turn]) -> usize {
    let mut order: Vec<i32> = Vec::new();
    for seg in segments.iter_mut() {
        let mut best: Option<(i32, i64)> = None;
        for t in turns {
            let overlap = seg.end_ms.min(t.end_ms) - seg.start_ms.max(t.start_ms);
            if overlap > 0 && best.map_or(true, |(_, o)| overlap > o) {
                best = Some((t.speaker, overlap));
            }
        }
        // Sin coincidencia (silencio entre turnos): el turno más cercano.
        let speaker = best.map(|b| b.0).or_else(|| {
            turns
                .iter()
                .min_by_key(|t| (t.start_ms - seg.end_ms).max(seg.start_ms - t.end_ms).max(0))
                .map(|t| t.speaker)
        });
        if let Some(sp) = speaker {
            let n = match order.iter().position(|&o| o == sp) {
                Some(i) => i,
                None => {
                    order.push(sp);
                    order.len() - 1
                }
            };
            seg.speaker = Some(format!("Hablante {}", n + 1));
        }
    }
    order.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: i64, b: i64) -> Segment {
        Segment { start_ms: a, end_ms: b, text: "x".into(), speaker: None }
    }

    #[test]
    fn parses_sidecar_output() {
        let t = parse_turns("3.372 49.087 0\n17.058 17.632 1\n\n").unwrap();
        assert_eq!(t, [Turn { start_ms: 3372, end_ms: 49087, speaker: 0 }, Turn { start_ms: 17058, end_ms: 17632, speaker: 1 }]);
        assert!(parse_turns("basura").is_err());
    }

    #[test]
    fn assign_by_overlap_and_order() {
        let turns = [
            Turn { start_ms: 0, end_ms: 4000, speaker: 3 },
            Turn { start_ms: 4000, end_ms: 9000, speaker: 0 },
            Turn { start_ms: 9500, end_ms: 12000, speaker: 3 },
        ];
        let mut segs = vec![seg(0, 3000), seg(3000, 8000), seg(9000, 9400), seg(9600, 11000)];
        assert_eq!(assign(&mut segs, &turns), 2);
        let names: Vec<_> = segs.iter().map(|s| s.speaker.clone().unwrap()).collect();
        assert_eq!(names, ["Hablante 1", "Hablante 2", "Hablante 2", "Hablante 1"]);
    }

    /// Requiere IURE_TEST_DIAR_DIR (con los dos .onnx) e IURE_TEST_AUDIO.
    #[test]
    fn diarize_real_audio() {
        let (Ok(dir), Ok(audio)) = (std::env::var("IURE_TEST_DIAR_DIR"), std::env::var("IURE_TEST_AUDIO")) else { return };
        let dir = std::path::PathBuf::from(dir);
        let samples = crate::audio::decode_to_pcm16k(Path::new(&audio)).unwrap();
        let t0 = std::time::Instant::now();
        let turns = diarize(
            &dir.join(crate::models::file_name(crate::models::DIAR_SEGMENTATION)),
            &dir.join(crate::models::file_name(crate::models::DIAR_EMBEDDING)),
            &samples,
            None,
        )
        .unwrap();
        let speakers: std::collections::BTreeSet<i32> = turns.iter().map(|t| t.speaker).collect();
        eprintln!("{} turnos, {} hablantes, {:.1}s para {:.0}s de audio", turns.len(), speakers.len(), t0.elapsed().as_secs_f32(), samples.len() as f32 / 16000.0);
        for t in turns.iter().take(40) {
            eprintln!("{:>7.1}-{:>7.1}  {}", t.start_ms as f32 / 1000.0, t.end_ms as f32 / 1000.0, t.speaker);
        }
        assert!(!turns.is_empty());
    }
}
