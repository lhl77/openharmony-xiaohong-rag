use std::sync::{Arc, Barrier};
use unicode_segmentation::UnicodeSegmentation;
use xiaohong_rag::*;

// Exercise private pure helpers without expanding the production public API.
mod types {
    pub use xiaohong_rag::*;
}
#[path = "../src/chunk.rs"]
mod chunk;
#[path = "../src/tokenize.rs"]
mod tokenize;

fn config() -> IndexConfig {
    IndexConfig {
        chunk_target_chars: 8,
        chunk_overlap_chars: 2,
        minimum_chunk_chars: 3,
        embedding_dimension: 0,
    }
}

fn document(id: &str, revision: u64, text: &str) -> DocumentInput {
    DocumentInput {
        document_id: id.into(),
        revision,
        title: "课堂笔记".into(),
        course: "数学".into(),
        text: text.into(),
    }
}

fn request(query: &str) -> SearchRequest {
    serde_json::from_value(serde_json::json!({"query": query})).unwrap()
}

fn engine() -> RagEngine {
    RagEngine::new(config(), Arc::new(NoopEmbedder)).unwrap()
}

#[test]
fn short_text_exact_boundary_and_tail_are_preserved() {
    for text in ["短", "abcdefgh", "abcdefgh尾", "甲。乙丙丁戊己庚辛壬癸", "a\nb\nc"] {
        let doc = document("a", 1, text);
        let chunks = chunk::split_document(&doc, &config()).unwrap();
        assert!(!chunks.is_empty());
        assert_eq!(chunks.last().unwrap().source_end, text.len());
        for (offset, c) in text.char_indices() {
            if !c.is_whitespace() {
                assert!(chunks.iter().any(|part| part.source_start <= offset && offset < part.source_end));
            }
        }
        for part in chunks {
            assert_eq!(part.text, text[part.source_start..part.source_end]);
        }
    }
}

#[test]
fn overlap_counts_graphemes_not_bytes() {
    let text = "甲乙👨‍👩‍👧‍👦e\u{301}丙丁🇨🇳戊己庚辛壬";
    let doc = document("a", 1, text);
    let chunks = chunk::split_document(&doc, &config()).unwrap();
    let boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i)
        .chain(std::iter::once(text.len())).collect();
    for part in &chunks {
        assert!(boundaries.contains(&part.source_start));
        assert!(boundaries.contains(&part.source_end));
    }
    for pair in chunks.windows(2) {
        assert_eq!(text[pair[1].source_start..pair[0].source_end].graphemes(true).count(), 2);
    }
}

#[test]
fn punctuation_and_whitespace_combining_clusters_stay_intact() {
    let text = " \u{301}甲。\u{301}乙丙丁戊己庚辛壬";
    let boundaries: Vec<_> = text.grapheme_indices(true).map(|(i, _)| i)
        .chain(std::iter::once(text.len())).collect();
    let chunks = chunk::split_document(&document("a", 1, text), &config()).unwrap();
    assert_eq!(chunks[0].source_start, 0);
    for part in chunks {
        assert!(boundaries.contains(&part.source_start));
        assert!(boundaries.contains(&part.source_end));
    }
}

#[test]
fn chinese_tokens_do_not_cross_separators() {
    let tokens = tokenize::frequencies("特征，值。甲A乙 丙\n丁！戊");
    assert_eq!(tokens.get("特"), Some(&1));
    assert_eq!(tokens.get("征"), Some(&1));
    assert_eq!(tokens.get("特征"), Some(&1));
    for forbidden in ["征值", "值甲", "甲乙", "乙丙", "丙丁", "丁戊", "，", "。"] {
        assert!(!tokens.contains_key(forbidden), "{forbidden}");
    }
    assert_eq!(tokens.get("a"), Some(&1));
    assert!(tokenize::frequencies("E\u{301}").contains_key("e\u{301}"));
}

#[test]
fn cosine_is_finite_for_extreme_valid_vectors() {
    for value in [f32::MAX, f32::MIN_POSITIVE, f32::from_bits(1), 1.0] {
        let a = [value, value];
        assert!((tokenize::cosine(&a, &a).unwrap() - 1.0).abs() < 1e-6);
        assert!((tokenize::cosine(&a, &[-value, -value]).unwrap() + 1.0).abs() < 1e-6);
    }
    for a in [vec![], vec![0.0], vec![f32::NAN], vec![f32::INFINITY], vec![f32::NEG_INFINITY]] {
        assert!(tokenize::cosine(&a, &a).is_none());
    }
    assert!(tokenize::cosine(&[1.0], &[1.0, 2.0]).is_none());
}

