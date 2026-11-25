use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};
use unicode_segmentation::UnicodeSegmentation;

use crate::chunk::split_document;
use crate::tokenize::{cosine, frequencies};
use crate::types::*;

/// Provider abstraction keeps model/runtime choice outside the retrieval core.
pub trait Embedder: Send + Sync {
    fn dimension(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
}
#[derive(Default)]
pub struct NoopEmbedder;
impl Embedder for NoopEmbedder {
    fn dimension(&self) -> usize { 0 }
    fn embed(&self, _: &str) -> Result<Vec<f32>, String> { Ok(Vec::new()) }
}

/// Adapter contract for searchable sources outside the local index.
/// Host applications implement this trait to bridge other in-device apps,
/// system search APIs, or internet services into M2's fused search.
pub trait SearchProvider: Send + Sync {
    fn id(&self) -> &str;
    /// Return up to `top_k` candidates. Providers rank themselves internally;
    /// the federated engine only re-ranks across sources via reciprocal rank.
    fn search(&self, query: &str, top_k: usize) -> Result<Vec<ProviderHit>, String>;
}

/// Single fused-search entry point: the local index plus registered providers.
pub struct FederatedSearch {
    local: Arc<RagEngine>,
    providers: RwLock<Vec<Arc<dyn SearchProvider>>>,
}

impl FederatedSearch {
    pub fn new(local: Arc<RagEngine>) -> Self {
        Self { local, providers: RwLock::new(Vec::new()) }
    }

    /// Register or replace a provider with the same ID.
    pub fn register(&self, provider: Arc<dyn SearchProvider>) -> bool {
        let Ok(mut providers) = self.providers.write() else { return false };
        if let Some(slot) = providers.iter_mut().find(|p| p.id() == provider.id()) {
            *slot = provider;
        } else {
            providers.push(provider);
        }
        true
    }

    pub fn unregister(&self, id: &str) -> bool {
        let Ok(mut providers) = self.providers.write() else { return false };
        let before = providers.len();
        providers.retain(|p| p.id() != id);
        providers.len() != before
    }

    pub fn search(&self, request: FederatedSearchRequest) -> RagResult<FederatedSearchResponse> {
        if request.query.trim().is_empty() {
            return Err(RagError::InvalidRequest("query must not be empty".into()));
        }
        if !(1..=50).contains(&request.top_k) {
            return Err(RagError::InvalidRequest("topK must be between 1 and 50".into()));
        }
        if request.rrf_k == 0 || request.rrf_k > MAX_RRF_K {
            return Err(RagError::InvalidRequest("rrfK must be between 1 and 10000".into()));
        }
        let use_source = |id: &str| {
            request.sources.is_empty() || request.sources.iter().any(|source| source == id)
        };

        let mut warnings = Vec::new();
        let mut grouped: Vec<(String, Vec<ProviderHit>)> = Vec::new();

        if use_source("local") {
            let local_request = SearchRequest {
                query: request.query.clone(),
                course_filter: None,
                top_k: request.top_k,
                rrf_k: request.rrf_k,
                vector_weight: 1.0,
                keyword_weight: 1.0,
            };
            match self.local.search(local_request) {
                Ok(response) => {
                    warnings.extend(response.warnings.iter().cloned());
                    let hits = response.hits.into_iter().map(|hit| ProviderHit {
                        title: hit.citation.title,
                        reference: hit.citation.chunk_id,
                        snippet: hit.citation.text,
                        score: hit.score,
                    }).collect();
                    grouped.push(("local".to_string(), hits));
                }
                Err(error) => warnings.push(format!("local source failed: {error}")),
            }
        }

        let providers = self.providers.read().map_err(|_| RagError::PoisonedLock)?;
        for provider in providers.iter() {
            if !use_source(provider.id()) {
                continue;
            }
            match provider.search(&request.query, request.top_k) {
                Ok(hits) => grouped.push((provider.id().to_string(), hits)),
                Err(error) => warnings.push(format!("source {} failed: {}", provider.id(), error)),
            }
        }
        drop(providers);

        let mut fused = Vec::new();
        for (source_id, mut hits) in grouped {
            hits.sort_by(|a, b| b.score.total_cmp(&a.score)
                .then_with(|| a.reference.cmp(&b.reference)));
            for (rank, hit) in hits.into_iter().enumerate() {
                let source_rank = rank + 1;
                let fused_score = 1.0 / (request.rrf_k as f32 + source_rank as f32);
                fused.push(FederatedHit {
                    source_id: source_id.clone(),
                    title: hit.title,
                    reference: hit.reference,
                    snippet: hit.snippet,
                    provider_score: hit.score,
                    fused_score,
                    source_rank,
                });
            }
        }
        fused.sort_by(|a, b| b.fused_score.total_cmp(&a.fused_score)
            .then_with(|| a.source_id.cmp(&b.source_id))
            .then_with(|| a.reference.cmp(&b.reference)));
        fused.truncate(request.top_k);
        Ok(FederatedSearchResponse {
            schema_version: 1,
            query: request.query,
            hits: fused,
            warnings,
        })
    }
}

#[derive(Clone)]
struct IndexedChunk {
    citation: Citation,
    terms: BTreeMap<String, u32>,
    vector: Vec<f32>,
}
#[derive(Default)]
struct State {
    docs: HashMap<(String, u64), Vec<IndexedChunk>>,
    active: HashMap<String, u64>,
    styles: HashMap<String, StyleStats>,
    // Retain revision high-water marks after deletion to prevent reference revival.
    revisions: HashMap<String, u64>,
}

pub struct RagEngine {
    config: IndexConfig,
    embedder: Arc<dyn Embedder>,
    state: RwLock<State>,
}
impl RagEngine {
    pub fn new(config: IndexConfig, embedder: Arc<dyn Embedder>) -> RagResult<Self> {
        config.validate()?;
        let dimension = embedder.dimension();
        if config.embedding_dimension != 0 && dimension != config.embedding_dimension {
            return Err(RagError::VectorDimension {
                expected: config.embedding_dimension,
                actual: dimension,
            });
        }
        Ok(Self { config, embedder, state: RwLock::new(State::default()) })
    }

