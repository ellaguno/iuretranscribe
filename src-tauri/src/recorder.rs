//! Grabación de micrófono y/o audio del sistema a WAV mono 16 kHz.
//!
//! - Linux: subprocesos `pw-record` (PipeWire) o `parec` (PulseAudio); el audio
//!   del sistema se toma del *monitor* de la salida predeterminada.
//! - Windows: cpal; el audio del sistema vía loopback WASAPI del dispositivo de salida.
//! - macOS: cpal para el micrófono; el audio del sistema requiere un dispositivo virtual.

use crate::audio::TARGET_RATE;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MIC: usize = 0;
const SYS: usize = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    pub inputs: Vec<AudioDevice>,
    /// "native" | "virtual" | "unavailable"
    pub system_capture: &'static str,
    pub note: String,
    pub backend: &'static str,
}

#[derive(Debug, Clone)]
pub struct StartOptions {
    pub capture_mic: bool,
    pub mic_device: Option<String>,
    pub capture_system: bool,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    pub active: bool,
    pub elapsed_secs: f64,
    pub mic_level: f32,
    pub sys_level: f32,
    pub path: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingResult {
    pub path: String,
    pub duration_secs: f64,
}

enum Msg {
    Data(usize, Vec<f32>),
    Error(usize, String),
}

struct Active {
    stop: Arc<AtomicBool>,
    started: Instant,
    path: PathBuf,
    mixer: Option<JoinHandle<Result<u64>>>,
    workers: Vec<JoinHandle<()>>,
    children: Arc<Mutex<Vec<std::process::Child>>>,
}

#[derive(Default)]
pub struct Recorder {
    active: Mutex<Option<Active>>,
    levels: [Arc<AtomicU32>; 2],
    last_error: Arc<Mutex<Option<String>>>,
}

impl Recorder {
    pub fn start(&self, opts: StartOptions) -> Result<PathBuf> {
        let mut slot = self.active.lock().unwrap();
        if slot.is_some() {
            return Err(anyhow!("Ya hay una grabación en curso"));
        }
        if !opts.capture_mic && !opts.capture_system {
            return Err(anyhow!("Selecciona al menos una fuente: micrófono o audio del sistema"));
        }
        std::fs::create_dir_all(&opts.output_dir)
            .map_err(|e| anyhow!("No se pudo crear la carpeta de grabaciones {}: {e}", opts.output_dir.display()))?;
        let name = format!("Grabación {}.wav", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = opts.output_dir.join(name);

        let stop = Arc::new(AtomicBool::new(false));
        let children = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<Msg>();
        *self.last_error.lock().unwrap() = None;
        for l in &self.levels {
            l.store(0, Ordering::Relaxed);
        }

        let mut workers = Vec::new();
        let mut sources = [false; 2];
        if opts.capture_mic {
            workers.push(spawn_source(MIC, opts.mic_device.clone(), tx.clone(), stop.clone(), children.clone())?);
            sources[MIC] = true;
        }
        if opts.capture_system {
            match spawn_source(SYS, None, tx.clone(), stop.clone(), children.clone()) {
                Ok(h) => {
                    workers.push(h);
                    sources[SYS] = true;
                }
                Err(e) => {
                    if !opts.capture_mic {
                        stop.store(true, Ordering::Relaxed);
                        return Err(e);
                    }
                    *self.last_error.lock().unwrap() = Some(format!("Sólo se graba el micrófono: {e}"));
                }
            }
        }
        drop(tx);

        let mixer = {
            let path = path.clone();
            let stop = stop.clone();
            let levels = [self.levels[0].clone(), self.levels[1].clone()];
            let err_slot = self.last_error.clone();
            std::thread::spawn(move || mixer(rx, &path, sources, stop, levels, err_slot))
        };
        *slot = Some(Active { stop, started: Instant::now(), path: path.clone(), mixer: Some(mixer), workers, children });
        Ok(path)
    }

    pub fn stop(&self) -> Result<RecordingResult> {
        let mut active = self
            .active
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| anyhow!("No hay ninguna grabación en curso"))?;
        active.stop.store(true, Ordering::Relaxed);
        for child in active.children.lock().unwrap().iter_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for w in active.workers.drain(..) {
            let _ = w.join();
        }
        let samples = active
            .mixer
            .take()
            .map(|m| m.join().unwrap_or_else(|_| Err(anyhow!("el mezclador falló"))))
            .unwrap_or(Ok(0))?;
        for l in &self.levels {
            l.store(0, Ordering::Relaxed);
        }
        if samples < TARGET_RATE as u64 / 2 {
            let _ = std::fs::remove_file(&active.path);
            let detail = self.last_error.lock().unwrap().clone().unwrap_or_default();
            return Err(anyhow!("La grabación no captó audio. {detail}"));
        }
        Ok(RecordingResult { path: active.path.to_string_lossy().into_owned(), duration_secs: samples as f64 / TARGET_RATE as f64 })
    }

