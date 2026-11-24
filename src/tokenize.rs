use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;

/// Deterministic, dependency-free keyword tokenizer for the on-device baseline.
/// Latin/digits are grouped; Han characters are indexed as unigrams and bigrams.
pub fn frequencies(text: &str) -> BTreeMap<String, u32> {
    let mut counts = BTreeMap::new();
    let mut latin = String::new();
    let flush = |latin: &mut String, counts: &mut BTreeMap<String, u32>| {
        if !latin.is_empty() {
            increment(counts, latin.to_lowercase());
            latin.clear();
        }
    };
    let mut previous_han: Option<&str> = None;
    for grapheme in text.graphemes(true) {
        let c = grapheme.chars().next().unwrap_or(' ');
        if is_han(c) {
            flush(&mut latin, &mut counts);
            increment(&mut counts, grapheme.to_owned());
            if let Some(previous) = previous_han {
                increment(&mut counts, format!("{previous}{grapheme}"));
            }
            previous_han = Some(grapheme);
            continue;
        }
        previous_han = None;
        if c.is_alphanumeric() || c == '_' {
            latin.push_str(grapheme);
            continue;
        }
        flush(&mut latin, &mut counts);
    }
    flush(&mut latin, &mut counts);
    counts
}

fn increment(counts: &mut BTreeMap<String, u32>, token: String) {
    let count = counts.entry(token).or_insert(0);
    *count = count.saturating_add(1);
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
        | 0x20000..=0x2FA1F | 0x30000..=0x323AF)
}

/// Widen before multiplication to avoid f32 overflow and subnormal underflow.
pub fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let (mut dot, mut aa, mut bb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (&x, &y) in a.iter().zip(b) {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (x, y) = (f64::from(x), f64::from(y));
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 {
        return None;
    }
    let value = dot / aa.sqrt() / bb.sqrt();
    value.is_finite().then(|| value.clamp(-1.0, 1.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_and_latin_tokens_exist() {
        let tokens = frequencies("特征值 λ2 特征向量");
        assert!(tokens.contains_key("特征"));
        assert!(tokens.contains_key("征值"));
        assert!(tokens.contains_key("λ2"));
    }

    #[test]
    fn cosine_is_normalized() {
        assert!((cosine(&[3.0, 4.0], &[3.0, 4.0]).unwrap() - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0], &[1.0, 2.0]).is_none());
    }
}