    pub fn add_documents(&self, documents: Vec<DocumentInput>) -> RagResult<IndexStats> {
        if documents.is_empty() {
            return Err(RagError::InvalidRequest("documents must not be empty".into()));
        }
        let mut ids = HashSet::new();
        for document in &documents {
            if !ids.insert(document.document_id.clone()) {
                return Err(RagError::InvalidRequest("duplicate documentId in batch".into()));
            }
        }

        // Prepare all documents and provider vectors before acquiring the write lock.
        let mut prepared = Vec::new();
        for document in documents {
            let chunks = split_document(&document, &self.config)?;
            let mut indexed = Vec::new();
            for citation in chunks {
                let terms = frequencies(&citation.text);
                let vector = self.embed_checked(&citation.text)?;
                indexed.push(IndexedChunk { citation, terms, vector });
            }
            let style = StyleStats::from_text(&document.text);
            prepared.push(((document.document_id, document.revision), indexed, style));
        }

        let mut state = self.state.write().map_err(|_| RagError::PoisonedLock)?;
        // Validate all revisions under the commit lock, including concurrent writes.
        for ((id, revision), _, _) in &prepared {
            if state.revisions.get(id).is_some_and(|previous| revision <= previous) {
                return Err(RagError::InvalidRequest("revision must strictly increase".into()));
            }
        }
        for ((id, revision), chunks, style) in prepared {
            state.active.insert(id.clone(), revision);
            state.revisions.insert(id.clone(), revision);
            state.styles.insert(id.clone(), style);
            state.docs.retain(|(document_id, _), _| document_id != &id);
            state.docs.insert((id, revision), chunks);
        }
        Ok(IndexStats {
            documents: state.active.len(),
            chunks: state.docs.values().map(Vec::len).sum(),
            embedding_dimension: self.embedder.dimension(),
        })
    }

    pub fn remove_document(&self, document_id: &str) -> RagResult<bool> {
        let mut state = self.state.write().map_err(|_| RagError::PoisonedLock)?;
        let existed = state.active.remove(document_id).is_some();
        state.docs.retain(|(id, _), _| id != document_id);
        state.styles.remove(document_id);
        Ok(existed)
    }

    pub fn search(&self, request: SearchRequest) -> RagResult<SearchResponse> {
        self.search_internal(request, None)
    }

