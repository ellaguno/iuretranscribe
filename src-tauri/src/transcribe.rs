//! Transcripción con whisper.cpp (whisper-rs), con caché del modelo cargado.

use crate::subtitles::Segment;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct CachedModel {
    pub path: PathBuf,
    pub use_gpu: bool,
    pub ctx: Arc<WhisperContext>,
}

#[derive(Default)]
pub struct Engine {
    cache: Mutex<Option<CachedModel>>,
    /// Sólo una transcripción a la vez.
    busy: Mutex<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: String,
    /// "decoding" | "loading" | "transcribing"
    pub stage: String,
    pub percent: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentEvent {
    pub job_id: String,
    pub segment: Segment,
}

/// Eventos que emite el motor durante una transcripción.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Progress(ProgressEvent),
    Segment(SegmentEvent),
}

pub type EventSink = Arc<dyn Fn(EngineEvent) + Send + Sync>;

#[derive(Clone)]
pub struct Options {
    pub job_id: String,
    pub input: PathBuf,
    pub model_path: PathBuf,
    pub language: String,
    pub translate: bool,
    pub use_gpu: bool,
    pub threads: u32,
    pub beam_size: u32,
    pub initial_prompt: Option<String>,
}

pub struct Output {
    pub segments: Vec<Segment>,
    pub audio_secs: f64,
    pub detected_language: Option<String>,
}

