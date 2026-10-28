//! Escritura del WAV de una grabación a prueba de disco lleno.

use crate::audio::TARGET_RATE;
use iurefficient_connect::tr;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};

/// WAV mono de 16 bits a 16 kHz que no pierde audio si una escritura falla (p. ej. el disco
/// se llena un momento): lo no escrito queda en memoria y se reintenta en cada vuelta. La
/// cabecera se actualiza cada pocos segundos para que el archivo sirva aunque la app se cierre.
pub struct WavSink<F: SinkFile = File> {
    file: F,
    /// Bytes de audio ya escritos tras la cabecera.
    committed: u64,
    pending: Vec<u8>,
    header_at: Instant,
    /// Muestras descartadas porque el búfer en memoria se llenó.
    dropped: u64,
}

const WAV_HEADER_LEN: u64 = 44;
/// Una hora de audio (≈115 MB) en memoria mientras no se pueda escribir.
const MAX_PENDING_BYTES: usize = TARGET_RATE as usize * 2 * 3600;

fn wav_header(data_len: u64) -> [u8; 44] {
    let data = data_len.min(u32::MAX as u64 - 36) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    h[24..28].copy_from_slice(&TARGET_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(TARGET_RATE * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

/// Lo que `WavSink` necesita del archivo (las pruebas usan uno en memoria que falla).
pub trait SinkFile: Write + Seek {
    fn set_len(&self, len: u64) -> std::io::Result<()>;
    fn sync_all(&self) -> std::io::Result<()>;
}

impl SinkFile for File {
    fn set_len(&self, len: u64) -> std::io::Result<()> {
        File::set_len(self, len)
    }
    fn sync_all(&self) -> std::io::Result<()> {
        File::sync_all(self)
    }
}

impl WavSink {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        Self::new(File::create(path)?)
    }
}

impl<F: SinkFile> WavSink<F> {
    fn new(mut file: F) -> std::io::Result<Self> {
        file.write_all(&wav_header(0))?;
        Ok(Self { file, committed: 0, pending: Vec::new(), header_at: Instant::now(), dropped: 0 })
    }

    pub fn push(&mut self, s: i16) {
        if self.pending.len() >= MAX_PENDING_BYTES {
            self.dropped += 1;
        } else {
            self.pending.extend_from_slice(&s.to_le_bytes());
        }
    }

    /// Escribe lo pendiente. Si falla, recorta el archivo a lo último completo (una escritura
    /// parcial desalinearía las muestras) y conserva el búfer para el siguiente intento.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let file = &mut self.file;
        let res = file.seek(SeekFrom::Start(WAV_HEADER_LEN + self.committed)).and_then(|_| file.write_all(&self.pending));
        if let Err(e) = res {
            let _ = self.file.set_len(WAV_HEADER_LEN + self.committed);
            return Err(e);
        }
        self.committed += self.pending.len() as u64;
        self.pending.clear();
        if self.header_at.elapsed() >= Duration::from_secs(5) {
            let _ = self.write_header();
        }
        Ok(())
    }

    fn write_header(&mut self) -> std::io::Result<()> {
        self.header_at = Instant::now();
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&wav_header(self.committed))
    }

    pub fn dropped_secs(&self) -> f64 {
        self.dropped as f64 / TARGET_RATE as f64
    }

    /// Escribe lo pendiente y la cabecera final. Si el disco sigue sin espacio y se da
    /// `rescue`, guarda lo que quedó en memoria en la carpeta temporal y dice dónde.
    /// Devuelve las muestras que quedaron en el archivo.
    pub fn finish(mut self, rescue: Option<&Path>) -> (u64, std::result::Result<(), String>) {
        let flushed = self.flush();
        let header = self.write_header().and_then(|_| self.file.sync_all());
        (self.committed / 2, flushed.and(header).map_err(|e| self.rescue(e, rescue)))
    }

    fn rescue(&self, e: std::io::Error, rescue: Option<&Path>) -> String {
        let (Some(original), false) = (rescue, self.pending.is_empty()) else {
            return tr!("Could not finish writing the recording: {e}", "No se pudo terminar de escribir la grabación: {e}");
        };
        let secs = self.pending.len() as f64 / 2.0 / TARGET_RATE as f64;
        let name = original.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "grabacion".into());
        let spill = std::env::temp_dir().join(format!("{name} (resto).wav"));
        let saved = File::create(&spill).and_then(|mut f| {
            f.write_all(&wav_header(self.pending.len() as u64))?;
            f.write_all(&self.pending)?;
            f.sync_all()
        });
        match saved {
            Ok(()) => tr!(
                "Could not finish writing the recording ({e}). The last {:.0} s were saved separately in {}.",
                "No se pudo terminar de escribir la grabación ({e}). Los últimos {:.0} s se guardaron aparte en {}.",
                secs,
                spill.display()
            ),
            Err(e2) => tr!(
                "Could not finish writing the recording ({e}); the last {:.0} s were lost ({e2}).",
                "No se pudo terminar de escribir la grabación ({e}); se perdieron los últimos {:.0} s ({e2}).",
                secs
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// Archivo en memoria con un tope de tamaño, como un disco que se llena.
    #[derive(Clone)]
    struct Disk {
        data: Rc<RefCell<Vec<u8>>>,
        cap: Rc<Cell<usize>>,
        pos: usize,
    }

    impl Disk {
        fn new(cap: usize) -> Self {
            Self { data: Rc::default(), cap: Rc::new(Cell::new(cap)), pos: 0 }
        }
    }

    impl Write for Disk {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let room = self.cap.get().saturating_sub(self.pos);
            if room == 0 && !buf.is_empty() {
                return Err(std::io::Error::new(std::io::ErrorKind::StorageFull, "No space left on device"));
            }
            let n = room.min(buf.len());
            let mut data = self.data.borrow_mut();
            if data.len() < self.pos + n {
                data.resize(self.pos + n, 0);
            }
            data[self.pos..self.pos + n].copy_from_slice(&buf[..n]);
            self.pos += n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Seek for Disk {
        fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
            self.pos = match to {
                SeekFrom::Start(n) => n as usize,
                SeekFrom::End(d) => (self.data.borrow().len() as i64 + d) as usize,
                SeekFrom::Current(d) => (self.pos as i64 + d) as usize,
            };
            Ok(self.pos as u64)
        }
    }

    impl SinkFile for Disk {
        fn set_len(&self, len: u64) -> std::io::Result<()> {
            self.data.borrow_mut().resize(len as usize, 0);
            Ok(())
        }
        fn sync_all(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn read_wav(bytes: Vec<u8>) -> Vec<i16> {
        let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).expect("WAV válido");
        assert_eq!(reader.spec().sample_rate, TARGET_RATE);
        reader.into_samples::<i16>().map(|s| s.unwrap()).collect()
    }

    /// El disco se llena a media escritura y luego se libera: no se pierde ni se desalinea nada.
    #[test]
    fn wav_sink_survives_full_disk() {
        let disk = Disk::new(44 + 1001); // número impar: la escritura parcial parte una muestra
        let mut sink = WavSink::new(disk.clone()).unwrap();
        let want: Vec<i16> = (0..3000).map(|i| (i * 7 - 9000) as i16).collect();
        for &s in &want[..1000] {
            sink.push(s);
        }
        assert!(sink.flush().is_err());
        assert_eq!(disk.data.borrow().len(), 44, "se recorta lo escrito a medias");
        for &s in &want[1000..2000] {
            sink.push(s);
        }
        assert!(sink.flush().is_err());
        disk.cap.set(usize::MAX);
        sink.flush().unwrap();
        for &s in &want[2000..] {
            sink.push(s);
        }
        let (samples, res) = sink.finish(None);
        res.unwrap();
        assert_eq!(samples, 3000);
        assert_eq!(read_wav(disk.data.borrow().clone()), want);
    }

    /// Si al detener sigue lleno, lo que quedó en memoria se guarda aparte y lo escrito vale.
    #[test]
    fn wav_sink_rescues_pending_audio() {
        let disk = Disk::new(44 + 400);
        let mut sink = WavSink::new(disk.clone()).unwrap();
        for i in 0..500 {
            sink.push(i as i16);
        }
        let original = std::env::temp_dir().join(format!("iure-rescate-{}.wav", std::process::id()));
        let spill = std::env::temp_dir().join(format!("iure-rescate-{} (resto).wav", std::process::id()));
        let (samples, res) = sink.finish(Some(&original));
        let msg = res.unwrap_err();
        assert!(msg.contains(&spill.display().to_string()), "{msg}");
        assert_eq!(samples, 0);
        assert_eq!(read_wav(std::fs::read(&spill).unwrap()), (0..500).map(|i| i as i16).collect::<Vec<_>>());
        let _ = std::fs::remove_file(&spill);
    }

    /// La cabecera se reescribe durante la grabación: un archivo abierto a medias se puede leer.
    #[test]
    fn wav_sink_header_is_kept_up_to_date() {
        let disk = Disk::new(usize::MAX);
        let mut sink = WavSink::new(disk.clone()).unwrap();
        for i in 0..100 {
            sink.push(i);
        }
        sink.header_at -= Duration::from_secs(10);
        sink.flush().unwrap();
        assert_eq!(read_wav(disk.data.borrow().clone()).len(), 100);
    }
}
