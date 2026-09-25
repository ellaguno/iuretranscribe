//! Vocabulario propio: el prompt inicial que orienta a Whisper hacia nombres y
//! términos del usuario, y las correcciones que se aplican al texto transcrito.

use crate::settings::Settings;
use crate::subtitles::Segment;
use serde::{Deserialize, Serialize};

/// Una corrección: cualquiera de las variantes `wrong` (separadas por comas) se
/// reemplaza por `right`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Correction {
    pub wrong: String,
    pub right: String,
}

/// Whisper sólo usa ~220 tokens del prompt; se recorta por si el usuario escribe de más.
const MAX_PROMPT_CHARS: usize = 600;

/// Prompt inicial para Whisper: el vocabulario del usuario más los términos
/// correctos de las correcciones que no aparezcan ya en él.
pub fn initial_prompt(settings: &Settings) -> Option<String> {
    let mut prompt = settings.vocabulary.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = prompt.to_lowercase();
    let extra: Vec<&str> = settings
        .corrections
        .iter()
        .map(|c| c.right.trim())
        .filter(|r| !r.is_empty() && !lower.contains(&r.to_lowercase()))
        .collect();
    if !extra.is_empty() {
        if !prompt.is_empty() && !prompt.ends_with(['.', ',', ';']) {
            prompt.push('.');
        }
        if !prompt.is_empty() {
            prompt.push(' ');
        }
        prompt.push_str(&extra.join(", "));
        prompt.push('.');
    }
    if prompt.is_empty() {
        return None;
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        prompt = prompt.chars().take(MAX_PROMPT_CHARS).collect();
    }
    Some(prompt)
}

/// Aplica las correcciones al texto sin distinguir mayúsculas y sólo sobre
/// palabras completas; un espacio de la variante acepta espacios o guiones.
#[derive(Debug, Clone, Default)]
pub struct Corrector {
    /// (variante en minúsculas como caracteres, reemplazo), la variante más larga primero.
    rules: Vec<(Vec<char>, String)>,
}

impl Corrector {
    pub fn new(corrections: &[Correction]) -> Self {
        let mut rules = Vec::new();
        for c in corrections {
            let right = c.right.trim();
            if right.is_empty() {
                continue;
            }
            for w in c.wrong.split(',') {
                let w = w.split_whitespace().collect::<Vec<_>>().join(" ");
                if w.is_empty() || w.eq_ignore_ascii_case(right) {
                    continue;
                }
                rules.push((w.chars().map(lower).collect::<Vec<_>>(), right.to_string()));
            }
        }
        rules.sort_by_key(|r| std::cmp::Reverse(r.0.len()));
        Self { rules }
    }

    pub fn from_settings(settings: &Settings) -> Self {
        Self::new(&settings.corrections)
    }

    pub fn apply(&self, text: &str) -> String {
        if self.rules.is_empty() {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let lowered: Vec<char> = chars.iter().copied().map(lower).collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        'outer: while i < chars.len() {
            let at_word_start = i == 0 || !is_word(chars[i - 1]);
            if at_word_start {
                for (pat, right) in &self.rules {
                    if let Some(end) = match_at(&lowered, i, pat) {
                        if end == chars.len() || !is_word(chars[end]) {
                            out.push_str(right);
                            i = end;
                            continue 'outer;
                        }
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    pub fn apply_segments(&self, segments: &mut [Segment]) {
        if self.rules.is_empty() {
            return;
        }
        for s in segments {
            s.text = self.apply(&s.text);
        }
    }
}

/// Posición final si `pat` coincide en `text[start..]`; un espacio del patrón
/// consume uno o más espacios/guiones.
fn match_at(text: &[char], start: usize, pat: &[char]) -> Option<usize> {
    let mut i = start;
    for &p in pat {
        if p == ' ' {
            let from = i;
            while i < text.len() && (text[i].is_whitespace() || text[i] == '-') {
                i += 1;
            }
            if i == from {
                return None;
            }
        } else if i < text.len() && text[i] == p {
            i += 1;
        } else {
            return None;
        }
    }
    Some(i)
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corr(wrong: &str, right: &str) -> Correction {
        Correction { wrong: wrong.into(), right: right.into() }
    }

    #[test]
    fn replaces_variants_as_whole_words() {
        let c = Corrector::new(&[corr("Yaguno", "Llaguno"), corr("Euro efficient, Ureficient", "Iurefficient")]);
        assert_eq!(c.apply("Soy Eduardo yaguno, de Euro-efficient."), "Soy Eduardo Llaguno, de Iurefficient.");
        assert_eq!(c.apply("UREFICIENT y ureficient"), "Iurefficient y Iurefficient");
        assert_eq!(c.apply("euro  efficient"), "Iurefficient");
        // No toca palabras que sólo contienen la variante.
        assert_eq!(c.apply("Yagunos"), "Yagunos");
        assert_eq!(c.apply("Neuro efficient"), "Neuro efficient");
    }

    #[test]
    fn longest_variant_wins_and_is_idempotent() {
        let c = Corrector::new(&[corr("Iure, Iure eficient", "Iurefficient")]);
        assert_eq!(c.apply("Iure eficient"), "Iurefficient");
        assert_eq!(c.apply(&c.apply("Iure eficient")), "Iurefficient");
    }

    #[test]
    fn prompt_merges_vocabulary_and_corrections() {
        let s = Settings {
            vocabulary: "Eduardo Llaguno".into(),
            corrections: vec![corr("Yaguno", "Llaguno"), corr("Ureficient", "Iurefficient")],
            ..Settings::default()
        };
        assert_eq!(initial_prompt(&s).as_deref(), Some("Eduardo Llaguno. Iurefficient."));
        assert_eq!(initial_prompt(&Settings::default()), None);
    }
}