#[test]
fn configuration_and_search_validation() {
    for bad in [
        IndexConfig { chunk_target_chars: 0, ..config() },
        IndexConfig { minimum_chunk_chars: 0, ..config() },
        IndexConfig { minimum_chunk_chars: 9, ..config() },
        IndexConfig { chunk_overlap_chars: 8, ..config() },
    ] {
        assert!(RagEngine::new(bad.clone(), Arc::new(NoopEmbedder)).is_err());
        assert!(chunk::split_document(&document("a", 1, "正文"), &bad).is_err());
    }
    assert!(RagEngine::new(IndexConfig { embedding_dimension: 2, ..config() }, Arc::new(NoopEmbedder)).is_err());
    let engine = engine();
    assert_eq!(request("甲").vector_weight, 1.0);
    assert_eq!(request("甲").keyword_weight, 1.0);
    for k in [1, MAX_RRF_K] {
        let mut req = request("甲");
        req.rrf_k = k;
        assert!(engine.search(req).is_ok());
    }
    let mut req = request("甲");
    for k in [0, MAX_RRF_K + 1] {
        req.rrf_k = k;
        assert!(engine.search(req.clone()).is_err());
    }
    for (vector, keyword) in [(0.0, 0.0), (-1.0, 1.0), (1.0, -1.0), (f32::NAN, 1.0), (1.0, f32::INFINITY), (f32::MAX, f32::MAX)] {
        let mut req = request("甲");
        req.vector_weight = vector;
        req.keyword_weight = keyword;
        assert!(engine.search(req).is_err());
    }
    for k in [0, 51] {
        let mut req = request("甲");
        req.top_k = k;
        assert!(engine.search(req).is_err());
    }
    assert!(engine.search(request(" \n")).is_err());
}

#[test]
fn duplicate_and_non_monotonic_batches_are_atomic() {
    let engine = engine();
    engine.add_documents(vec![document("a", 2, "旧正文")]).unwrap();
    for revisions in [(3, 3), (3, 4)] {
        assert!(engine.add_documents(vec![document("a", revisions.0, "新"), document("a", revisions.1, "新")]).is_err());
    }
    for revision in [1, 2] {
        assert!(engine.add_documents(vec![document("b", 1, "新"), document("a", revision, "新")]).is_err());
        assert_eq!(engine.stats().unwrap().documents, 1);
        assert_eq!(engine.search(request("旧")).unwrap().hits[0].citation.revision, 2);
    }
    assert!(engine.add_documents(vec![document("b", 1, "新"), document("", 1, "无效")]).is_err());
    assert_eq!(engine.stats().unwrap().documents, 1);
    assert!(engine.add_documents(vec![]).is_err());
}

struct OutputEmbedder {
    bad: Vec<f32>,
}

impl Embedder for OutputEmbedder {
    fn dimension(&self) -> usize {
        2
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        if text.contains("坏") {
            Ok(self.bad.clone())
        } else if text.contains("失败") {
            Err("provider failure".into())
        } else {
            Ok(vec![f32::MAX, f32::MAX])
        }
    }
}

#[test]
fn provider_invalid_outputs_fail_without_partial_commit() {
    for bad in [vec![f32::NAN, 1.0], vec![f32::INFINITY, 1.0], vec![f32::NEG_INFINITY, 1.0], vec![0.0, 0.0], vec![], vec![1.0]] {
        let engine = RagEngine::new(config(), Arc::new(OutputEmbedder { bad })).unwrap();
        engine.add_documents(vec![document("a", 1, "好")]).unwrap();
        assert!(engine.add_documents(vec![document("a", 2, "新"), document("b", 1, "坏")]).is_err());
        assert_eq!(engine.stats().unwrap().documents, 1);
        assert_eq!(engine.search(request("好")).unwrap().hits[0].citation.revision, 1);
        assert!(engine.search(request("坏")).is_err());
        assert!(engine.search(request("失败")).is_err());
        assert!(engine.add_documents(vec![document("b", 1, "失败")]).is_err());
    }
}

