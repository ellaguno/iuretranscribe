//! Decodificación de audio/video a PCM mono f32 de 16 kHz.
//! Primero se intenta con symphonia (Rust puro); si el formato no es
//! compatible (p. ej. Opus/WebM) se recurre a `ffmpeg` si está instalado.

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const TARGET_RATE: u32 = 16_000;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "opus", "wma", "aiff", "aif", "mp4", "m4v", "mov",
    "mkv", "webm", "avi", "mpg", "mpeg", "3gp", "wmv", "ts",
];

/// Duración aproximada en segundos sin decodificar todo el archivo.
pub fn probe_duration(path: &Path) -> Option<f64> {
    if let Some(d) = probe_duration_symphonia(path) {
        return Some(d);
    }
    probe_duration_ffprobe(path)
}

fn probe_duration_symphonia(path: &Path) -> Option<f64> {
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;
    let track = probed
        .format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;
    let params = &track.codec_params;
    let n_frames = params.n_frames?;
    if let Some(tb) = params.time_base {
        let t = tb.calc_time(n_frames);
        return Some(t.seconds as f64 + t.frac);
    }
    let rate = params.sample_rate?;
    Some(n_frames as f64 / rate as f64)
}

fn probe_duration_ffprobe(path: &Path) -> Option<f64> {
    let out = hidden_command("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok()
}

/// Decodifica el archivo completo a mono 16 kHz.
pub fn decode_to_pcm16k(path: &Path) -> Result<Vec<f32>> {
    match decode_symphonia(path) {
        Ok(samples) if !samples.is_empty() => Ok(samples),
        Ok(_) => decode_ffmpeg(path).context("El archivo no contiene audio decodificable"),
        Err(sym_err) => {
            log::warn!("symphonia no pudo decodificar {}: {sym_err:#}; probando ffmpeg", path.display());
            decode_ffmpeg(path).map_err(|ff_err| {
                anyhow!(
                    "No se pudo decodificar el audio. Formato no compatible ({sym_err}). \
                     Instala ffmpeg para ampliar los formatos soportados ({ff_err})."
                )
            })
        }
    }
}

fn decode_symphonia(path: &Path) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path).with_context(|| format!("No se pudo abrir {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let fmt_opts = FormatOptions { enable_gapless: true, ..Default::default() };
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &MetadataOptions::default())
        .map_err(|e| anyhow!("formato no reconocido: {e}"))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no se encontró una pista de audio"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("códec no soportado: {e}"))?;

    let mut mono: Vec<f32> = Vec::new();
    let mut rate: u32 = track.codec_params.sample_rate.unwrap_or(0);
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(anyhow!("error leyendo el contenedor: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(e)) => {
                log::debug!("paquete dañado ignorado: {e}");
                continue;
            }
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(anyhow!("error decodificando: {e}")),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        if rate == 0 {
            rate = spec.rate;
        }
        let needed = decoded.capacity() as u64;
        let buf = match sample_buf.as_mut() {
            Some(b) if b.capacity() >= (needed as usize) * channels => b,
            _ => {
                sample_buf = Some(SampleBuffer::<f32>::new(needed, spec));
                sample_buf.as_mut().unwrap()
            }
        };
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        if channels == 1 {
            mono.extend_from_slice(samples);
        } else {
            let inv = 1.0 / channels as f32;
            mono.extend(samples.chunks_exact(channels).map(|f| f.iter().sum::<f32>() * inv));
        }
    }
    if rate == 0 {
        return Err(anyhow!("frecuencia de muestreo desconocida"));
    }
    Ok(resample(&mono, rate, TARGET_RATE))
}

