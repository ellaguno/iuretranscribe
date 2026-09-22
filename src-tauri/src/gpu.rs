//! Sonda de GPU para las variantes CUDA/Vulkan.
//!
//! ggml no devuelve un error cuando el driver de la máquina no sirve: aborta el
//! proceso entero. Para que la app no «desaparezca», la primera vez que se va a
//! usar el modelo se lanza un proceso hijo (`iuretranscribe --gpu-probe …`) que
//! carga el modelo y transcribe un segundo de silencio, primero con GPU y, si
//! muere, sólo con procesador. El resultado se guarda en los ajustes y se repite
//! al cambiar de versión de la app.

use crate::settings::Settings;
use crate::transcribe;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const RELEASES_URL: &str = "https://github.com/ellaguno/iuretranscribe/releases/latest";

/// Tiempo máximo para que el hijo cargue el modelo y transcriba (discos lentos).
const PROBE_TIMEOUT: Duration = Duration::from_secs(240);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuNotice {
    /// "cpu" = sigue funcionando sin GPU; "none" = esta variante no sirve aquí.
    pub level: String,
    pub message: String,
    pub url: String,
}

/// Argumentos del proceso hijo: `--gpu-probe <modelo> gpu|cpu`. Devuelve `Some(código)`
/// si esta ejecución es una sonda y ya terminó.
pub fn handle_probe_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--gpu-probe") {
        return None;
    }
    let model = args.get(2)?;
    let use_gpu = args.get(3).map(String::as_str) != Some("cpu");
    match transcribe::probe(Path::new(model), use_gpu) {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("sonda de GPU: {e:#}");
            Some(2)
        }
    }
}

/// Ejecuta la sonda en un proceso hijo. `true` si terminó bien.
fn run_child(model_path: &Path, use_gpu: bool) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut cmd = crate::audio::hidden_command(exe.to_string_lossy().as_ref());
    cmd.arg("--gpu-probe").arg(model_path).arg(if use_gpu { "gpu" } else { "cpu" });
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("no se pudo lanzar la sonda de GPU: {e}");
            return false;
        }
    };
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut e) = stderr {
            use std::io::Read;
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if started.elapsed() > PROBE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => break None,
        }
    };
    let err = reader.join().unwrap_or_default();
    let ok = status.map(|s| s.success()).unwrap_or(false);
    let mode = if use_gpu { "GPU" } else { "CPU" };
    match status {
        Some(s) if s.success() => log::info!("sonda de {mode}: correcta"),
        Some(s) => log::warn!("sonda de {mode}: falló ({s}); {}", err.trim()),
        None => log::warn!("sonda de {mode}: sin respuesta en {}s", PROBE_TIMEOUT.as_secs()),
    }
    ok
}

/// Nombre del instalador sin GPU para esta plataforma (como aparece en la release).
fn cpu_variant_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os}-{arch}")
}

fn notice_for(result: &str) -> Option<GpuNotice> {
    let backend = transcribe::backend_name();
    let variant = cpu_variant_name();
    match result {
        "cpu" => Some(GpuNotice {
            level: "cpu".into(),
            message: format!(
                "Esta computadora no puede usar la aceleración {backend}. IureTranscribe seguirá funcionando con el procesador; para esta máquina conviene instalar la versión «{variant}» (sin GPU), más ligera."
            ),
            url: RELEASES_URL.into(),
        }),
        "none" => Some(GpuNotice {
            level: "none".into(),
            message: format!(
                "Esta versión de IureTranscribe ({backend}) no funciona en esta computadora: el motor no arranca ni con GPU ni con procesador. Instala la versión «{variant}» (sin GPU) desde {RELEASES_URL}."
            ),
            url: RELEASES_URL.into(),
        }),
        _ => None,
    }
}

/// Decide si se puede pedir GPU para esta transcripción.
///
/// En variantes sin GPU devuelve `wanted` tal cual. En CUDA/Vulkan ejecuta la sonda
/// (una vez por versión), avisa a la interfaz si el resultado no es «gpu» y devuelve
/// `Err` con el mensaje si la variante no sirve en esta máquina.
pub fn effective_use_gpu(app: &AppHandle, settings: &Mutex<Settings>, settings_path: &Path, model_path: &Path, wanted: bool) -> Result<bool, String> {
    if !transcribe::is_gpu_variant() {
        return Ok(wanted);
    }
    let version = env!("CARGO_PKG_VERSION");
    let cached = {
        let s = settings.lock().unwrap();
        if s.gpu_probe_version == version && !s.gpu_probe_result.is_empty() {
            Some(s.gpu_probe_result.clone())
        } else {
            None
        }
    };
    let result = match cached {
        Some(r) => r,
        None => {
            let _ = app.emit("gpu-probe", "start");
            let r = if run_child(model_path, true) {
                "gpu"
            } else if run_child(model_path, false) {
                "cpu"
            } else {
                "none"
            };
            let mut s = settings.lock().unwrap();
            s.gpu_probe_result = r.into();
            s.gpu_probe_version = version.into();
            if let Err(e) = s.save(settings_path) {
                log::warn!("no se pudo guardar el resultado de la sonda de GPU: {e}");
            }
            r.to_string()
        }
    };
    if let Some(n) = notice_for(&result) {
        let _ = app.emit("gpu-notice", &n);
        if result == "none" {
            return Err(n.message);
        }
        return Ok(false);
    }
    Ok(wanted)
}