struct BlockingEmbedder {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl Embedder for BlockingEmbedder {
    fn dimension(&self) -> usize {
        1
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        if text == "慢" {
            self.entered.wait();
            self.release.wait();
        }
        Ok(vec![1.0])
    }
}

#[test]
fn concurrent_stale_batch_is_rejected_at_commit() {
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let engine = Arc::new(RagEngine::new(config(), Arc::new(BlockingEmbedder {
        entered: entered.clone(), release: release.clone(),
    })).unwrap());
    let writer = engine.clone();
    let task = std::thread::spawn(move || writer.add_documents(vec![
        document("b", 1, "好"), document("a", 2, "慢"),
    ]));
    entered.wait();
    engine.add_documents(vec![document("a", 3, "快")]).unwrap();
    release.wait();
    assert!(task.join().unwrap().is_err());
    assert_eq!(engine.stats().unwrap().documents, 1);
    assert_eq!(engine.search(request("快")).unwrap().hits[0].citation.revision, 3);
}

#[test]
fn references_are_checked_against_current_revision_and_scope() {
    let engine = engine();
    engine.add_documents(vec![document("a", 1, "正文")]).unwrap();
    let citation = engine.search(request("正文")).unwrap().hits.remove(0).citation;
    let reference = SourceReference {
        document_id: citation.document_id.clone(), revision: 1, chunk_id: citation.chunk_id.clone(),
    };
    assert_eq!(engine.resolve(&reference).unwrap(), Some(citation));
    assert!(engine.resolve_scoped(&reference, &[]).unwrap().is_none());
    assert!(engine.resolve_scoped(&reference, &["a".into()]).unwrap().is_some());
    let mut wrong = reference.clone();
    wrong.chunk_id = "missing".into();
    assert!(engine.resolve(&wrong).unwrap().is_none());
    wrong = reference.clone();
    wrong.document_id = "b".into();
    assert!(engine.resolve(&wrong).unwrap().is_none());
    engine.add_documents(vec![document("a", 2, "新正文")]).unwrap();
    assert!(engine.resolve(&reference).unwrap().is_none());
    engine.remove_document("a").unwrap();
    assert!(engine.resolve(&reference).unwrap().is_none());
    assert!(engine.add_documents(vec![document("a", 1, "正文")]).is_err());
    engine.add_documents(vec![document("a", 3, "正文")]).unwrap();
    assert!(engine.resolve(&reference).unwrap().is_none());
}

#[test]
fn scoped_search_filters_before_ranking_and_intersects_course() {
    let engine = engine();
    engine.add_documents(vec![document("a", 1, "甲甲甲"), document("b", 1, "甲")]).unwrap();
    let mut req = request("甲");
    req.top_k = 1;
    let result = engine.search_scoped(req.clone(), &["b".into()]).unwrap();
    assert_eq!(result.hits[0].citation.document_id, "b");
    assert_eq!(result.hits[0].keyword_rank, Some(1));
    assert!(engine.search_scoped(req.clone(), &[]).unwrap().hits.is_empty());
    assert!(engine.search_scoped(req.clone(), &["missing".into()]).unwrap().hits.is_empty());
    assert!(engine.search_scoped(req.clone(), &[" ".into()]).is_err());
    req.course_filter = Some("另一门课".into());
    assert!(engine.search_scoped(req, &["b".into()]).unwrap().hits.is_empty());
}

#[test]
fn style_is_structural_scoped_deduplicated_and_revision_aware() {
    let engine = engine();
    engine.add_documents(vec![
        document("a", 1, "# 私密标题\n\n甲乙\n\n丙丁戊己\n\n1. 私密条目\n- 私密列表\n```\n###### 不是标题\n+ 不是列表\n```"),
        document("b", 1, "### 不允许标题\n+ 不允许列表\n秘密正文"),
    ]).unwrap();
    assert!(serde_json::from_str::<StyleRequest>("{}").is_err());
    let style = engine.style_profile(StyleRequest { allowed_document_ids: vec!["a".into(), "a".into()] }).unwrap();
    assert_eq!(style.heading_levels, vec![1]);
    assert!(style.ordered_list);
    assert_eq!(style.bullets, vec!["-"]);
    assert_eq!(style.average_paragraph_length, 3.0);
    let json = serde_json::to_value(&style).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 4);
    assert!(!json.to_string().contains("私密"));
    assert!(!json.to_string().contains("秘密"));
    assert_eq!(engine.style_profile(StyleRequest { allowed_document_ids: vec![] }).unwrap(), StyleProfile::default());
    engine.add_documents(vec![document("a", 2, "## 新标题\n\n短")]).unwrap();
    let style = engine.style_profile(StyleRequest { allowed_document_ids: vec!["a".into()] }).unwrap();
    assert_eq!(style.heading_levels, vec![2]);
    assert!(!style.ordered_list);
    assert!(style.bullets.is_empty());
    assert_eq!(style.average_paragraph_length, 1.0);
    engine.remove_document("a").unwrap();
    assert_eq!(engine.style_profile(StyleRequest { allowed_document_ids: vec!["a".into()] }).unwrap(), StyleProfile::default());
}