fn decode_ffmpeg(path: &Path) -> Result<Vec<f32>> {
    let out = hidden_command("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-vn", "-f", "f32le", "-ac", "1", "-ar", "16000", "-"])
        .stdin(Stdio::null())
        .output()
        .context("ffmpeg no está instalado o no se pudo ejecutar")?;
    if !out.status.success() {
        return Err(anyhow!("ffmpeg falló: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let samples = out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect::<Vec<f32>>();
    Ok(samples)
}

/// Comando externo sin ventana de consola en Windows.
pub fn hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Remuestreo por sinc enventanado (Blackman) con tabla de fases.
pub fn resample(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    const HALF_TAPS: i64 = 32;
    const PHASES: usize = 128;
    let ratio = dst_rate as f64 / src_rate as f64; // salida / entrada
    let cutoff = 0.5 * ratio.min(1.0) * 0.96; // ciclos por muestra de entrada

    // tabla[fase][tap]: h(k - frac) para k en -HALF_TAPS+1 ..= HALF_TAPS
    let taps = (2 * HALF_TAPS) as usize;
    let mut table = vec![0f32; PHASES * taps];
    for p in 0..PHASES {
        let frac = p as f64 / PHASES as f64;
        let mut sum = 0f64;
        for (i, k) in (-HALF_TAPS + 1..=HALF_TAPS).enumerate() {
            let t = k as f64 - frac;
            let x = t / HALF_TAPS as f64; // -1..1
            let w = if x.abs() >= 1.0 { 0.0 } else { 0.42 + 0.5 * (std::f64::consts::PI * x).cos() + 0.08 * (2.0 * std::f64::consts::PI * x).cos() };
            let arg = 2.0 * cutoff * t;
            let sinc = if arg.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * arg).sin() / (std::f64::consts::PI * arg) };
            let h = 2.0 * cutoff * sinc * w;
            table[p * taps + i] = h as f32;
            sum += h;
        }
        if sum.abs() > 1e-12 {
            for i in 0..taps {
                table[p * taps + i] /= sum as f32;
            }
        }
    }

    let out_len = ((input.len() as f64) * ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    let n = input.len() as i64;
    for i in 0..out_len {
        let pos = i as f64 / ratio;
        let n0 = pos.floor() as i64;
        let frac = pos - n0 as f64;
        let phase = ((frac * PHASES as f64).round() as usize).min(PHASES - 1);
        let row = &table[phase * taps..(phase + 1) * taps];
        let mut acc = 0f32;
        for (j, k) in (-HALF_TAPS + 1..=HALF_TAPS).enumerate() {
            let idx = n0 + k;
            if idx >= 0 && idx < n {
                acc += input[idx as usize] * row[j];
            }
        }
        out.push(acc);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dominant_freq(samples: &[f32], rate: u32) -> f64 {
        // Cruces por cero como estimador simple de frecuencia.
        let mut crossings = 0usize;
        for w in samples.windows(2) {
            if (w[0] < 0.0) != (w[1] < 0.0) {
                crossings += 1;
            }
        }
        crossings as f64 / 2.0 / (samples.len() as f64 / rate as f64)
    }

    #[test]
    fn resample_keeps_tone_and_length() {
        let src = 48_000u32;
        let secs = 1.0f64;
        let n = (src as f64 * secs) as usize;
        let input: Vec<f32> = (0..n).map(|i| (2.0 * std::f64::consts::PI * 440.0 * i as f64 / src as f64).sin() as f32).collect();
        let out = resample(&input, src, TARGET_RATE);
        assert!((out.len() as i64 - TARGET_RATE as i64).abs() <= 1, "longitud {}", out.len());
        let f = dominant_freq(&out[100..out.len() - 100], TARGET_RATE);
        assert!((f - 440.0).abs() < 5.0, "frecuencia {f}");
        let peak = out.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.9 && peak < 1.05, "amplitud {peak}");
    }

    #[test]
    fn resample_upsamples() {
        let input: Vec<f32> = (0..8000).map(|i| (i as f32 / 50.0).sin()).collect();
        let out = resample(&input, 8000, 16000);
        assert_eq!(out.len(), 16000);
    }

    #[test]
    fn decode_scratch_files_if_present() {
        // Archivos generados con ffmpeg durante el desarrollo; se omite si no existen.
        let Ok(dir) = std::env::var("IURE_TEST_DIR") else { return };
        for name in ["sine48k.wav", "test.mp3", "test.mp4"] {
            let p = Path::new(&dir).join(name);
            if !p.exists() {
                continue;
            }
            let dur = probe_duration(&p);
            let pcm = decode_to_pcm16k(&p).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            let secs = pcm.len() as f64 / TARGET_RATE as f64;
            eprintln!("{name}: probe={dur:?} decoded={secs:.2}s");
            assert!(secs > 0.5, "{name} decodificó {secs}s");
            if let Some(d) = dur {
                assert!((d - secs).abs() < 1.0, "{name}: duración sonda {d} vs decodificada {secs}");
            }
            if name == "sine48k.wav" {
                let f = dominant_freq(&pcm[100..pcm.len() - 100], TARGET_RATE);
                assert!((f - 440.0).abs() < 5.0, "frecuencia {f}");
            }
        }
    }
}
