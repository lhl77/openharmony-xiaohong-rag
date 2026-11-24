use std::sync::Arc;
use xiaohong_rag::{DocumentInput, IndexConfig, NoopEmbedder, RagEngine, SearchRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This is an explicitly authored demonstration note, not user/private data.
    let note = "几何重数等于特征子空间的维数。对特征值 λ，特征子空间为 Ker(λI−A)，其维数等于 n−rank(λI−A)。特征值的几何重数不大于代数重数。";
    let engine = RagEngine::new(IndexConfig::default(), Arc::new(NoopEmbedder))?;
    let stats = engine.add_documents(vec![DocumentInput {
        document_id: "DEMO-LA-001".into(),
        revision: 1,
        title: "线代复习：几何重数".into(),
        course: "线性代数".into(),
        text: note.into(),
    }])?;
    let response = engine.search(SearchRequest {
        query: "几何重数 特征子空间 维数".into(),
        course_filter: Some("线性代数".into()),
        top_k: 5,
        rrf_k: 60,
        vector_weight: 1.0,
        keyword_weight: 1.0,
    })?;
    println!("INDEX_STATS={}", serde_json::to_string_pretty(&stats)?);
    println!("QUERY={}", response.query);
    println!("SEARCH_RESPONSE={}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
