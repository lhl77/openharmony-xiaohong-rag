use std::sync::Arc;
use xiaohong_rag::*;

struct StubProvider {
    id: &'static str,
    term: &'static str,
    score: f32,
}

impl SearchProvider for StubProvider {
    fn id(&self) -> &str { self.id }

    fn search(&self, query: &str, _top_k: usize) -> Result<Vec<ProviderHit>, String> {
        if query.contains(self.term) {
            Ok(vec![ProviderHit {
                title: format!("{} 命中", self.id),
                reference: format!("app://{}/1", self.id),
                snippet: "演示来源片段".into(),
                score: self.score,
            }])
        } else {
            Ok(Vec::new())
        }
    }
}

fn seeded_engine() -> FederatedSearch {
    let engine = Arc::new(RagEngine::new(IndexConfig::default(), Arc::new(NoopEmbedder)).unwrap());
    engine.add_documents(vec![DocumentInput {
        document_id: "N1".into(),
        revision: 1,
        title: "线代笔记".into(),
        course: "数学".into(),
        text: "几何重数等于特征子空间的维数。".into(),
    }]).unwrap();
    let fused = FederatedSearch::new(engine);
    fused.register(Arc::new(StubProvider { id: "notes-app", term: "几何重数", score: 0.9 }));
    fused.register(Arc::new(StubProvider { id: "web", term: "几何重数", score: 0.4 }));
    fused
}

#[test]
fn federated_search_fuses_local_and_providers() {
    let response = seeded_engine().search(FederatedSearchRequest {
        query: "几何重数".into(),
        sources: Vec::new(),
        top_k: 5,
        rrf_k: 60,
    }).unwrap();
    let mut sources: Vec<&str> = response.hits.iter().map(|hit| hit.source_id.as_str()).collect();
    sources.sort_unstable();
    assert_eq!(sources, vec!["local", "notes-app", "web"]);
    let top = response.hits.first().unwrap();
    assert_eq!(top.source_rank, 1);
}

#[test]
fn federated_sources_filter_selects_subset() {
    let response = seeded_engine().search(FederatedSearchRequest {
        query: "几何重数".into(),
        sources: vec!["web".into()],
        top_k: 5,
        rrf_k: 60,
    }).unwrap();
    assert!(!response.hits.is_empty());
    assert!(response.hits.iter().all(|hit| hit.source_id == "web"));
}

#[test]
fn failed_provider_yields_warning_but_keeps_others() {
    struct FailingProvider;
    impl SearchProvider for FailingProvider {
        fn id(&self) -> &str { "broken" }

        fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<ProviderHit>, String> {
            Err("not reachable".into())
        }
    }
    let engine = Arc::new(RagEngine::new(IndexConfig::default(), Arc::new(NoopEmbedder)).unwrap());
    let fused = FederatedSearch::new(engine);
    fused.register(Arc::new(FailingProvider));
    let response = fused.search(FederatedSearchRequest {
        query: "x".into(),
        sources: Vec::new(),
        top_k: 5,
        rrf_k: 60,
    }).unwrap();
    assert!(response.warnings.iter().any(|warning| warning.contains("broken")));
}

#[test]
fn federated_search_rejects_invalid_requests() {
    let fused = seeded_engine();
    assert!(fused.search(FederatedSearchRequest {
        query: "".into(),
        sources: Vec::new(),
        top_k: 5,
        rrf_k: 60,
    }).is_err());
    assert!(fused.search(FederatedSearchRequest {
        query: "x".into(),
        sources: Vec::new(),
        top_k: 51,
        rrf_k: 60,
    }).is_err());
    assert!(fused.search(FederatedSearchRequest {
        query: "x".into(),
        sources: Vec::new(),
        top_k: 5,
        rrf_k: 0,
    }).is_err());
}
