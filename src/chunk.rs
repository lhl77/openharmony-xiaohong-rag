use unicode_segmentation::UnicodeSegmentation;

use crate::types::{Citation, DocumentInput, IndexConfig, RagError, RagResult};

/// Split by paragraph/sentence where possible, then by Unicode grapheme. The
/// stored source offsets are UTF-8 byte offsets into the exact document text.
pub fn split_document(doc: &DocumentInput, config: &IndexConfig) -> RagResult<Vec<Citation>> {
    if doc.document_id.trim().is_empty() || doc.title.trim().is_empty() {
        return Err(RagError::InvalidRequest("documentId and title must be non-empty".into()));
    }
    config.validate()?;
    let mut output = Vec::new();
    let mut start = 0usize;
    let text = doc.text.as_str();
    while start < text.len() {
        let target = grapheme_limit(text, start, config.chunk_target_chars);
        let proposed = boundary_at_or_before(text, start, target);
        // A minimum is a boundary preference, never a reason to discard text.
        let end = if target == text.len()
            || text[start..proposed].graphemes(true).count() < config.minimum_chunk_chars
        {
            target
        } else {
            proposed
        };
        let part = &text[start..end];
        let first = part.grapheme_indices(true)
            .find(|(_, g)| !g.chars().all(char::is_whitespace));
        let last = part.grapheme_indices(true).rev()
            .find(|(_, g)| !g.chars().all(char::is_whitespace));
        let (trim_prefix, trim_suffix) = match (first, last) {
            (Some((first, _)), Some((last, g))) => (first, last + g.len()),
            _ => (0, 0),
        };
        let chunk = &part[trim_prefix..trim_suffix];
        if !chunk.is_empty() {
            let actual_start = start + trim_prefix;
            let actual_end = start + trim_suffix;
            let id = format!("{}:r{}:c{:04}", doc.document_id, doc.revision, output.len() + 1);
            output.push(Citation {
                document_id: doc.document_id.clone(), revision: doc.revision,
                title: doc.title.clone(), course: doc.course.clone(), chunk_id: id,
                source_start: actual_start, source_end: actual_end, text: chunk.to_owned(),
            });
        }
        if end == text.len() { break; }
        let chunk_graphemes = text[start..end].graphemes(true).count();
        let next = if config.chunk_overlap_chars > 0
            && chunk_graphemes > config.chunk_overlap_chars
        {
            text[start..end]
                .grapheme_indices(true)
                .rev()
                .nth(config.chunk_overlap_chars - 1)
                .map(|(i, _)| start + i)
                .unwrap_or(end)
        } else {
            end
        };
        let next = if next <= start { next_boundary(text, start) } else { next };
        debug_assert!(next > start && text.is_char_boundary(next));
        start = next;
    }
    Ok(output)
}

fn boundary_at_or_before(s: &str, from: usize, limit: usize) -> usize {
    let slice = &s[from..limit];
    if let Some((i, g)) = slice.grapheme_indices(true).rev().find(|(_, g)| g.contains('\n')) {
        return from + i + g.len();
    }
    if let Some((i, g)) = slice.grapheme_indices(true).rev().find(|(_, g)| {
        g.chars().next().is_some_and(|c| matches!(c, '。' | '！' | '？' | ';' | '；' | '.' | '!' | '?'))
    }) {
        return from + i + g.len();
    }
    limit
}
fn grapheme_limit(s: &str, start: usize, count: usize) -> usize {
    s[start..].grapheme_indices(true)
        .nth(count)
        .map(|(i, _)| start + i)
        .unwrap_or(s.len())
}

fn next_boundary(s: &str, at: usize) -> usize {
    s[at..].grapheme_indices(true)
        .nth(1)
        .map(|(i, _)| at + i)
        .unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_have_valid_unicode_source_ranges_and_reconstruct() {
        let doc = DocumentInput {
            document_id: "N1".into(), revision: 2, title: "笔记".into(), course: "线代".into(),
            text: "λ 是特征值。\n几何重数小于等于代数重数。".into(),
        };
        let config = IndexConfig {
            chunk_target_chars: 12, chunk_overlap_chars: 2,
            embedding_dimension: 0, minimum_chunk_chars: 2,
        };
        let chunks = split_document(&doc, &config).unwrap();
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(doc.text.is_char_boundary(chunk.source_start));
            assert!(doc.text.is_char_boundary(chunk.source_end));
            assert_eq!(&doc.text[chunk.source_start..chunk.source_end], chunk.text);
        }
    }

    #[test]
    fn short_sentence_chunks_progress_without_overlap_loop() {
        let doc = DocumentInput {
            document_id: "N1".into(), revision: 1, title: "笔记".into(), course: "线代".into(),
            text: "甲。乙。丙丁戊己庚辛壬癸".into(),
        };
        let config = IndexConfig {
            chunk_target_chars: 8, chunk_overlap_chars: 3,
            embedding_dimension: 0, minimum_chunk_chars: 1,
        };
        let chunks = split_document(&doc, &config).unwrap();
        assert_eq!(chunks.first().unwrap().text, "甲。乙。");
        assert_eq!(chunks.last().unwrap().source_end, doc.text.len());
    }
}