    /// The caller supplies a trusted selection scope; this does not authenticate users.
    /// Empty scopes select nothing. Filtering occurs before ranks and topK.
    pub fn search_scoped(
        &self,
        request: SearchRequest,
        allowed_document_ids: &[String],
    ) -> RagResult<SearchResponse> {
        let allowed = validate_scope(allowed_document_ids)?;
        self.search_internal(request, Some(&allowed))
    }

    fn search_internal(
        &self,
        request: SearchRequest,
        allowed: Option<&HashSet<&str>>,
    ) -> RagResult<SearchResponse> {
        if request.query.trim().is_empty() {
            return Err(RagError::InvalidRequest("query must not be empty".into()));
        }
        if !(1..=50).contains(&request.top_k) {
            return Err(RagError::InvalidRequest("topK must be between 1 and 50".into()));
        }
        RrfConfig {
            k: request.rrf_k,
            vector_weight: request.vector_weight,
            keyword_weight: request.keyword_weight,
        }.validate()?;
        let query_terms = frequencies(&request.query);
        let query_vector = self.embed_checked(&request.query)?;
        let state = self.state.read().map_err(|_| RagError::PoisonedLock)?;
        let mut candidates = Vec::new();
        for ((id, revision), chunks) in &state.docs {
            if state.active.get(id) != Some(revision)
                || allowed.is_some_and(|scope| !scope.contains(id.as_str()))
            {
                continue;
            }
            for chunk in chunks {
                if request.course_filter.as_ref().is_some_and(|filter| filter != &chunk.citation.course) {
                    continue;
                }
                let keyword = keyword_score(&query_terms, &chunk.terms);
                let vector = cosine(&query_vector, &chunk.vector).unwrap_or(0.0).max(0.0);
                candidates.push((chunk, keyword, vector));
            }
        }
        let keyword_ranks = ranks(candidates.iter().map(|item| {
            (item.0.citation.chunk_id.clone(), item.1)
        }).collect());
        let vector_ranks = ranks(candidates.iter().map(|item| {
            (item.0.citation.chunk_id.clone(), item.2)
        }).collect());
        let mut hits = Vec::new();
        for (chunk, keyword, vector) in candidates {
            let keyword_rank = keyword_ranks.get(&chunk.citation.chunk_id).copied()
                .filter(|_| keyword > 0.0);
            let vector_rank = vector_ranks.get(&chunk.citation.chunk_id).copied()
                .filter(|_| vector > 0.0);
            let mut score = 0.0;
            if let Some(rank) = vector_rank {
                score += request.vector_weight / (request.rrf_k as f32 + rank as f32);
            }
            if let Some(rank) = keyword_rank {
                score += request.keyword_weight / (request.rrf_k as f32 + rank as f32);
            }
            if score.is_finite() && score > 0.0 {
                hits.push(SearchHit {
                    citation: chunk.citation.clone(), score, vector_rank, keyword_rank,
                });
            }
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score)
            .then_with(|| a.citation.chunk_id.cmp(&b.citation.chunk_id)));
        hits.truncate(request.top_k);
        let warnings = if query_vector.is_empty() {
            vec!["No embedding provider is configured; results use the keyword channel only.".into()]
        } else {
            Vec::new()
        };
        Ok(SearchResponse { schema_version: 1, query: request.query, hits, warnings })
    }

    pub fn stats(&self) -> RagResult<IndexStats> {
        let state = self.state.read().map_err(|_| RagError::PoisonedLock)?;
        Ok(IndexStats {
            documents: state.active.len(),
            chunks: state.docs.values().map(Vec::len).sum(),
            embedding_dimension: self.embedder.dimension(),
        })
    }

    fn embed_checked(&self, text: &str) -> RagResult<Vec<f32>> {
        let dimension = self.embedder.dimension();
        if dimension == 0 {
            return Ok(Vec::new());
        }
        let vector = self.embedder.embed(text).map_err(RagError::Embedding)?;
        if vector.len() != dimension {
            return Err(RagError::VectorDimension { expected: dimension, actual: vector.len() });
        }
        if cosine(&vector, &vector).is_none() {
            return Err(RagError::Embedding("vector must be finite and have non-zero norm".into()));
        }
        Ok(vector)
    }

