//! Buzón de trabajos para la ventana.
//!
//! El servidor MCP (`--mcp`, el que usa IureDesk) graba y transcribe en otro proceso. La
//! lista de la ventana vive en su memoria y la escribe entera en `jobs.json`, así que no se
//! puede tocar ese archivo desde fuera: la ventana lo reescribiría. En su lugar, cada
//! cambio se deja aquí como un archivo pequeño en `jobs-inbox/`:
//!
//! - el MCP escribe `<ms>-<pid>-<n>.json.tmp` y lo renombra a `.json` (atómico: nunca se
//!   lee a medias);
//! - la ventana lista los `.json` en orden, los aplica a su lista (actualiza el trabajo con
//!   esa ruta o lo agrega), guarda `jobs.json` y sólo entonces borra exactamente los
//!   archivos que leyó. Lo que llegue mientras tanto es otro archivo y espera a la
//!   siguiente pasada. Sin candados y sin perder nada, con la ventana abierta o cerrada.
//!
//! Todo lo que se lee de aquí es hostil: tamaño acotado, nombres validados, campos
//! comprobados; lo que no cumple se descarta (y se borra) sin tumbar nada.

#[cfg(test)]
use crate::subtitles::Segment;
use crate::TranscriptResult;
use iurefficient_connect::tr;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const DIR_NAME: &str = "jobs-inbox";
/// Tamaño máximo de un registro (con los segmentos de varias horas de reunión cabe de sobra).
pub const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;
/// Registros por pasada (la ventana repite si quedan más).
pub const MAX_BATCH: usize = 100;
/// Máximo de caracteres de cada campo de los datos de la reunión.
pub const MAX_META_CHARS: usize = 4_000;
const MAX_PATH_CHARS: usize = 4_096;
/// `jobs.json` más grande que esto no se lee para buscar datos de la reunión.
const MAX_JOBS_JSON_BYTES: u64 = 64 * 1024 * 1024;
/// Estados que puede traer un registro.
pub const STATUSES: [&str; 5] = ["recording", "recorded", "transcribing", "done", "error"];

static SEQ: AtomicU64 = AtomicU64::new(1);

/// Datos de la reunión (los mismos campos que `JobMeta` en la ventana, más el título).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Meta {
    pub title: String,
    pub date: String,
    pub place: String,
    /// Uno por línea, como los guarda la ventana.
    pub participants: String,
    pub notes: String,
}

impl Meta {
    pub fn is_empty(&self) -> bool {
        [&self.title, &self.date, &self.place, &self.participants, &self.notes].iter().all(|s| s.trim().is_empty())
    }

    /// Los datos que vienen en los argumentos de una herramienta MCP (`participants` como
    /// texto o como lista). Cada campo se recorta a `MAX_META_CHARS`.
    pub fn from_args(args: &Value) -> Meta {
        let field = |k: &str| bounded(args.get(k).and_then(Value::as_str).unwrap_or("").trim());
        let participants = match args.get("participants") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).take(200).collect::<Vec<_>>().join("\n"),
            Some(Value::String(s)) => split_people(s).join("\n"),
            _ => String::new(),
        };
        Meta { title: field("title"), date: field("date"), place: field("place"), participants: bounded(&participants), notes: field("notes") }
    }

    /// Contexto para el resumen o la minuta, con las mismas etiquetas que la ventana.
    pub fn context(&self) -> String {
        let mut lines = Vec::new();
        let mut push = |v: &str, line: String| {
            if !v.trim().is_empty() {
                lines.push(line);
            }
        };
        let (title, date, place, notes) = (self.title.trim(), self.date.trim(), self.place.trim(), self.notes.trim());
        push(title, tr!("Title: {title}", "Título: {title}"));
        push(date, tr!("Date: {date}", "Fecha: {date}"));
        push(place, tr!("Place: {place}", "Lugar: {place}"));
        let people = split_people(&self.participants).join(", ");
        push(&people, tr!("Participants: {people}", "Participantes: {people}"));
        push(notes, tr!("Additional notes: {notes}", "Notas adicionales: {notes}"));
        lines.join("\n")
    }
}

