//! Identificación de hablantes: segmentación pyannote 3.0 + huellas de voz WeSpeaker
//! + agrupamiento, con sherpa-onnx. Es acústica: no depende del idioma.
//!
//! Corre en un ejecutable aparte (`iuretranscribe-diarize`, sidecar del instalador):
//! onnxruntime estático y whisper.cpp no conviven en el mismo binario (instancias
//! de `std::regex` incompatibles en Linux, CRT MT/MD en Windows).
//!
//! En las grabaciones con micrófono y sistema, el grabador guarda además el canal del
//! sistema (`tracks/<grabación>.sys.wav`): el micrófono sale de restarlo a la mezcla,
//! así que se sabe qué dijiste tú y sólo se separan entre sí las voces de la bocina.

use crate::subtitles::Segment;
use anyhow::{anyhow, Context, Result};
use iurefficient_connect::tr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Tramo en el que habla una persona (hablante numerado desde 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: i32,
}

const SIDECAR: &str = "iuretranscribe-diarize";

/// Los canales del sistema se borran pasado este tiempo (ocupan ~115 MB por hora).
const TRACK_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

static TRACKS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Carpeta de los canales del sistema; se fija una vez al arrancar.
pub fn set_tracks_dir(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    cleanup_tracks(&dir);
    let _ = TRACKS_DIR.set(dir);
}

/// Dónde se guarda (o se guardó) el canal del sistema de una grabación.
pub fn system_track(recording: &Path) -> Option<PathBuf> {
    let name = recording.file_name()?.to_string_lossy().into_owned();
    Some(TRACKS_DIR.get()?.join(format!("{name}.sys.wav")))
}

/// Al renombrar una grabación, su canal del sistema la acompaña.
pub fn rename_track(from: &Path, to: &Path) {
    if let (Some(a), Some(b)) = (system_track(from), system_track(to)) {
        if a.is_file() {
            let _ = std::fs::rename(a, b);
        }
    }
}

fn cleanup_tracks(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > TRACK_MAX_AGE);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

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
    let exe = sidecar_path().ok_or_else(|| {
        anyhow!(tr!(
            "{SIDECAR} was not found next to the application; reinstall IureTranscribe",
            "No se encontró {SIDECAR} junto a la aplicación; reinstala IureTranscribe"
        ))
    })?;
    let pcm = std::env::temp_dir().join(format!("iuretranscribe-diar-{}-{}.f32", std::process::id(), uuid::Uuid::new_v4()));
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(&pcm, bytes).with_context(|| tr!("Could not prepare the audio", "No se pudo preparar el audio"))?;
    let mut cmd = crate::audio::hidden_command(exe.to_string_lossy().as_ref());
    cmd.arg(segmentation).arg(embedding).arg(&pcm);
    if let Some(n) = num_speakers.filter(|n| *n > 0) {
        cmd.arg(n.to_string());
    }
    let out = cmd.output();
    let _ = std::fs::remove_file(&pcm);
    let out = out.with_context(|| tr!("Could not run {}", "No se pudo ejecutar {}", exe.display()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(tr!("Speaker identification failed ({}): {}", "Falló la identificación de hablantes ({}): {}", out.status, err.trim())));
    }
    parse_turns(&String::from_utf8_lossy(&out.stdout))
}

fn parse_turns(text: &str) -> Result<Vec<Turn>> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut it = l.split_whitespace();
            let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) else {
                return Err(anyhow!(tr!("unexpected output: {l}", "salida inesperada: {l}")));
            };
            let secs = |v: &str| v.parse::<f64>().map(|x| (x * 1000.0) as i64);
            Ok(Turn { start_ms: secs(a)?, end_ms: secs(b)?, speaker: c.parse()? })
        })
        .collect()
}

/// Hablante del turno que más coincide con el segmento; si cae en un silencio entre
/// turnos, el del turno más cercano.
fn best_speaker(seg: &Segment, turns: &[Turn]) -> Option<i32> {
    let mut best: Option<(i32, i64)> = None;
    for t in turns {
        let overlap = seg.end_ms.min(t.end_ms) - seg.start_ms.max(t.start_ms);
        if overlap > 0 && best.map_or(true, |(_, o)| overlap > o) {
            best = Some((t.speaker, overlap));
        }
    }
    best.map(|b| b.0).or_else(|| {
        turns
            .iter()
            .min_by_key(|t| (t.start_ms - seg.end_ms).max(seg.start_ms - t.end_ms).max(0))
            .map(|t| t.speaker)
    })
}

