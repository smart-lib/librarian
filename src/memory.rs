use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    config::Config,
    db::Database,
    domain::{ContextPack, MemoryHit, MemoryItem, MemoryKind},
};

const DEFAULT_CANDIDATE_LIMIT: i64 = 120;
const DEFAULT_HIT_LIMIT: usize = 12;

#[derive(Clone, Debug)]
pub struct RetrievalRequest {
    pub query: String,
    pub project_id: Option<Uuid>,
    pub activity_id: Option<Uuid>,
    pub limit: usize,
}

pub async fn retrieve_context_with_config(
    db: &Database,
    config: Option<&Config>,
    request: RetrievalRequest,
) -> Result<ContextPack> {
    let now = Utc::now();
    let embedding_model = config
        .map(|config| config.memory.embedding_model.as_str())
        .unwrap_or(LOCAL_HASH_MODEL);
    let embedding_dimensions = config
        .map(|config| config.memory.embedding_dimensions as usize)
        .unwrap_or(LOCAL_HASH_DIMENSIONS);
    let query_embedding = embed_text(&request.query, embedding_dimensions);
    let candidates = db
        .memory_candidates(
            request.project_id,
            request.activity_id,
            DEFAULT_CANDIDATE_LIMIT,
        )
        .await?;
    let query_terms = terms(&request.query);
    let mut hits = Vec::new();

    for item in candidates {
        if item
            .valid_until
            .is_some_and(|valid_until| valid_until < now)
        {
            continue;
        }
        let stored_embedding = db.get_memory_embedding(item.id, embedding_model).await?;
        let semantic_score = stored_embedding
            .as_ref()
            .and_then(|embedding| decode_embedding(&embedding.vector))
            .map(|embedding| cosine_similarity(&query_embedding, &embedding))
            .unwrap_or_else(|| lexical_score(&item, &query_terms));
        let hit = score_item(
            &item,
            &query_terms,
            semantic_score,
            request.project_id,
            request.activity_id,
            now,
        );
        if hit.score > 0.05 || matches!(item.kind, MemoryKind::Instruction | MemoryKind::Decision) {
            hits.push(hit);
        }
    }

    suppress_contradicted(&mut hits);
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(request.limit.max(1));

    Ok(ContextPack {
        query: request.query,
        project_id: request.project_id,
        activity_id: request.activity_id,
        generated_at: now,
        hits,
    })
}

pub async fn backfill_embeddings(db: &Database, config: &Config, limit: i64) -> Result<usize> {
    let model = config.memory.embedding_model.as_str();
    let items = db
        .memory_items_missing_embedding(model, limit.max(1))
        .await?;
    let count = items.len();
    for item in items {
        embed_item(db, config, &item).await?;
    }
    Ok(count)
}

pub async fn embed_item(db: &Database, config: &Config, item: &MemoryItem) -> Result<()> {
    let model = config.memory.embedding_model.as_str();
    let dimensions = config.memory.embedding_dimensions.max(8) as usize;
    let embedding = embed_memory_item(item, dimensions);
    db.upsert_memory_embedding(
        item.id,
        model,
        dimensions as i64,
        encode_embedding(&embedding),
    )
    .await?;
    Ok(())
}

pub fn render_context_pack(pack: &ContextPack) -> String {
    if pack.hits.is_empty() {
        return "No relevant memory found.".to_string();
    }

    let mut out = String::new();
    for (index, hit) in pack.hits.iter().enumerate() {
        let item = &hit.item;
        out.push_str(&format!(
            "{}. [{:?}] score={:.3} observed={} id={}\n{}\n\n",
            index + 1,
            item.kind,
            hit.score,
            item.observed_at.to_rfc3339(),
            item.id,
            item.content
        ));
    }
    out.trim_end().to_string()
}

fn score_item(
    item: &MemoryItem,
    query_terms: &HashSet<String>,
    semantic_score: f64,
    project_id: Option<Uuid>,
    activity_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> MemoryHit {
    let lexical_score = lexical_score(item, query_terms);
    let recency_score = recency_score(item, now);
    let scope_score = scope_score(item, project_id, activity_id);
    let kind_score = kind_score(&item.kind);
    let validity_score = validity_score(item, now);
    let score = (semantic_score * 0.42 + lexical_score * 0.18 + kind_score * 0.12 + 0.28)
        * recency_score
        * scope_score
        * validity_score
        * item.confidence.clamp(0.0, 1.5)
        * item.salience.clamp(0.1, 2.0);

    MemoryHit {
        item: item.clone(),
        score,
        semantic_score,
        lexical_score,
        recency_score,
        scope_score,
        reason: format!(
            "semantic={semantic_score:.3}; lexical={lexical_score:.3}; recency={recency_score:.3}; scope={scope_score:.3}"
        ),
    }
}

const LOCAL_HASH_MODEL: &str = "local-hash-v1";
const LOCAL_HASH_DIMENSIONS: usize = 384;

fn embed_memory_item(item: &MemoryItem, dimensions: usize) -> Vec<f32> {
    let mut text = String::new();
    if let Some(topic) = &item.topic {
        text.push_str(topic);
        text.push('\n');
    }
    text.push_str(&item.content);
    embed_text(&text, dimensions)
}

fn embed_text(text: &str, dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(8);
    let mut vector = vec![0.0_f32; dimensions];
    for term in terms(text) {
        let hash = fnv1a64(term.as_bytes());
        let index = (hash as usize) % dimensions;
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 + ((hash >> 8) % 7) as f32 / 16.0;
        vector[index] += sign * weight;
    }
    normalize(&mut vector);
    vector
}

fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes(chunk.try_into().ok()?));
    }
    Some(out)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for index in 0..len {
        let left = left[index] as f64;
        let right = right[index] as f64;
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    (dot / left_norm.sqrt() / right_norm.sqrt()).clamp(0.0, 1.0)
}