impl Engine {
    fn context(&self, path: &Path, use_gpu: bool) -> Result<Arc<WhisperContext>> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(c) = cache.as_ref() {
            if c.path == path && c.use_gpu == use_gpu {
                return Ok(c.ctx.clone());
            }
        }
        *cache = None; // libera el modelo anterior antes de cargar otro
        let mut params = WhisperContextParameters::default();
        params.use_gpu(use_gpu);
        params.flash_attn(use_gpu);
        let ctx = WhisperContext::new_with_params(
            path.to_str().ok_or_else(|| anyhow!("ruta de modelo no válida"))?,
            params,
        )
        .map_err(|e| anyhow!("No se pudo cargar el modelo {}: {e}", path.display()))?;
        let ctx = Arc::new(ctx);
        *cache = Some(CachedModel { path: path.to_path_buf(), use_gpu, ctx: ctx.clone() });
        Ok(ctx)
    }

    pub fn unload(&self) {
        *self.cache.lock().unwrap() = None;
    }

    pub fn run(&self, sink: EventSink, opts: Options, cancel: Arc<AtomicBool>) -> Result<Output> {
        let job_id = opts.job_id.clone();
        sink(EngineEvent::Progress(ProgressEvent { job_id: job_id.clone(), stage: "decoding".into(), percent: 0 }));
        let samples = crate::audio::decode_to_pcm16k(&opts.input)
            .with_context(|| format!("Error al leer {}", opts.input.display()))?;
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("Cancelado"));
        }
        if samples.len() < crate::audio::TARGET_RATE as usize / 2 {
            return Err(anyhow!("El archivo no contiene audio suficiente para transcribir"));
        }
        self.transcribe_pcm(sink, &opts, &samples, cancel, false)
    }

    /// Transcribe PCM mono 16 kHz ya decodificado.
    ///
    /// En modo `live` (bloques de una grabación en curso) no se emiten eventos de
    /// progreso, se usa decodificación rápida y se descartan segmentos sin voz.
    pub fn transcribe_pcm(&self, sink: EventSink, opts: &Options, samples: &[f32], cancel: Arc<AtomicBool>, live: bool) -> Result<Output> {
        let _guard = self.busy.lock().unwrap();
        let job_id = opts.job_id.clone();
        let audio_secs = samples.len() as f64 / crate::audio::TARGET_RATE as f64;
        let progress = |stage: &str, percent: i32| {
            if !live {
                sink(EngineEvent::Progress(ProgressEvent { job_id: job_id.clone(), stage: stage.into(), percent }));
            }
        };

        progress("loading", 0);
        let ctx = self.context(&opts.model_path, opts.use_gpu)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("Cancelado"));
        }
        let mut state = ctx.create_state().map_err(|e| anyhow!("No se pudo inicializar whisper: {e}"))?;

        let beam = if live { 1 } else { opts.beam_size };
        let strategy = if beam > 1 {
            SamplingStrategy::BeamSearch { beam_size: beam as i32, patience: -1.0 }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        };
        let mut params = FullParams::new(strategy);
        let threads = if opts.threads == 0 {
            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32
        } else {
            opts.threads as i32
        };
        params.set_n_threads(threads);
        params.set_translate(opts.translate);
        let lang_owned;
        if opts.language == "auto" || opts.language.is_empty() {
            params.set_language(None);
            params.set_detect_language(true);
        } else {
            lang_owned = opts.language.clone();
            params.set_language(Some(lang_owned.as_str()));
        }
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_token_timestamps(false);
        if live {
            // Se conserva el reintento con temperatura (0.2 por defecto): es lo que corta
            // los bucles de greedy ("sí, sí, sí…") cuando la compresión o la entropía fallan.
            params.set_suppress_nst(true);
            params.set_no_context(true);
        }
        if let Some(p) = opts.initial_prompt.as_deref().filter(|p| !p.trim().is_empty()) {
            params.set_initial_prompt(p);
        }

        if !live {
            {
                let sink = sink.clone();
                let job_id = job_id.clone();
                params.set_progress_callback_safe(move |p: i32| {
                    sink(EngineEvent::Progress(ProgressEvent { job_id: job_id.clone(), stage: "transcribing".into(), percent: p }));
                });
            }
            {
                let sink = sink.clone();
                let job_id = job_id.clone();
                params.set_segment_callback_safe_lossy(move |d: whisper_rs::SegmentCallbackData| {
                    let seg = Segment { start_ms: d.start_timestamp * 10, end_ms: d.end_timestamp * 10, text: d.text.trim().to_string(), speaker: None };
                    sink(EngineEvent::Segment(SegmentEvent { job_id: job_id.clone(), segment: seg }));
                });
            }
        }
        // Callback de aborto propio: `set_abort_callback_safe` de whisper-rs 0.16 castea mal
        // el puntero de la clausura y aborta de forma aleatoria ("failed to encode").
        // `cancel` vive hasta el final de esta función, así que el puntero es válido.
        unsafe extern "C" fn abort_trampoline(data: *mut std::ffi::c_void) -> bool {
            unsafe { (*(data as *const AtomicBool)).load(Ordering::Relaxed) }
        }
        unsafe {
            params.set_abort_callback(Some(abort_trampoline));
            params.set_abort_callback_user_data(Arc::as_ptr(&cancel) as *mut std::ffi::c_void);
        }

        progress("transcribing", 0);
        state
            .full(params, samples)
            .map_err(|e| anyhow!("whisper falló: {e}"))?;
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow!("Cancelado"));
        }

        let mut segments = Vec::new();
        for seg in state.as_iter() {
            let text = seg.to_str_lossy().map(|c| c.trim().to_string()).unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            if live && (seg.no_speech_probability() > 0.7 || is_non_speech_marker(&text) || wrong_script(&opts.language, &text)) {
                continue;
            }
            let Some(text) = clean_repetitions(&text) else { continue };
            segments.push(Segment { start_ms: seg.start_timestamp() * 10, end_ms: seg.end_timestamp() * 10, text, speaker: None });
        }
        let detected_language = {
            let id = state.full_lang_id_from_state();
            if id >= 0 { whisper_rs::get_lang_str(id).map(|s| s.to_string()) } else { None }
        };
        progress("transcribing", 100);
        Ok(Output { segments, audio_secs, detected_language })
    }
}

/// Textos que whisper produce ante silencio o música ("[MÚSICA]", "(Aplausos)", …).
fn is_non_speech_marker(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')')) || t.starts_with('♪')
}

