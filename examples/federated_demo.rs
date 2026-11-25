use std::sync::Arc;
use xiaohong_rag::{
    DocumentInput, FederatedSearch, FederatedSearchRequest, IndexConfig, NoopEmbedder,
    ProviderHit, RagEngine, SearchProvider,
};

/// Bridges another in-device app. No real IPC is performed in this demo.
struct NotesAppProvider;
impl SearchProvider for NotesAppProvider {
    fn id(&self) -> &str { "notes-app" }

    fn search(&self, query: &str, _top_k: usize) -> Result<Vec<ProviderHit>, String> {
        if query.contains("几何重数") || query.contains("特征") {
            Ok(vec![ProviderHit {
                title: "错题本：几何重数".into(),
                reference: "xiaohong-notes://note/7".into(),
                snippet: "上次错题：几何重数等于特征子空间维数。".into(),
                score: 0.9,
            }])
        } else {
            Ok(Vec::new())
        }
    }
}

/// Simulates an internet search provider; no network request is made.
struct WebProvider;
impl SearchProvider for WebProvider {
    fn id(&self) -> &str { "web" }

    fn search(&self, query: &str, _top_k: usize) -> Result<Vec<ProviderHit>, String> {
        Ok(vec![ProviderHit {
            title: "OpenHarmony 检索能力（演示占位）".into(),
            reference: format!("https://example.com/search?q={query}"),
            snippet: "互联网结果仅作为授权后的增强，不包含用户私有数据。".into(),
            score: 0.4,
        }])
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Explicitly authored demonstration data, not user/private content.
    let engine = Arc::new(RagEngine::new(IndexConfig::default(), Arc::new(NoopEmbedder))?);
    engine.add_documents(vec![DocumentInput {
        document_id: "DEMO-LA-001".into(),
        revision: 1,
        title: "线代复习：几何重数".into(),
        course: "线性代数".into(),
        text: "几何重数等于特征子空间的维数。对特征值 λ，特征子空间为 Ker(λI−A)，其维数等于 n−rank(λI−A)。".into(),
    }])?;

    let fused = FederatedSearch::new(engine);
    fused.register(Arc::new(NotesAppProvider));
    fused.register(Arc::new(WebProvider));

    let response = fused.search(FederatedSearchRequest {
        query: "几何重数 特征子空间".into(),
        sources: Vec::new(),
        top_k: 6,
        rrf_k: 60,
    })?;
    println!("FEDERATED_RESPONSE={}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