fn split_people(s: &str) -> Vec<&str> {
    s.split(['\n', ',', ';']).map(str::trim).filter(|x| !x.is_empty()).take(200).collect()
}

fn bounded(s: &str) -> String {
    s.chars().take(MAX_META_CHARS).collect()
}

/// Un cambio de un trabajo. Sólo `path` es obligatorio; lo demás, lo que se sepa.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// Ruta absoluta del audio (identifica el trabajo).
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// Uno de `STATUSES`; sin él, el registro sólo añade datos (p. ej. la ruta de la minuta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// La transcripción terminada, con la misma forma que la de la ventana.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TranscriptResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes_path: Option<String>,
    /// Cuándo se escribió (ms desde 1970).
    #[serde(default)]
    pub at: i64,
}

impl Record {
    pub fn new(path: &Path, status: Option<&str>) -> Record {
        Record {
            path: path.to_string_lossy().into_owned(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            status: status.map(str::to_string),
            at: now_ms(),
            ..Record::default()
        }
    }

    /// ¿Se puede aplicar sin riesgo? Ruta absoluta y acotada, estado conocido, textos acotados.
    fn sanitize(mut self) -> Option<Record> {
        if self.path.is_empty() || self.path.chars().count() > MAX_PATH_CHARS || !Path::new(&self.path).is_absolute() {
            return None;
        }
        if self.status.as_deref().is_some_and(|s| !STATUSES.contains(&s)) {
            return None;
        }
        let fix_path = |p: Option<String>| p.filter(|p| p.chars().count() <= MAX_PATH_CHARS && Path::new(p).is_absolute());
        self.summary_path = fix_path(self.summary_path);
        self.minutes_path = fix_path(self.minutes_path);
        self.name = self.name.map(|n| n.chars().take(512).collect());
        self.error = self.error.map(|e| e.chars().take(MAX_META_CHARS).collect());
        self.meta = self.meta.map(|m| Meta {
            title: bounded(&m.title),
            date: bounded(&m.date),
            place: bounded(&m.place),
            participants: bounded(&m.participants),
            notes: bounded(&m.notes),
        });
        self.duration_secs = self.duration_secs.filter(|d| d.is_finite() && *d >= 0.0);
        if let Some(r) = self.result.as_mut() {
            if !r.audio_secs.is_finite() || r.audio_secs < 0.0 {
                r.audio_secs = 0.0;
            }
            if !r.elapsed_secs.is_finite() || r.elapsed_secs < 0.0 {
                r.elapsed_secs = 0.0;
            }
        }
        Some(self)
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)).unwrap_or(0)
}

/// Deja un registro en el buzón. Si con los segmentos no cabe, se envía sin la
/// transcripción (la ventana la lee del disco).
pub fn append(dir: &Path, rec: &Record) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| tr!("Could not create {}: {e}", "No se pudo crear {}: {e}", dir.display()))?;
    let mut json = serde_json::to_vec(rec).map_err(|e| e.to_string())?;
    if json.len() as u64 > MAX_RECORD_BYTES {
        let slim = Record { result: None, ..rec.clone() };
        json = serde_json::to_vec(&slim).map_err(|e| e.to_string())?;
        if json.len() as u64 > MAX_RECORD_BYTES {
            return Err(tr!("The record is too large.", "El registro es demasiado grande."));
        }
    }
    let name = format!("{:013}-{}-{}.json", now_ms().max(0), std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed));
    let path = dir.join(&name);
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, &json).map_err(|e| tr!("Could not write {}: {e}", "No se pudo escribir {}: {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(tr!("Could not write {}: {e}", "No se pudo escribir {}: {e}", path.display()));
    }
    Ok(path)
}

/// Lo mismo que `append`, pero un fallo sólo se registra: el buzón nunca detiene una
/// transcripción ni una grabación.
pub fn post(dir: Option<&Path>, rec: &Record) {
    let Some(dir) = dir else { return };
    if let Err(e) = append(dir, rec) {
        log::warn!("buzón de trabajos: {e}");
    }
}