/// Con un idioma de alfabeto latino fijado, un segmento mayoritariamente en otro
/// alfabeto ("оном") es ruido que el reintento con temperatura convirtió en texto.
fn wrong_script(language: &str, text: &str) -> bool {
    const LATIN: &[&str] = &["es", "en", "pt", "fr", "it", "de", "ca", "gl", "eu", "nl", "ro"];
    if !LATIN.contains(&language) {
        return false;
    }
    let (mut latin, mut other) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        if (c as u32) <= 0x024F { latin += 1 } else { other += 1 }
    }
    other > latin
}

/// Normaliza una palabra para comparar repeticiones ("Sí," == "sí").
fn norm_word(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Recorta los bucles de whisper: una palabra repetida más de 3 veces seguidas o una
/// frase de 2 a 4 palabras repetida más de 2 veces se deja en ese máximo. Devuelve
/// `None` si el segmento era sólo el bucle (p. ej. "no, no, no, no, no, no…").
pub fn clean_repetitions(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let keys: Vec<String> = words.iter().map(|w| norm_word(w)).collect();
    let mut out: Vec<usize> = Vec::with_capacity(words.len());
    let mut removed = 0usize;
    let mut i = 0;
    'outer: while i < words.len() {
        for n in 1..=4usize {
            if keys[i..].len() < n * 2 || keys[i..i + n].iter().any(|k| k.is_empty()) {
                continue;
            }
            let mut reps = 1;
            while i + (reps + 1) * n <= keys.len() && keys[i + reps * n..i + (reps + 1) * n] == keys[i..i + n] {
                reps += 1;
            }
            let keep = if n == 1 { 3 } else { 2 };
            if reps > keep {
                // Las primeras copias y la última, cuya puntuación enlaza con lo que sigue.
                out.extend(i..i + (keep - 1) * n);
                out.extend(i + (reps - 1) * n..i + reps * n);
                removed += (reps - keep) * n;
                i += reps * n;
                continue 'outer;
            }
        }
        out.push(i);
        i += 1;
    }
    if removed == 0 {
        return Some(text.to_string());
    }
    let distinct: std::collections::HashSet<&String> = keys.iter().filter(|k| !k.is_empty()).collect();
    if removed >= 4 && distinct.len() <= 4 {
        return None;
    }
    let mut cleaned = out.iter().map(|&j| words[j]).collect::<Vec<_>>().join(" ");
    // Si el bucle quedó al final, "sí, sí, sí," termina en coma: se cierra con "…".
    if cleaned.ends_with(',') {
        cleaned.pop();
        cleaned.push('…');
    }
    Some(cleaned)
}

/// Variantes cuyo backend puede abortar el proceso (no devolver error) si el driver
/// de la máquina no sirve: se comprueban en un proceso hijo antes de usarlas.
pub fn is_gpu_variant() -> bool {
    cfg!(feature = "cuda") || cfg!(feature = "vulkan")
}

/// Sonda de compatibilidad: carga el modelo, inicializa el backend y transcribe un
/// segundo de silencio. Se ejecuta en un proceso hijo (`--gpu-probe`) porque si el
/// driver de GPU falla, ggml aborta el proceso en lugar de devolver un error.
pub fn probe(model_path: &Path, use_gpu: bool) -> Result<()> {
    let mut params = WhisperContextParameters::default();
    params.use_gpu(use_gpu);
    params.flash_attn(use_gpu);
    let ctx = WhisperContext::new_with_params(model_path.to_str().ok_or_else(|| anyhow!("ruta de modelo no válida"))?, params)
        .map_err(|e| anyhow!("No se pudo cargar el modelo: {e}"))?;
    let mut state = ctx.create_state().map_err(|e| anyhow!("No se pudo inicializar whisper: {e}"))?;
    let mut full = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    full.set_n_threads(2);
    full.set_language(Some("es"));
    full.set_print_special(false);
    full.set_print_progress(false);
    full.set_print_realtime(false);
    full.set_print_timestamps(false);
    let silence = vec![0.0f32; crate::audio::TARGET_RATE as usize];
    state.full(full, &silence).map_err(|e| anyhow!("whisper falló: {e}"))?;
    Ok(())
}

