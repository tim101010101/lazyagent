mod provider;

pub use provider::Provider;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ===== Provider Manifest =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol_version: u32,
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Capability {
    ListSessions {
        searchable: bool,
        sortable_fields: Vec<String>,
    },
    Resume {
        modes: Vec<ResumeMode>,
    },
    TokenUsage,
    CostTracking,
    NewSession,
    DeleteSession,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResumeMode {
    ExactId,
    LastSession,
    #[serde(other)]
    Unknown,
}

// ===== Session Summary (list path — lightweight) =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub provider_id: String,
    pub native_id: String,
    pub title: String,
    pub project_path: Option<String>,
    pub created_at: Option<i64>,  // unix millis
    pub updated_at: Option<i64>,
    pub git_branch: Option<String>,
    pub message_count: Option<u64>,
}

// ===== Session Detail (detail path — full) =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub facts: SessionFacts,
    pub meta: BTreeMap<String, serde_json::Value>,
    pub detail_blocks: Vec<DetailBlock>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionFacts {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub cost_microdollars: Option<i64>,
    pub model: Option<String>,
    pub context_window: Option<u64>,
}

// ===== Declarative Detail Blocks =====

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DetailBlock {
    KeyValue { title: String, pairs: Vec<KvPair> },
    Metrics { title: String, items: Vec<MetricItem> },
    Text { title: String, content: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvPair {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricItem {
    pub label: String,
    pub value: i64,
    pub unit: String,
    pub max_value: Option<i64>,
}

// ===== Exec Plan =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecPlan {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub interactive: bool,
    pub needs_approval: bool,
}

// ===== Query =====

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListQuery {
    pub project_filter: Option<String>,
    pub search: Option<String>,
    pub sort_by: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub items: Vec<SessionSummary>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
    pub fetched_at: i64,
}

// ===== Health =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub available: bool,
    pub message: Option<String>,
    pub data_path: Option<String>,
}

// ===== Error =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ProviderError {}