    /// None means deleted, stale revision, or unknown chunk. Returned text always
    /// comes from the current index, never from a caller-supplied citation body.
    pub fn resolve(&self, reference: &SourceReference) -> RagResult<Option<Citation>> {
        let state = self.state.read().map_err(|_| RagError::PoisonedLock)?;
        Ok(resolve_in_state(&state, reference))
    }

    pub fn resolve_scoped(
        &self,
        reference: &SourceReference,
        allowed_document_ids: &[String],
    ) -> RagResult<Option<Citation>> {
        let allowed = validate_scope(allowed_document_ids)?;
        if !allowed.contains(reference.document_id.as_str()) {
            return Ok(None);
        }
        self.resolve(reference)
    }

    pub fn style_profile(&self, request: StyleRequest) -> RagResult<StyleProfile> {
        let allowed = validate_scope(&request.allowed_document_ids)?;
        let state = self.state.read().map_err(|_| RagError::PoisonedLock)?;
        let mut combined = StyleStats::default();
        for id in allowed {
            if let Some(style) = state.styles.get(id) {
                combined.heading_levels.extend(&style.heading_levels);
                combined.bullets.extend(style.bullets.iter().cloned());
                combined.ordered_list |= style.ordered_list;
                combined.paragraph_chars += style.paragraph_chars;
                combined.paragraphs += style.paragraphs;
            }
        }
        Ok(StyleProfile {
            heading_levels: combined.heading_levels.into_iter().collect(),
            ordered_list: combined.ordered_list,
            bullets: combined.bullets.into_iter().collect(),
            average_paragraph_length: if combined.paragraphs == 0 {
                0.0
            } else {
                combined.paragraph_chars as f64 / combined.paragraphs as f64
            },
        })
    }
}

fn validate_scope(ids: &[String]) -> RagResult<HashSet<&str>> {
    if ids.iter().any(|id| id.trim().is_empty()) {
        return Err(RagError::InvalidRequest("allowed document IDs must be non-empty".into()));
    }
    Ok(ids.iter().map(String::as_str).collect())
}

fn resolve_in_state(state: &State, reference: &SourceReference) -> Option<Citation> {
    if state.active.get(&reference.document_id) != Some(&reference.revision) {
        return None;
    }
    state.docs.get(&(reference.document_id.clone(), reference.revision))?
        .iter()
        .find(|chunk| chunk.citation.chunk_id == reference.chunk_id)
        .map(|chunk| chunk.citation.clone())
}

#[derive(Default)]
struct StyleStats {
    heading_levels: std::collections::BTreeSet<usize>,
    ordered_list: bool,
    bullets: std::collections::BTreeSet<String>,
    paragraph_chars: usize,
    paragraphs: usize,
}

impl StyleStats {
    fn from_text(text: &str) -> Self {
        let mut stats = Self::default();
        let mut paragraph = String::new();
        let mut fence: Option<(char, usize)> = None;
        for line in text.lines() {
            let line = line.trim();
            let marker = line.chars().next().unwrap_or(' ');
            let run = line.chars().take_while(|c| *c == marker).count();
            if let Some((open, length)) = fence {
                if marker == open && run >= length && line[run..].trim().is_empty() {
                    fence = None;
                }
                continue;
            }
            if matches!(marker, '`' | '~') && run >= 3 {
                stats.finish_paragraph(&mut paragraph);
                fence = Some((marker, run));
                continue;
            }
            let heading = line.bytes().take_while(|b| *b == b'#').count();
            let is_heading = (1..=6).contains(&heading)
                && line[heading..].starts_with(char::is_whitespace);
            let bullet = ["-", "*", "+", "•"].into_iter().find(|prefix| {
                line.strip_prefix(prefix).is_some_and(|rest| rest.starts_with(char::is_whitespace))
            });
            let digits = line.bytes().take_while(u8::is_ascii_digit).count();
            let ordered = digits > 0 && line[digits..].strip_prefix(['.', ')'])
                .is_some_and(|rest| rest.starts_with(char::is_whitespace));
            if line.is_empty() || is_heading || bullet.is_some() || ordered {
                stats.finish_paragraph(&mut paragraph);
                if is_heading {
                    stats.heading_levels.insert(heading);
                }
                if let Some(bullet) = bullet {
                    stats.bullets.insert(bullet.to_owned());
                }
                stats.ordered_list |= ordered;
            } else {
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(line);
            }
        }
        stats.finish_paragraph(&mut paragraph);
        stats
    }