    pub fn status(&self) -> RecordingStatus {
        let active = self.active.lock().unwrap();
        let level = |i: usize| f32::from_bits(self.levels[i].load(Ordering::Relaxed));
        match active.as_ref() {
            Some(a) => RecordingStatus {
                active: true,
                elapsed_secs: a.started.elapsed().as_secs_f64(),
                mic_level: level(MIC),
                sys_level: level(SYS),
                path: Some(a.path.to_string_lossy().into_owned()),
                error: self.last_error.lock().unwrap().clone(),
            },
            None => RecordingStatus { active: false, elapsed_secs: 0.0, mic_level: 0.0, sys_level: 0.0, path: None, error: None },
        }
    }
}

/// Mezcla las fuentes (ya en mono 16 kHz) y escribe el WAV. Devuelve muestras escritas.
fn mixer(
    rx: Receiver<Msg>,
    path: &Path,
    sources: [bool; 2],
    stop: Arc<AtomicBool>,
    levels: [Arc<AtomicU32>; 2],
    err_slot: Arc<Mutex<Option<String>>>,
) -> Result<u64> {
    let spec = hound::WavSpec { channels: 1, sample_rate: TARGET_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|e| anyhow!("No se pudo crear {}: {e}", path.display()))?;
    let mut queues: [VecDeque<f32>; 2] = [VecDeque::new(), VecDeque::new()];
    let mut alive = sources;
    let mut written = 0u64;
    let to_i16 = |s: f32| (s.clamp(-1.0, 1.0) * 32767.0) as i16;
    let stall_limit = TARGET_RATE as usize * 2; // 2 s sin datos de la otra fuente → no esperar más

    loop {
        match rx.recv_timeout(Duration::from_millis(150)) {
            Ok(Msg::Data(i, data)) => {
                let rms = (data.iter().map(|s| s * s).sum::<f32>() / data.len().max(1) as f32).sqrt();
                levels[i].store((rms * 6.0).min(1.0).to_bits(), Ordering::Relaxed);
                queues[i].extend(data);
            }
            Ok(Msg::Error(i, e)) => {
                alive[i] = false;
                levels[i].store(0, Ordering::Relaxed);
                let who = if i == MIC { "micrófono" } else { "audio del sistema" };
                *err_slot.lock().unwrap() = Some(format!("Falló la captura de {who}: {e}"));
            }
            Err(RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if alive[MIC] && alive[SYS] {
            let n = queues[MIC].len().min(queues[SYS].len());
            for _ in 0..n {
                let s = queues[MIC].pop_front().unwrap_or(0.0) + queues[SYS].pop_front().unwrap_or(0.0);
                writer.write_sample(to_i16(s))?;
            }
            written += n as u64;
            // Si una fuente se estanca (p. ej. loopback sin reproducción), no retener la otra.
            for i in [MIC, SYS] {
                let other = 1 - i;
                if queues[other].is_empty() && queues[i].len() > stall_limit {
                    while let Some(s) = queues[i].pop_front() {
                        writer.write_sample(to_i16(s))?;
                        written += 1;
                    }
                }
            }
        } else {
            for q in queues.iter_mut() {
                while let Some(s) = q.pop_front() {
                    writer.write_sample(to_i16(s))?;
                    written += 1;
                }
            }
        }
    }
    // Vacía lo que quede, mezclando con silencio.
    let n = queues[MIC].len().max(queues[SYS].len());
    for _ in 0..n {
        let s = queues[MIC].pop_front().unwrap_or(0.0) + queues[SYS].pop_front().unwrap_or(0.0);
        writer.write_sample(to_i16(s))?;
        written += 1;
    }
    writer.finalize()?;
    Ok(written)
}

// ---------------------------------------------------------------------------
// Linux: PipeWire / PulseAudio por subproceso
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
fn command_exists(name: &str) -> bool {
    crate::audio::hidden_command(name)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[cfg(target_os = "linux")]
pub fn list_devices() -> DeviceList {
    let available = command_exists("pw-record") || command_exists("parec");
    DeviceList {
        inputs: vec![AudioDevice { id: "default".into(), name: "Micrófono predeterminado del sistema".into(), is_default: true }],
        system_capture: if available { "native" } else { "unavailable" },
        note: if available {
            "El micrófono y la salida se eligen en la configuración de sonido del sistema. El audio del sistema se toma del monitor de la salida predeterminada.".into()
        } else {
            "Se requiere PipeWire (pw-record) o PulseAudio (parec) para grabar.".into()
        },
        backend: "PipeWire/PulseAudio",
    }
}

#[cfg(target_os = "linux")]
fn spawn_source(
    idx: usize,
    _device: Option<String>,
    tx: Sender<Msg>,
    stop: Arc<AtomicBool>,
    children: Arc<Mutex<Vec<std::process::Child>>>,
) -> Result<JoinHandle<()>> {
    use std::io::Read;
    use std::process::Stdio;
    let mut cmd = if command_exists("pw-record") {
        let mut c = crate::audio::hidden_command("pw-record");
        c.args(["--format=f32", "--rate=16000", "--channels=1", "--latency=50ms"]);
        if idx == SYS {
            c.args(["-P", "{ stream.capture.sink=true }"]);
        }
        c.arg("-");
        c
    } else if command_exists("parec") {
        let mut c = crate::audio::hidden_command("parec");
        c.args(["--raw", "--format=float32le", "--rate=16000", "--channels=1"]);
        c.arg(if idx == SYS { "--device=@DEFAULT_MONITOR@" } else { "--device=@DEFAULT_SOURCE@" });
        c
    } else {
        return Err(anyhow!("Se requiere PipeWire (pw-record) o PulseAudio (parec) para grabar"));
    };
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("No se pudo iniciar la captura: {e}"))?;
    let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("sin stdout"))?;
    let mut stderr = child.stderr.take();
    children.lock().unwrap().push(child);

    Ok(std::thread::spawn(move || {
        let mut buf = vec![0u8; 16 * 1024];
        let mut pending: Vec<u8> = Vec::new();
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    let usable = pending.len() / 4 * 4;
                    let samples: Vec<f32> = pending[..usable].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                    pending.drain(..usable);
                    if !samples.is_empty() && tx.send(Msg::Data(idx, samples)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if !stop.load(Ordering::Relaxed) {
            let mut msg = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut msg);
            }
            let msg = msg.trim();
            let _ = tx.send(Msg::Error(idx, if msg.is_empty() { "el proceso de captura terminó".into() } else { msg.to_string() }));
        }
    }))
}

// ---------------------------------------------------------------------------
// Windows / macOS: cpal
// ---------------------------------------------------------------------------
#[cfg(not(target_os = "linux"))]
pub fn list_devices() -> DeviceList {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    let mut inputs = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let Ok(id) = d.id() else { continue };
            let id = id.to_string();
            let name = d.description().map(|desc| desc.name().to_string()).unwrap_or_else(|_| id.clone());
            inputs.push(AudioDevice { is_default: default_id.as_deref() == Some(id.as_str()), id, name });
        }
    }
    inputs.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
    #[cfg(target_os = "windows")]
    let (system_capture, note) = ("native", "El audio del sistema se captura de la salida predeterminada (loopback WASAPI).".to_string());
    #[cfg(not(target_os = "windows"))]
    let (system_capture, note) = (
        "virtual",
        "macOS no permite capturar la salida directamente. Instala un dispositivo virtual (p. ej. BlackHole), crea un dispositivo de salida múltiple en Configuración de audio MIDI y elige BlackHole como micrófono para grabar la bocina.".to_string(),
    );
    DeviceList { inputs, system_capture, note, backend: "cpal" }
}