/// Un registro leído, con el nombre del archivo para confirmarlo con `ack`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub file: String,
    pub record: Record,
}

/// Nombre de un registro escrito por `append` (sólo dígitos y guiones + `.json`).
fn valid_name(n: &str) -> bool {
    n.len() <= 80
        && n.strip_suffix(".json").is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
}

/// Lee hasta `MAX_BATCH` registros, del más antiguo al más reciente, sin borrarlos. Los
/// que no se pueden usar (demasiado grandes, JSON inválido, ruta relativa…) se borran.
pub fn take(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names = Vec::new();
    for e in rd.flatten().take(100_000) {
        let name = e.file_name().to_string_lossy().into_owned();
        if valid_name(&name) {
            names.push(name);
        } else if name.ends_with(".json.tmp") {
            // Restos de un proceso que murió a media escritura.
            let old = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() > 3600);
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    names.sort();
    let mut out = Vec::new();
    for name in names {
        if out.len() >= MAX_BATCH {
            break;
        }
        let path = dir.join(&name);
        match read_record(&path) {
            Some(record) => out.push(Entry { file: name, record }),
            None => {
                log::warn!("buzón de trabajos: se descarta {name} (no es un registro válido)");
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    out
}

fn read_record(path: &Path) -> Option<Record> {
    let bytes = read_bounded(path, MAX_RECORD_BYTES)?;
    serde_json::from_slice::<Record>(&bytes).ok()?.sanitize()
}

/// Lee un archivo si no pasa de `max` bytes.
pub fn read_bounded(path: &Path, max: u64) -> Option<Vec<u8>> {
    let f = std::fs::File::open(path).ok()?;
    if f.metadata().ok()?.len() > max {
        return None;
    }
    let mut bytes = Vec::new();
    f.take(max.saturating_add(1)).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > max {
        return None;
    }
    Some(bytes)
}

/// Borra los registros ya aplicados y guardados. Sólo nombres válidos dentro del buzón.
pub fn ack(dir: &Path, files: &[String]) -> usize {
    files.iter().take(MAX_BATCH * 10).filter(|f| valid_name(f)).filter(|f| std::fs::remove_file(dir.join(f.as_str())).is_ok()).count()
}

/// Datos de la reunión que la ventana guardó para ese audio en `jobs.json`, si los hay.
pub fn stored_meta(jobs_path: &Path, audio: &Path) -> Option<Meta> {
    let bytes = read_bounded(jobs_path, MAX_JOBS_JSON_BYTES)?;
    let jobs: Value = serde_json::from_slice(&bytes).ok()?;
    let wanted = audio.to_string_lossy();
    jobs.as_array()?
        .iter()
        .rev()
        .find(|j| j.get("path").and_then(Value::as_str) == Some(wanted.as_ref()))
        .and_then(|j| j.get("meta").cloned())
        .and_then(|m| serde_json::from_value::<Meta>(m).ok())
        .map(|m| Meta { title: bounded(&m.title), date: bounded(&m.date), place: bounded(&m.place), participants: bounded(&m.participants), notes: bounded(&m.notes) })
        .filter(|m| !m.is_empty())
}

/// Los segmentos van tal cual en `result`; esto sólo se usa en las pruebas.
#[cfg(test)]
fn sample_result(n: usize) -> TranscriptResult {
    TranscriptResult {
        segments: (0..n).map(|i| Segment { start_ms: i as i64 * 1000, end_ms: i as i64 * 1000 + 900, text: format!("segmento {i}"), speaker: None }).collect(),
        text: "hola".into(),
        audio_secs: 12.0,
        base_name: "junta".into(),
        output_dir: "/tmp".into(),
        ..TranscriptResult::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("iuret-inbox-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn abs(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn append_take_ack_round_trip_in_order() {
        let dir = temp_dir("rt");
        let mut a = Record::new(&abs("junta.wav"), Some("transcribing"));
        a.meta = Some(Meta { title: "Junta".into(), ..Meta::default() });
        append(&dir, &a).unwrap();
        let mut b = Record::new(&abs("junta.wav"), Some("done"));
        b.result = Some(sample_result(3));
        append(&dir, &b).unwrap();
        let got = take(&dir);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].record.status.as_deref(), Some("transcribing"));
        assert_eq!(got[1].record.result.as_ref().map(|r| r.segments.len()), Some(3));
        // Lo que llega después de leer no se borra al confirmar.
        append(&dir, &Record::new(&abs("otra.wav"), Some("recording"))).unwrap();
        let files: Vec<String> = got.iter().map(|e| e.file.clone()).collect();
        assert_eq!(ack(&dir, &files), 2);
        let rest = take(&dir);
        assert_eq!(rest.len(), 1);
        assert!(rest[0].record.path.ends_with("otra.wav"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_files_are_dropped_not_applied() {
        let dir = temp_dir("hostile");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1-1-1.json"), b"{not json").unwrap();
        std::fs::write(dir.join("1-1-2.json"), br#"{"path":"relativo.wav","status":"done"}"#).unwrap();
        std::fs::write(dir.join("1-1-3.json"), format!(r#"{{"path":{:?},"status":"hacked"}}"#, abs("x.wav"))).unwrap();
        std::fs::write(dir.join("1-1-4.json"), vec![b' '; (MAX_RECORD_BYTES + 10) as usize]).unwrap();
        std::fs::write(dir.join("1-1-5.json"), format!(r#"{{"path":{:?},"status":"done","durationSecs":-3}}"#, abs("ok.wav"))).unwrap();
        std::fs::write(dir.join("../escape.json"), b"{}").unwrap();
        let got = take(&dir);
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].record.duration_secs, None);
        // Los inválidos se borraron; el válido sigue hasta confirmarlo.
        let left: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(left, ["1-1-5.json"]);
        // `ack` no sale del buzón ni borra nombres raros.
        assert_eq!(ack(&dir, &["../escape.json".into(), "/etc/passwd".into(), "a.txt".into()]), 0);
        let _ = std::fs::remove_file(dir.join("../escape.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_result_is_sent_without_segments() {
        let dir = temp_dir("big");
        let mut r = Record::new(&abs("larga.wav"), Some("done"));
        let mut big = sample_result(1);
        big.text = "x".repeat((MAX_RECORD_BYTES + 1) as usize);
        r.result = Some(big);
        append(&dir, &r).unwrap();
        let got = take(&dir);
        assert_eq!(got.len(), 1);
        assert!(got[0].record.result.is_none());
        assert_eq!(got[0].record.status.as_deref(), Some("done"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn meta_from_args_accepts_string_or_list_and_is_bounded() {
        let m = Meta::from_args(&serde_json::json!({"participants": ["Ana", " ", "Luis"], "title": "x".repeat(MAX_META_CHARS + 50)}));
        assert_eq!(m.participants, "Ana\nLuis");
        assert_eq!(m.title.chars().count(), MAX_META_CHARS);
        let m = Meta::from_args(&serde_json::json!({"participants": "Ana, Luis; Eva", "place": "Sala 2", "date": 5}));
        assert_eq!(m.participants, "Ana\nLuis\nEva");
        assert_eq!(m.date, "");
        let ctx = m.context();
        assert!(ctx.contains("Sala 2") && ctx.contains("Ana, Luis, Eva"), "{ctx}");
        assert!(Meta::default().is_empty());
    }

    #[test]
    fn stored_meta_reads_the_window_jobs() {
        let dir = temp_dir("meta");
        std::fs::create_dir_all(&dir).unwrap();
        let jobs = dir.join("jobs.json");
        let audio = abs("junta.wav");
        std::fs::write(&jobs, serde_json::to_string(&serde_json::json!([{"path": audio, "meta": {"place": "Sala", "participants": "Ana"}}])).unwrap()).unwrap();
        assert_eq!(stored_meta(&jobs, &audio).map(|m| m.place), Some("Sala".into()));
        assert!(stored_meta(&jobs, &abs("otra.wav")).is_none());
        std::fs::write(&jobs, b"[{]").unwrap();
        assert!(stored_meta(&jobs, &audio).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
