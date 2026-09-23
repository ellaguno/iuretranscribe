//! `iuretranscribe-diarize <segmentación.onnx> <huellas.onnx> <audio.f32> [hablantes]`
//!
//! Lee PCM mono 16 kHz en f32 little-endian y escribe en stdout una línea por turno:
//! `<inicio_s> <fin_s> <hablante>`. Va en un proceso aparte porque onnxruntime
//! (estático) y whisper.cpp chocan en el mismo binario (std::regex / CRT de Windows).

use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig, OfflineSpeakerSegmentationModelConfig,
    OfflineSpeakerSegmentationPyannoteModelConfig, SpeakerEmbeddingExtractorConfig,
};
use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err("uso: iuretranscribe-diarize <segmentación.onnx> <huellas.onnx> <audio.f32> [hablantes]".into());
    }
    let num_speakers: i32 = args.get(4).and_then(|n| n.parse().ok()).filter(|n| *n > 0).unwrap_or(-1);
    let bytes = std::fs::read(&args[3]).map_err(|e| format!("No se pudo leer el audio: {e}"))?;
    let samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32;
    let config = OfflineSpeakerDiarizationConfig {
        segmentation: OfflineSpeakerSegmentationModelConfig {
            pyannote: OfflineSpeakerSegmentationPyannoteModelConfig { model: Some(args[1].clone()), ..Default::default() },
            num_threads: threads,
            ..Default::default()
        },
        embedding: SpeakerEmbeddingExtractorConfig { model: Some(args[2].clone()), num_threads: threads, ..Default::default() },
        clustering: FastClusteringConfig { num_clusters: num_speakers, ..Default::default() },
        ..Default::default()
    };
    let sd = OfflineSpeakerDiarization::create(&config).ok_or("No se pudieron cargar los modelos de hablantes")?;
    if sd.sample_rate() != 16000 {
        return Err(format!("Los modelos de hablantes esperan {} Hz", sd.sample_rate()));
    }
    let result = sd.process(&samples).ok_or("Falló la identificación de hablantes")?;
    let mut out = std::io::stdout().lock();
    for s in result.sort_by_start_time() {
        writeln!(out, "{:.3} {:.3} {}", s.start, s.end, s.speaker).map_err(|e| e.to_string())?;
    }
    Ok(())
}