/// Numera los hablantes por orden de aparición (el primero que habla es el 1).
#[derive(Default)]
struct Order(Vec<i32>);

impl Order {
    fn index(&mut self, sp: i32) -> usize {
        match self.0.iter().position(|&o| o == sp) {
            Some(i) => i,
            None => {
                self.0.push(sp);
                self.0.len() - 1
            }
        }
    }
}

pub fn speaker_label(n: usize) -> String {
    tr!("Speaker {}", "Hablante {}", n + 1)
}

/// Audio mezclado (sin canales aparte): cada segmento recibe «Hablante N». Devuelve
/// cuántos hablantes distintos quedaron.
pub fn assign(segments: &mut [Segment], turns: &[Turn]) -> usize {
    let mut order = Order::default();
    for seg in segments.iter_mut() {
        if let Some(sp) = best_speaker(seg, turns) {
            seg.speaker = Some(speaker_label(order.index(sp)));
        }
    }
    order.0.len()
}

fn rms(samples: &[f32], start_ms: i64, end_ms: i64) -> f32 {
    let at = |ms: i64| ((ms.max(0) as usize) * crate::audio::TARGET_RATE as usize / 1000).min(samples.len());
    let s = &samples[at(start_ms)..at(end_ms).max(at(start_ms))];
    (s.iter().map(|x| x * x).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

/// Grabación con canal del sistema aparte. `names` = [tú, interlocutor]. Lo que dijiste
/// tú (segmento ya etiquetado con tu nombre o con más energía en el micrófono) queda a
/// tu nombre; lo de la bocina se reparte entre los turnos de `turns`, que salen de
/// diarizar sólo el canal del sistema. Con una sola voz al otro lado se conserva el
/// nombre del interlocutor; con varias, «Interlocutor 1», «Interlocutor 2»…
/// Devuelve cuántas voces distintas se oyeron por la bocina.
pub fn assign_split(segments: &mut [Segment], turns: &[Turn], names: &[String; 2], mic: &[f32], sys: &[f32]) -> usize {
    let [me, other] = names;
    let from_mic: Vec<bool> = segments
        .iter()
        .map(|s| match s.speaker.as_deref() {
            Some(n) if n == me => true,
            Some(n) if n == other => false,
            _ => rms(mic, s.start_ms, s.end_ms) > rms(sys, s.start_ms, s.end_ms),
        })
        .collect();
    let mut order = Order::default();
    let picks: Vec<Option<usize>> = segments
        .iter()
        .zip(&from_mic)
        .map(|(s, &m)| if m { None } else { best_speaker(s, turns).map(|sp| order.index(sp)) })
        .collect();
    let voices = order.0.len();
    for ((seg, &m), pick) in segments.iter_mut().zip(&from_mic).zip(picks) {
        seg.speaker = Some(match (m, pick) {
            (true, _) => me.clone(),
            (false, Some(i)) if voices > 1 => format!("{other} {}", i + 1),
            _ => other.clone(),
        });
    }
    voices.max(segments.iter().zip(&from_mic).any(|(_, &m)| !m) as usize)
}

/// Lee el canal del sistema y calcula el del micrófono (mezcla − sistema).
pub fn load_tracks(mix: &[f32], sys_path: &Path) -> Result<(Vec<f32>, Vec<f32>)> {
    let mut reader = hound::WavReader::open(sys_path).with_context(|| tr!("Could not read {}", "No se pudo leer {}", sys_path.display()))?;
    let mut sys: Vec<f32> = reader.samples::<i16>().map(|s| s.map(|v| v as f32 / 32767.0)).collect::<std::result::Result<_, _>>()?;
    sys.resize(mix.len(), 0.0);
    let mic = mix.iter().zip(&sys).map(|(m, s)| m - s).collect();
    Ok((mic, sys))
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
        assert_eq!(names, [speaker_label(0), speaker_label(1), speaker_label(1), speaker_label(0)]);
    }

    #[test]
    fn split_keeps_me_and_separates_the_other_side() {
        let rate = crate::audio::TARGET_RATE as usize;
        // 0–2 s habla el micrófono; 2–6 s, la bocina.
        let mut mic = vec![0.0f32; rate * 6];
        let mut sys = vec![0.0f32; rate * 6];
        mic[..rate * 2].iter_mut().for_each(|x| *x = 0.3);
        sys[rate * 2..].iter_mut().for_each(|x| *x = 0.3);
        let names = ["Ana".to_string(), "Interlocutor".to_string()];
        let turns = [Turn { start_ms: 2000, end_ms: 4000, speaker: 5 }, Turn { start_ms: 4000, end_ms: 6000, speaker: 2 }];

        // Sin etiquetas previas (transcripción de la mezcla): decide la energía.
        let mut segs = vec![seg(0, 2000), seg(2000, 4000), seg(4000, 6000)];
        assert_eq!(assign_split(&mut segs, &turns, &names, &mic, &sys), 2);
        let got: Vec<_> = segs.iter().map(|s| s.speaker.clone().unwrap()).collect();
        assert_eq!(got, ["Ana", "Interlocutor 1", "Interlocutor 2"]);

        // Una sola voz al otro lado: se conserva el nombre del interlocutor.
        let mut segs = vec![seg(0, 2000), seg(2000, 6000)];
        segs[0].speaker = Some("Ana".into());
        assert_eq!(assign_split(&mut segs, &turns[..1], &names, &mic, &sys), 1);
        assert_eq!(segs[1].speaker.as_deref(), Some("Interlocutor"));
    }

    /// Grabación con canal del sistema: IURE_TEST_DIAR_DIR (modelos) e IURE_TEST_SPLIT
    /// (carpeta con split_mix.wav, split_sys.wav y split.truth.json [[ini_ms, fin_ms, voz]]).
    /// La voz del micrófono es la primera de la referencia.
    #[test]
    fn split_real_audio() {
        let (Ok(dir), Ok(split)) = (std::env::var("IURE_TEST_DIAR_DIR"), std::env::var("IURE_TEST_SPLIT")) else { return };
        let (dir, split) = (PathBuf::from(dir), PathBuf::from(split));
        let truth: Vec<(i64, i64, String)> = serde_json::from_str(&std::fs::read_to_string(split.join("split.truth.json")).unwrap()).unwrap();
        let mix = crate::audio::decode_to_pcm16k(&split.join("split_mix.wav")).unwrap();
        let (mic, sys) = load_tracks(&mix, &split.join("split_sys.wav")).unwrap();
        let turns = diarize(
            &dir.join(crate::models::file_name(crate::models::DIAR_SEGMENTATION)),
            &dir.join(crate::models::file_name(crate::models::DIAR_EMBEDDING)),
            &sys,
            None,
        )
        .unwrap();
        let mut segs: Vec<Segment> = truth.iter().map(|(a, b, _)| seg(*a, *b)).collect();
        let names = ["Yo".to_string(), "Interlocutor".to_string()];
        let voices = assign_split(&mut segs, &turns, &names, &mic, &sys);
        let me = &truth[0].2;
        for ((_, _, real), s) in truth.iter().zip(&segs) {
            eprintln!("{real:>5} → {}", s.speaker.as_deref().unwrap_or("-"));
        }
        // Tú: exactamente tus turnos.
        for ((_, _, real), s) in truth.iter().zip(&segs) {
            assert_eq!(real == me, s.speaker.as_deref() == Some("Yo"), "{real}");
        }
        // Bocina: nunca dos voces reales con la misma etiqueta (eso no tiene arreglo); que
        // una voz salga partida en dos se corrige renombrando, así que se tolera una de más.
        let mut owner = std::collections::HashMap::new();
        for ((_, _, real), s) in truth.iter().zip(&segs).filter(|((_, _, r), _)| r != me) {
            assert_eq!(owner.entry(s.speaker.clone()).or_insert_with(|| real.clone()), real, "{:?}", s.speaker);
        }
        let real_voices = truth.iter().map(|t| &t.2).filter(|r| *r != me).collect::<std::collections::HashSet<_>>().len();
        assert!(voices >= real_voices && voices <= real_voices + 1, "{voices} voces para {real_voices}");
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