#[cfg(not(target_os = "linux"))]
fn spawn_source(
    idx: usize,
    device_id: Option<String>,
    tx: Sender<Msg>,
    stop: Arc<AtomicBool>,
    _children: Arc<Mutex<Vec<std::process::Child>>>,
) -> Result<JoinHandle<()>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = if idx == SYS {
        #[cfg(target_os = "windows")]
        {
            host.default_output_device().ok_or_else(|| anyhow!("No hay dispositivo de salida para capturar"))?
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err(anyhow!("En macOS el audio del sistema requiere un dispositivo virtual como BlackHole (elígelo como micrófono)"));
        }
    } else {
        let chosen = device_id.filter(|id| id != "default").and_then(|id| {
            host.input_devices().ok()?.find(|d| d.id().map(|x| x.to_string() == id).unwrap_or(false))
        });
        chosen.or_else(|| host.default_input_device()).ok_or_else(|| anyhow!("No hay micrófono disponible"))?
    };
    let supported = device.default_input_config().map_err(|e| anyhow!("El dispositivo no admite captura: {e}"))?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels.max(1) as usize;
    let rate = config.sample_rate;
    let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

    let handle = std::thread::spawn(move || {
        let mut rs = crate::audio::StreamResampler::new(rate, TARGET_RATE);
        let err_tx = tx.clone();
        let err_cb = move |e: cpal::StreamError| {
            let _ = err_tx.send(Msg::Error(idx, e.to_string()));
        };
        macro_rules! build {
            ($t:ty, $conv:expr) => {
                device.build_input_stream::<$t, _, _>(
                    config.clone(),
                    move |data: &[$t], _| {
                        let mono: Vec<f32> = data
                            .chunks_exact(channels)
                            .map(|f| f.iter().map(|&s| $conv(s)).sum::<f32>() / channels as f32)
                            .collect();
                        let mut out = Vec::with_capacity(mono.len());
                        rs.process(&mono, &mut out);
                        if !out.is_empty() {
                            let _ = tx.send(Msg::Data(idx, out));
                        }
                    },
                    err_cb,
                    None,
                )
                .map_err(|e| anyhow!("No se pudo abrir el dispositivo: {e}"))
            };
        }
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build!(f32, |s: f32| s),
            cpal::SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32768.0),
            cpal::SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
            cpal::SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2_147_483_648.0),
            other => Err(anyhow!("Formato de muestra no soportado: {other:?}")),
        };
        match stream.and_then(|s| s.play().map(|_| s).map_err(|e| anyhow!("No se pudo iniciar la captura: {e}"))) {
            Ok(stream) => {
                let _ = ready_tx.send(Ok(()));
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(handle),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow!("El dispositivo de audio no respondió")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Graba 2 s del micrófono y del sistema. Se omite salvo que IURE_TEST_RECORD=1.
    #[test]
    fn record_two_seconds() {
        if std::env::var("IURE_TEST_RECORD").as_deref() != Ok("1") {
            return;
        }
        let dir = std::env::temp_dir().join("iure-rec-test");
        let rec = Recorder::default();
        let path = rec
            .start(StartOptions {
                capture_mic: std::env::var("IURE_TEST_RECORD_MIC").as_deref() != Ok("0"),
                mic_device: None,
                capture_system: std::env::var("IURE_TEST_RECORD_SYS").as_deref() != Ok("0"),
                output_dir: dir,
            })
            .expect("start");
        let secs: f64 = std::env::var("IURE_TEST_RECORD_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);
        std::thread::sleep(Duration::from_secs_f64(secs + 0.2));
        let st = rec.status();
        assert!(st.active && st.elapsed_secs > secs);
        let res = rec.stop().expect("stop");
        eprintln!("grabado {} ({:.2}s)", res.path, res.duration_secs);
        assert!(res.duration_secs > secs - 0.5 && res.duration_secs < secs + 2.0, "duración {}", res.duration_secs);
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().sample_rate, TARGET_RATE);
        if std::env::var("IURE_TEST_KEEP").is_err() {
            let _ = std::fs::remove_file(path);
        }
    }
}
