use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type RagResult<T> = Result<T, RagError>;

#[derive(Debug, Error)]
pub enum RagError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("document not found: {0}")]
    DocumentNotFound(String),
    #[error("embedding failed: {0}")]
    Embedding(String),
    #[error("vector dimension mismatch: expected {expected}, got {actual}")]
    VectorDimension { expected: usize, actual: usize },
    #[error("index lock poisoned")]
    PoisonedLock,
    #[error("null pointer at C ABI boundary")]
    NullPointer,
}

impl RagError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Json(_) => "INVALID_JSON",
            Self::InvalidRequest(_) => "INVALID_REQUEST",
            Self::DocumentNotFound(_) => "DOCUMENT_NOT_FOUND",
            Self::Embedding(_) => "EMBEDDING_FAILED",
            Self::VectorDimension { .. } => "VECTOR_DIMENSION_MISMATCH",
            Self::PoisonedLock => "INDEX_LOCKED",
            Self::NullPointer => "NULL_INPUT",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInput {
    pub document_id: String,
    pub revision: u64,
    pub title: String,
    pub course: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub document_id: String,
    pub revision: u64,
    pub title: String,
    pub course: String,
    pub chunk_id: String,
    /// Half-open UTF-8 byte range in the source revision: [start, end).
    pub source_start: usize,
    pub source_end: usize,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub citation: Citation,
    pub score: f32,
    pub vector_rank: Option<usize>,
    pub keyword_rank: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub schema_version: u16,
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub course_filter: Option<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_rrf_k")]
    pub rrf_k: u32,
    #[serde(default = "default_weight")]
    pub vector_weight: f32,
    #[serde(default = "default_weight")]
    pub keyword_weight: f32,
}

fn default_top_k() -> usize {
    5
}

fn default_rrf_k() -> u32 {
    60
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDocumentsRequest {
    pub documents: Vec<DocumentInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RrfConfig {
    pub k: u32,
    pub vector_weight: f32,
    pub keyword_weight: f32,
}
impl Default for RrfConfig {
    fn default() -> Self { Self { k: 60, vector_weight: 1.0, keyword_weight: 1.0 } }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexConfig {
    pub chunk_target_chars: usize,
    pub chunk_overlap_chars: usize,
    pub embedding_dimension: usize,
    pub minimum_chunk_chars: usize,
}
impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            chunk_target_chars: 1024,
            chunk_overlap_chars: 96,
            embedding_dimension: 0,
            minimum_chunk_chars: 24,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStats {
    pub documents: usize,
    pub chunks: usize,
    pub embedding_dimension: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitRequest {
    #[serde(default)]
    pub config: IndexConfig,
}

impl IndexConfig {
    pub fn validate(&self) -> RagResult<()> {
        if self.chunk_target_chars == 0
            || self.minimum_chunk_chars == 0
            || self.minimum_chunk_chars > self.chunk_target_chars
            || self.chunk_overlap_chars >= self.chunk_target_chars
        {
            return Err(RagError::InvalidRequest("invalid chunk configuration".into()));
        }
        Ok(())
    }
}

pub const MAX_RRF_K: u32 = 10_000;

impl RrfConfig {
    pub fn validate(&self) -> RagResult<()> {
        if self.k == 0 || self.k > MAX_RRF_K {
            return Err(RagError::InvalidRequest("rrfK must be between 1 and 10000".into()));
        }
        if !self.vector_weight.is_finite()
            || !self.keyword_weight.is_finite()
            || self.vector_weight < 0.0
            || self.keyword_weight < 0.0
            || (self.vector_weight == 0.0 && self.keyword_weight == 0.0)
            || f64::from(self.vector_weight) + f64::from(self.keyword_weight) > f64::from(f32::MAX)
        {
            return Err(RagError::InvalidRequest(
                "weights must be finite, non-negative, with a positive f32-representable sum".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    pub document_id: String,
    pub revision: u64,
    pub chunk_id: String,
}

/// Explicit caller-provided sample scope, NOT identity authentication.
/// Missing IDs fail deserialization; an empty list selects no documents.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleRequest {
    pub allowed_document_ids: Vec<String>,
}

/// Structural statistics only; never includes source prose.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StyleProfile {
    pub heading_levels: Vec<usize>,
    pub ordered_list: bool,
    pub bullets: Vec<String>,
    /// Mean grapheme count of prose paragraphs; headings, lists and code excluded.
    pub average_paragraph_length: f64,
}