    fn finish_paragraph(&mut self, paragraph: &mut String) {
        if !paragraph.is_empty() {
            self.paragraph_chars += paragraph.graphemes(true).count();
            self.paragraphs += 1;
            paragraph.clear();
        }
    }
}
fn keyword_score(query: &BTreeMap<String, u32>, document: &BTreeMap<String, u32>) -> f32 {
    let overlap: f32 = query.iter().map(|(term, count)| {
        *count as f32 * document.get(term).copied().unwrap_or(0) as f32
    }).sum();
    if query.is_empty() {
        0.0
    } else {
        overlap / query.values().sum::<u32>() as f32
    }
}

fn ranks(mut values: Vec<(String, f32)>) -> HashMap<String, usize> {
    values.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut seen = HashSet::new();
    let mut output = HashMap::new();
    for (id, score) in values {
        if score > 0.0 && seen.insert(id.clone()) {
            let rank = output.len() + 1;
            output.insert(id, rank);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedEmbedder {
        dim: usize,
    }

    impl Embedder for FixedEmbedder {
        fn dimension(&self) -> usize { self.dim }

        fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
            Ok(if text.contains("矩阵") || text.contains("特征") {
                vec![1.0, 0.0]
            } else {
                vec![0.0, 1.0]
            })
        }
    }

    fn cfg() -> IndexConfig {
        IndexConfig {
            chunk_target_chars: 24,
            chunk_overlap_chars: 4,
            embedding_dimension: 2,
            minimum_chunk_chars: 3,
        }
    }

    #[test]
    fn dual_channel_returns_citations_and_ranks() {
        let engine = RagEngine::new(cfg(), Arc::new(FixedEmbedder { dim: 2 })).unwrap();
        engine.add_documents(vec![DocumentInput {
            document_id: "N1".into(), revision: 3, title: "线代笔记".into(),
            course: "线代".into(), text: "特征值与矩阵的几何重数定义。几何重数不大于代数重数。".into(),
        }]).unwrap();
        let result = engine.search(SearchRequest {
            query: "特征矩阵".into(), course_filter: Some("线代".into()),
            top_k: 3, rrf_k: 60, vector_weight: 1.0, keyword_weight: 1.0,
        }).unwrap();
        assert!(!result.hits.is_empty());
        assert!(result.hits.iter().all(|hit| {
            hit.citation.document_id == "N1" && hit.citation.revision == 3 && hit.score > 0.0
        }));
        assert!(result.hits.iter().any(|hit| {
            hit.keyword_rank.is_some() && hit.vector_rank.is_some()
        }));
    }

    #[test]
    fn reindex_replaces_old_revision_and_delete_clears_it() {
        let engine = RagEngine::new(cfg(), Arc::new(FixedEmbedder { dim: 2 })).unwrap();
        for revision in 1..=2 {
            engine.add_documents(vec![DocumentInput {
                document_id: "N1".into(), revision, title: "n".into(), course: "c".into(),
                text: format!("版本{revision} 特征值定义和例题。"),
            }]).unwrap();
        }
        assert_eq!(engine.stats().unwrap().documents, 1);
        assert_eq!(engine.stats().unwrap().chunks, 1);
        assert!(engine.remove_document("N1").unwrap());
        assert_eq!(engine.stats().unwrap().chunks, 0);
    }

    #[test]
    fn empty_embedding_falls_back_to_keyword_with_warning() {
        let engine = RagEngine::new(
            IndexConfig { embedding_dimension: 0, ..cfg() }, Arc::new(NoopEmbedder),
        ).unwrap();
        engine.add_documents(vec![DocumentInput {
            document_id: "N1".into(), revision: 1, title: "n".into(), course: "c".into(),
            text: "矩阵 特征值 代数重数。".into(),
        }]).unwrap();
        let result = engine.search(SearchRequest {
            query: "特征值".into(), course_filter: None, top_k: 5, rrf_k: 60,
            vector_weight: 1.0, keyword_weight: 1.0,
        }).unwrap();
        assert!(!result.hits.is_empty());
        assert!(!result.warnings.is_empty());
    }
}
