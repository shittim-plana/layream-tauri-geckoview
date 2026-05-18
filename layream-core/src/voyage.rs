use serde::{Deserialize, Serialize};

use crate::error::LayreamError;

const EMBEDDINGS_URL: &str = "https://api.voyageai.com/v1/embeddings";
const RERANK_URL: &str = "https://api.voyageai.com/v1/rerank";
const MAX_BATCH_SIZE: usize = 128;

#[derive(Debug, Clone, Serialize)]
struct EmbedRequest<'a> {
    input: &'a [String],
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_type: Option<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbedData {
    embedding: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct RerankRequest<'a> {
    query: &'a str,
    documents: &'a [String],
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<usize>,
    return_documents: bool,
    truncation: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RerankResult {
    pub index: usize,
    pub relevance_score: f64,
    pub document: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RerankResponse {
    data: Vec<RerankResult>,
}

pub async fn embed(
    client: &reqwest::Client,
    api_key: &str,
    texts: &[String],
    model: &str,
    input_type: Option<&str>,
) -> Result<Vec<Vec<f64>>, LayreamError> {
    let mut all_embeddings = Vec::with_capacity(texts.len());

    for chunk in texts.chunks(MAX_BATCH_SIZE) {
        let req = EmbedRequest {
            input: chunk,
            model,
            input_type,
        };

        let resp = client
            .post(EMBEDDINGS_URL)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&req)
            .send()
            .await
            .map_err(|e| LayreamError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
            return Err(LayreamError::ApiError { status, body });
        }

        let embed_resp: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| LayreamError::Http(e.to_string()))?;

        all_embeddings.extend(embed_resp.data.into_iter().map(|d| d.embedding));
    }

    Ok(all_embeddings)
}

pub async fn rerank(
    client: &reqwest::Client,
    api_key: &str,
    query: &str,
    documents: &[String],
    model: &str,
    top_k: Option<usize>,
) -> Result<Vec<RerankResult>, LayreamError> {
    let req = RerankRequest {
        query,
        documents,
        model,
        top_k,
        return_documents: true,
        truncation: true,
    };

    let resp = client
        .post(RERANK_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
        return Err(LayreamError::ApiError { status, body });
    }

    let rerank_resp: RerankResponse = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    Ok(rerank_resp.data)
}

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub fn top_k_by_similarity(
    query_embedding: &[f64],
    doc_embeddings: &[Vec<f64>],
    k: usize,
) -> Vec<(usize, f64)> {
    let mut scores: Vec<(usize, f64)> = doc_embeddings
        .iter()
        .enumerate()
        .map(|(i, emb)| (i, cosine_similarity(query_embedding, emb)))
        .collect();
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(k);
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn cosine_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-10);
    }

    #[test]
    fn cosine_zero_vector() {
        let a = vec![1.0, 2.0];
        let b = vec![0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn top_k_ranking() {
        let query = vec![1.0, 0.0];
        let docs = vec![
            vec![0.0, 1.0],
            vec![1.0, 0.0],
            vec![0.5, 0.5],
        ];
        let top = top_k_by_similarity(&query, &docs, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 1);
        assert!((top[0].1 - 1.0).abs() < 1e-10);
    }
}