fn normalize(vector: &mut [f32]) {
    let norm = vector
        .iter()
        .map(|value| (*value as f64) * (*value as f64))
        .sum::<f64>()
        .sqrt() as f32;
    if norm > 0.0 {
        for value in vector {
            *value /= norm;
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn lexical_score(item: &MemoryItem, query_terms: &HashSet<String>) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }

    let mut haystack = item.content.clone();
    if let Some(topic) = &item.topic {
        haystack.push(' ');
        haystack.push_str(topic);
    }
    let item_terms = terms(&haystack);
    if item_terms.is_empty() {
        return 0.0;
    }

    let overlap = query_terms.intersection(&item_terms).count() as f64;
    let coverage = overlap / query_terms.len().max(1) as f64;
    let density = overlap / item_terms.len().max(1) as f64;
    (coverage * 0.75 + density * 0.25).clamp(0.0, 1.0)
}

fn recency_score(item: &MemoryItem, now: DateTime<Utc>) -> f64 {
    let age_days = now
        .signed_duration_since(item.observed_at)
        .num_hours()
        .max(0) as f64
        / 24.0;
    let half_life = match item.kind {
        MemoryKind::Status | MemoryKind::UserMessage | MemoryKind::AssistantMessage => 30.0,
        MemoryKind::Instruction | MemoryKind::Decision => 365.0,
        MemoryKind::Fact | MemoryKind::Summary | MemoryKind::RunObservation => 120.0,
    };
    let decayed = 0.5_f64.powf(age_days / half_life);
    decayed.clamp(0.12, 1.0)
}

fn scope_score(item: &MemoryItem, project_id: Option<Uuid>, activity_id: Option<Uuid>) -> f64 {
    let mut score = 1.0;
    if let Some(activity_id) = activity_id {
        score *= if item.activity_id == Some(activity_id) {
            1.45
        } else if item.activity_id.is_none() {
            1.0
        } else {
            0.55
        };
    }
    if let Some(project_id) = project_id {
        score *= if item.project_id == Some(project_id) {
            1.35
        } else if item.project_id.is_none() {
            0.92
        } else {
            0.35
        };
    }
    score
}

fn kind_score(kind: &MemoryKind) -> f64 {
    match kind {
        MemoryKind::Instruction => 1.0,
        MemoryKind::Decision => 0.92,
        MemoryKind::Status => 0.82,
        MemoryKind::Summary => 0.72,
        MemoryKind::Fact => 0.65,
        MemoryKind::RunObservation => 0.58,
        MemoryKind::UserMessage | MemoryKind::AssistantMessage => 0.45,
    }
}

fn validity_score(item: &MemoryItem, now: DateTime<Utc>) -> f64 {
    if item.valid_from.is_some_and(|valid_from| valid_from > now) {
        return 0.0;
    }
    if item
        .valid_until
        .is_some_and(|valid_until| valid_until < now)
    {
        return 0.0;
    }
    1.0
}

fn suppress_contradicted(hits: &mut Vec<MemoryHit>) {
    let mut by_id = HashMap::new();
    for (index, hit) in hits.iter().enumerate() {
        by_id.insert(hit.item.id, index);
    }

    let mut suppressed = HashSet::new();
    for hit in hits.iter() {
        if let Some(older_id) = hit.item.contradicts_id {
            if let Some(older_index) = by_id.get(&older_id) {
                let older = &hits[*older_index].item;
                if hit.item.observed_at >= older.observed_at {
                    suppressed.insert(older_id);
                }
            }
        }
    }

    hits.retain(|hit| !suppressed.contains(&hit.item.id));
}

fn terms(input: &str) -> HashSet<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter_map(|raw| {
            let term = raw.trim().to_lowercase();
            if term.len() < 2 || stopword(&term) {
                None
            } else {
                Some(term)
            }
        })
        .collect()
}

fn stopword(term: &str) -> bool {
    matches!(
        term,
        "the" | "and" | "for" | "что" | "как" | "это" | "там" | "или" | "для" | "над" | "with"
    )
}

pub fn default_hit_limit() -> usize {
    DEFAULT_HIT_LIMIT
}