pub fn backend_name() -> &'static str {
    if cfg!(feature = "cuda") {
        "CUDA"
    } else if cfg!(feature = "metal") {
        "Metal"
    } else if cfg!(feature = "coreml") {
        "CoreML"
    } else if cfg!(feature = "vulkan") {
        "Vulkan"
    } else if cfg!(feature = "openblas") {
        "CPU (OpenBLAS)"
    } else {
        "CPU"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn wrong_script_detects_noise() {
        assert!(wrong_script("es", "оном"));
        assert!(!wrong_script("es", "Sí, ya se ve. ¿Qué tal?"));
        assert!(!wrong_script("ru", "оном"));
        assert!(!wrong_script("auto", "оном"));
    }

    #[test]
    fn repetitions_are_trimmed() {
        assert_eq!(clean_repetitions("Hola, ¿cómo estás?").as_deref(), Some("Hola, ¿cómo estás?"));
        assert_eq!(clean_repetitions("No, no, no, eso no.").as_deref(), Some("No, no, no, eso no."));
        assert_eq!(clean_repetitions("Sí, sí, sí, sí, sí, sí, sí, sí, sí, sí,"), None);
        assert_eq!(clean_repetitions("no no no no no no no no"), None);
        assert_eq!(
            clean_repetitions("Entonces lo revisamos mañana y sí, sí, sí, sí, sí, sí, sí, sí,").as_deref(),
            Some("Entonces lo revisamos mañana y sí, sí, sí…")
        );
        assert_eq!(
            clean_repetitions("Le digo que no sé, no sé, no sé, no sé, no sé qué pasó con el contrato").as_deref(),
            Some("Le digo que no sé, no sé qué pasó con el contrato")
        );
    }

    /// Prueba de extremo a extremo: requiere IURE_TEST_MODEL (ruta a un ggml-*.bin)
    /// e IURE_TEST_AUDIO (archivo de audio con voz). Se omite si no están definidos.
    #[test]
    fn transcribe_real_audio() {
        let (Ok(model), Ok(audio)) = (std::env::var("IURE_TEST_MODEL"), std::env::var("IURE_TEST_AUDIO")) else {
            eprintln!("omitido: define IURE_TEST_MODEL e IURE_TEST_AUDIO");
            return;
        };
        let events: Arc<Mutex<Vec<EngineEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: EventSink = {
            let events = events.clone();
            Arc::new(move |e| events.lock().unwrap().push(e))
        };
        let engine = Engine::default();
        let out = engine
            .run(
                sink,
                Options {
                    job_id: "t".into(),
                    input: PathBuf::from(audio),
                    model_path: PathBuf::from(model),
                    language: "es".into(),
                    translate: false,
                    use_gpu: false,
                    threads: 0,
                    beam_size: 1,
                    initial_prompt: None,
                },
                Arc::new(AtomicBool::new(false)),
            )
            .expect("la transcripción falló");
        let text = crate::subtitles::to_plain(&out.segments);
        eprintln!("audio: {:.1}s, segmentos: {}, texto: {text}", out.audio_secs, out.segments.len());
        assert!(out.audio_secs > 1.0);
        assert!(!out.segments.is_empty(), "sin segmentos");
        assert!(text.chars().count() > 10, "texto demasiado corto: {text}");
        let ev = events.lock().unwrap();
        assert!(ev.iter().any(|e| matches!(e, EngineEvent::Segment(_))), "no se emitieron segmentos en vivo");
        assert!(ev.iter().any(|e| matches!(e, EngineEvent::Progress(p) if p.stage == "transcribing")));
    }
}
