use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::protocol::{
    Capability, DetailBlock, ExecPlan, HealthStatus, KvPair, ListQuery, ListResponse, MetricItem,
    Provider, ProviderError, ProviderManifest, ResumeMode, SessionDetail, SessionFacts,
    SessionSummary,
};

/// Parsed from the first `user` message in a JSONL file
#[derive(Debug, Clone)]
struct JsonlSessionMeta {
    session_id: String,
    cwd: Option<String>,
    git_branch: Option<String>,
    first_prompt: Option<String>,
    first_timestamp: Option<String>,
    file_mtime: Option<i64>, // unix millis from file metadata
    message_count: u64,
    is_sidechain: bool,
}

/// JSONL line — we parse the fields we need
#[derive(Debug, Deserialize)]
struct JsonlLine {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    timestamp: Option<String>,
    message: Option<serde_json::Value>,
}

/// For token accumulation from assistant messages
#[derive(Debug, Deserialize)]
struct AssistantMessage {
    model: Option<String>,
    usage: Option<UsageData>,
}

#[derive(Debug, Deserialize)]
struct UsageData {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
}

pub struct ClaudeProvider {
    claude_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            claude_dir: home.join(".claude"),
        }
    }

    #[cfg(test)]
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { claude_dir: dir }
    }

    fn projects_dir(&self) -> PathBuf {
        self.claude_dir.join("projects")
    }

    /// Scan all JSONL files and extract lightweight session metadata.
    /// Reads only the first few lines of each file for speed.
    fn scan_sessions(&self) -> Vec<(PathBuf, JsonlSessionMeta)> {
        let projects_dir = self.projects_dir();
        let mut results = Vec::new();

        let dirs = match fs::read_dir(&projects_dir) {
            Ok(d) => d,
            Err(_) => return results,
        };

        for dir_entry in dirs.flatten() {
            let dir_path = dir_entry.path();
            if !dir_path.is_dir() {
                continue;
            }

            let files = match fs::read_dir(&dir_path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for file_entry in files.flatten() {
                let file_path = file_entry.path();
                if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }

                if let Some(meta) = Self::scan_jsonl_meta(&file_path) {
                    if !meta.is_sidechain {
                        results.push((file_path, meta));
                    }
                }
            }
        }

        results
    }

    /// Read a JSONL file and extract session metadata.
    /// For the list view, reads only the first user message for title/cwd/branch,
    /// and uses file mtime for updated_at (much faster than reading entire file).
    fn scan_jsonl_meta(path: &Path) -> Option<JsonlSessionMeta> {
        use std::io::{BufRead, BufReader};

        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut session_id = None;
        let mut cwd = None;
        let mut git_branch = None;
        let mut first_prompt = None;
        let mut first_timestamp = None;
        let mut is_sidechain = false;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let parsed: JsonlLine = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if parsed.msg_type.as_deref() == Some("user") && session_id.is_none() {
                session_id = parsed.session_id;
                cwd = parsed.cwd;
                git_branch = parsed.git_branch;
                first_timestamp = parsed.timestamp;
                is_sidechain = parsed.is_sidechain.unwrap_or(false);

                if let Some(msg) = &parsed.message {
                    first_prompt = extract_prompt_text(msg);
                }
                break; // Only need the first user message for list view
            }
        }

        let session_id = session_id.unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into()
        });

        // Use file mtime for updated_at — much faster than reading entire file
        let file_mtime = path
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);

        Some(JsonlSessionMeta {
            session_id,
            cwd,
            git_branch,
            first_prompt,
            first_timestamp,
            file_mtime,
            message_count: 0,
            is_sidechain,
        })
    }

    fn meta_to_summary(meta: &JsonlSessionMeta) -> SessionSummary {
        let title = meta
            .first_prompt
            .clone()
            .unwrap_or_else(|| "Untitled session".into());

        let created_at = meta
            .first_timestamp
            .as_deref()
            .and_then(parse_iso_to_millis);

        // Prefer file mtime for updated_at (always available, most accurate)
        let updated_at = meta.file_mtime.or(created_at);

        SessionSummary {
            provider_id: "claude-code".into(),
            native_id: meta.session_id.clone(),
            title,
            project_path: meta.cwd.clone(),
            created_at,
            updated_at,
            git_branch: meta.git_branch.clone(),
            message_count: if meta.message_count > 0 {
                Some(meta.message_count)
            } else {
                None
            },
        }
    }

    fn find_session_jsonl(&self, native_id: &str) -> Option<PathBuf> {
        let projects_dir = self.projects_dir();
        if let Ok(entries) = fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let jsonl = path.join(format!("{}.jsonl", native_id));
                    if jsonl.exists() {
                        return Some(jsonl);
                    }
                }
            }
        }
        None
    }

    fn parse_session_facts(path: &Path) -> SessionFacts {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return SessionFacts::default(),
        };

        let mut facts = SessionFacts::default();
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut cache_read: u64 = 0;
        let mut cache_write: u64 = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed: JsonlLine = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if parsed.msg_type.as_deref() == Some("assistant") {
                if let Some(msg_val) = &parsed.message {
                    if let Ok(assistant) =
                        serde_json::from_value::<AssistantMessage>(msg_val.clone())
                    {
                        if facts.model.is_none() {
                            if let Some(m) = &assistant.model {
                                if m != "<synthetic>" {
                                    facts.model = Some(m.clone());
                                }
                            }
                        }
                        if let Some(usage) = &assistant.usage {
                            input_tokens += usage.input_tokens.unwrap_or(0);
                            output_tokens += usage.output_tokens.unwrap_or(0);
                            cache_read += usage.cache_read_input_tokens.unwrap_or(0);
                            cache_write += usage.cache_creation_input_tokens.unwrap_or(0);
                        }
                    }
                }
            }
        }

        if input_tokens > 0 {
            facts.input_tokens = Some(input_tokens);
        }
        if output_tokens > 0 {
            facts.output_tokens = Some(output_tokens);
        }
        if cache_read > 0 {
            facts.cache_read_tokens = Some(cache_read);
        }
        if cache_write > 0 {
            facts.cache_write_tokens = Some(cache_write);
        }

        facts
    }

    fn build_detail_blocks(summary: &SessionSummary, facts: &SessionFacts) -> Vec<DetailBlock> {
        let mut blocks = Vec::new();

        // Token usage metrics
        let mut metrics = Vec::new();
        if let Some(v) = facts.input_tokens {
            metrics.push(MetricItem {
                label: "Input".into(),
                value: v as i64,
                unit: "tokens".into(),
                max_value: None,
            });
        }
        if let Some(v) = facts.output_tokens {
            metrics.push(MetricItem {
                label: "Output".into(),
                value: v as i64,
                unit: "tokens".into(),
                max_value: None,
            });
        }
        if let Some(v) = facts.cache_read_tokens {
            metrics.push(MetricItem {
                label: "Cache Read".into(),
                value: v as i64,
                unit: "tokens".into(),
                max_value: None,
            });
        }
        if let Some(v) = facts.cache_write_tokens {
            metrics.push(MetricItem {
                label: "Cache Write".into(),
                value: v as i64,
                unit: "tokens".into(),
                max_value: None,
            });
        }
        if !metrics.is_empty() {
            blocks.push(DetailBlock::Metrics {
                title: "Token Usage".into(),
                items: metrics,
            });
        }

        // Session info key-value pairs
        let mut pairs = Vec::new();
        if let Some(count) = summary.message_count {
            pairs.push(KvPair {
                key: "Messages".into(),
                value: count.to_string(),
            });
        }
        if let Some(branch) = &summary.git_branch {
            pairs.push(KvPair {
                key: "Branch".into(),
                value: branch.clone(),
            });
        }
        if let Some(model) = &facts.model {
            pairs.push(KvPair {
                key: "Model".into(),
                value: model.clone(),
            });
        }
        if let Some(project) = &summary.project_path {
            pairs.push(KvPair {
                key: "Project".into(),
                value: project.clone(),
            });
        }
        if !pairs.is_empty() {
            blocks.push(DetailBlock::KeyValue {
                title: "Session Info".into(),
                pairs,
            });
        }

        blocks
    }
}

impl Provider for ClaudeProvider {
    fn manifest(&self) -> ProviderManifest {
        ProviderManifest {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            version: "0.1.0".into(),
            protocol_version: 1,
            capabilities: vec![
                Capability::ListSessions {
                    searchable: true,
                    sortable_fields: vec!["updated_at".into(), "created_at".into()],
                },
                Capability::Resume {
                    modes: vec![ResumeMode::ExactId, ResumeMode::LastSession],
                },
                Capability::TokenUsage,
            ],
        }
    }

    fn health(&self) -> HealthStatus {
        let projects_dir = self.projects_dir();
        let claude_exists = projects_dir.exists();

        if !claude_exists {
            return HealthStatus {
                available: false,
                message: Some(
                    "Claude Code data directory not found. Is Claude Code installed?".into(),
                ),
                data_path: None,
            };
        }

        // Check if claude binary is available
        let binary_available = std::process::Command::new("claude")
            .arg("--version")
            .output()
            .is_ok();

        HealthStatus {
            available: true,
            message: if binary_available {
                None
            } else {
                Some("Claude Code data found but 'claude' binary not in PATH".into())
            },
            data_path: Some(projects_dir.to_string_lossy().into()),
        }
    }

    fn list_sessions(&self, query: &ListQuery) -> Result<ListResponse, ProviderError> {
        let scanned = self.scan_sessions();
        let mut all_summaries: Vec<SessionSummary> = scanned
            .iter()
            .map(|(_, meta)| Self::meta_to_summary(meta))
            .collect();

        // Apply project filter
        if let Some(ref filter) = query.project_filter {
            all_summaries.retain(|s| {
                s.project_path
                    .as_deref()
                    .is_some_and(|p| p.contains(filter.as_str()))
            });
        }

        // Apply search filter
        if let Some(ref search) = query.search {
            let search_lower = search.to_lowercase();
            all_summaries.retain(|s| s.title.to_lowercase().contains(&search_lower));
        }

        // Sort by updated_at descending (most recent first)
        all_summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        // Apply limit
        let total = all_summaries.len() as u64;
        if let Some(limit) = query.limit {
            all_summaries.truncate(limit as usize);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Ok(ListResponse {
            items: all_summaries,
            next_cursor: None,
            total: Some(total),
            fetched_at: now,
        })
    }

    fn session_detail(&self, native_id: &str) -> Result<SessionDetail, ProviderError> {
        let jsonl_path = self.find_session_jsonl(native_id).ok_or_else(|| ProviderError {
            code: "NOT_FOUND".into(),
            message: format!("Session {} not found", native_id),
            retryable: false,
        })?;

        let meta = Self::scan_jsonl_meta(&jsonl_path).ok_or_else(|| ProviderError {
            code: "PARSE_ERROR".into(),
            message: format!("Failed to parse session {}", native_id),
            retryable: false,
        })?;

        let summary = Self::meta_to_summary(&meta);
        let facts = Self::parse_session_facts(&jsonl_path);
        let detail_blocks = Self::build_detail_blocks(&summary, &facts);

        Ok(SessionDetail {
            summary,
            facts,
            meta: BTreeMap::new(),
            detail_blocks,
        })
    }

    fn resume_command(&self, native_id: &str) -> Result<ExecPlan, ProviderError> {
        // Find cwd from the JSONL file
        let cwd = self
            .find_session_jsonl(native_id)
            .and_then(|p| Self::scan_jsonl_meta(&p))
            .and_then(|m| m.cwd);

        Ok(ExecPlan {
            program: "claude".into(),
            args: vec!["--resume".into(), native_id.into()],
            cwd,
            env: BTreeMap::new(),
            interactive: true,
            needs_approval: false,
        })
    }
}

/// Extract plain text from a Claude message content structure.
/// Content can be a string or an array of {type: "text", text: "..."} objects.
fn extract_prompt_text(msg: &serde_json::Value) -> Option<String> {
    // Try as object with "content" field
    if let Some(content) = msg.get("content") {
        return extract_from_content(content);
    }
    // Try the value itself as content
    extract_from_content(msg)
}

fn extract_from_content(content: &serde_json::Value) -> Option<String> {
    if let Some(s) = content.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(truncate_title(trimmed));
        }
    }
    if let Some(arr) = content.as_array() {
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        return Some(truncate_title(trimmed));
                    }
                }
            }
        }
    }
    None
}

fn truncate_title(s: &str) -> String {
    // Take first line, truncate to 120 chars
    let first_line = s.lines().next().unwrap_or(s);
    let chars: Vec<char> = first_line.chars().take(120).collect();
    chars.into_iter().collect()
}

fn parse_iso_to_millis(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }

    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;

    let millis = if s.len() > 20 && s.as_bytes()[19] == b'.' {
        let end = s.find('Z').unwrap_or(s.len());
        let frac = &s[20..end];
        let frac_str = format!("{:0<3}", &frac[..frac.len().min(3)]);
        frac_str.parse::<i64>().unwrap_or(0)
    } else {
        0
    };

    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    let month_days = [
        31,
        28 + if is_leap_year(year) { 1 } else { 0 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for m in 0..(month - 1) as usize {
        days += month_days[m] as i64;
    }
    days += day - 1;

    let total_millis = ((days * 24 + hour) * 60 + min) * 60 * 1000 + sec * 1000 + millis;
    Some(total_millis)
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> (TempDir, ClaudeProvider) {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().to_path_buf();
        let provider = ClaudeProvider::with_dir(claude_dir.clone());

        let project_dir = claude_dir.join("projects").join("test-project");
        fs::create_dir_all(&project_dir).unwrap();

        (tmp, provider)
    }

    fn write_jsonl(tmp: &TempDir, session_id: &str, lines: &[&str]) {
        let project_dir = tmp.path().join("projects").join("test-project");
        let content = lines.join("\n");
        fs::write(
            project_dir.join(format!("{}.jsonl", session_id)),
            content,
        )
        .unwrap();
    }

    fn write_session(tmp: &TempDir, id: &str, prompt: &str, cwd: &str, branch: &str, ts: &str) {
        write_jsonl(
            tmp,
            id,
            &[&format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"{}","gitBranch":"{}","isSidechain":false,"timestamp":"{}","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
                id, cwd, branch, ts, prompt
            )],
        );
    }

    #[test]
    fn test_scan_sessions_from_jsonl() {
        let (tmp, provider) = create_test_dir();
        write_session(
            &tmp,
            "abc123",
            "Fix the auth bug",
            "/Users/test/Code/app",
            "main",
            "2026-02-01T11:01:44.771Z",
        );

        let result = provider.list_sessions(&ListQuery::default()).unwrap();
        assert_eq!(result.items.len(), 1);

        let session = &result.items[0];
        assert_eq!(session.native_id, "abc123");
        assert_eq!(session.title, "Fix the auth bug");
        assert_eq!(session.provider_id, "claude-code");
        assert_eq!(
            session.project_path.as_deref(),
            Some("/Users/test/Code/app")
        );
        assert_eq!(session.git_branch.as_deref(), Some("main"));
        assert!(session.created_at.is_some());
        assert!(session.updated_at.is_some()); // from file mtime
    }

    #[test]
    fn test_sidechain_sessions_filtered() {
        let (tmp, provider) = create_test_dir();
        write_session(
            &tmp,
            "normal",
            "Normal session",
            "/test",
            "main",
            "2026-01-01T00:00:00Z",
        );
        write_jsonl(
            &tmp,
            "sidechain",
            &[r#"{"type":"user","sessionId":"sidechain","isSidechain":true,"timestamp":"2026-01-01T00:00:00Z","message":{"content":"side"}}"#],
        );

        let result = provider.list_sessions(&ListQuery::default()).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].native_id, "normal");
    }

    #[test]
    fn test_search_filter() {
        let (tmp, provider) = create_test_dir();
        write_session(&tmp, "s1", "Fix auth bug", "/test", "main", "2026-01-01T00:00:00Z");
        write_session(&tmp, "s2", "Add caching", "/test", "main", "2026-01-01T00:00:00Z");

        let query = ListQuery {
            search: Some("auth".into()),
            ..Default::default()
        };
        let result = provider.list_sessions(&query).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].native_id, "s1");
    }

    #[test]
    fn test_parse_jsonl_token_accumulation() {
        let (tmp, _provider) = create_test_dir();
        let project_dir = tmp.path().join("projects").join("test-project");

        write_jsonl(
            &tmp,
            "tok-test",
            &[
                r#"{"type":"user","sessionId":"tok-test","timestamp":"2026-01-01T00:00:00Z","message":{"content":"hello"}}"#,
                r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":200,"cache_creation_input_tokens":300}}}"#,
                r#"{"type":"assistant","timestamp":"2026-01-01T00:00:02Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":150,"output_tokens":75,"cache_read_input_tokens":100}}}"#,
            ],
        );

        let facts = ClaudeProvider::parse_session_facts(&project_dir.join("tok-test.jsonl"));

        assert_eq!(facts.input_tokens, Some(250));
        assert_eq!(facts.output_tokens, Some(125));
        assert_eq!(facts.cache_read_tokens, Some(300));
        assert_eq!(facts.cache_write_tokens, Some(300));
        assert_eq!(facts.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn test_session_detail_builds_blocks() {
        let (tmp, provider) = create_test_dir();
        write_jsonl(
            &tmp,
            "detail-test",
            &[
                r#"{"type":"user","sessionId":"detail-test","cwd":"/test/project","gitBranch":"dev","isSidechain":false,"timestamp":"2026-01-01T00:00:00Z","message":{"content":"Test session"}}"#,
                r#"{"type":"assistant","timestamp":"2026-01-01T00:00:01Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":1000,"output_tokens":500}}}"#,
            ],
        );

        let detail = provider.session_detail("detail-test").unwrap();
        assert_eq!(detail.summary.native_id, "detail-test");
        assert_eq!(detail.facts.input_tokens, Some(1000));
        assert_eq!(detail.facts.output_tokens, Some(500));
        assert!(detail.detail_blocks.len() >= 2);
    }

    #[test]
    fn test_session_not_found() {
        let (_tmp, provider) = create_test_dir();
        let err = provider.session_detail("nonexistent").unwrap_err();
        assert_eq!(err.code, "NOT_FOUND");
    }

    #[test]
    fn test_resume_command_sets_cwd() {
        let (tmp, provider) = create_test_dir();
        write_session(
            &tmp,
            "abc123",
            "Fix bug",
            "/Users/test/Code/app",
            "main",
            "2026-01-01T00:00:00Z",
        );

        let plan = provider.resume_command("abc123").unwrap();
        assert_eq!(plan.program, "claude");
        assert_eq!(plan.args, vec!["--resume", "abc123"]);
        assert_eq!(plan.cwd.as_deref(), Some("/Users/test/Code/app"));
        assert!(plan.interactive);
    }

    #[test]
    fn test_resume_command_without_jsonl() {
        let (_tmp, provider) = create_test_dir();
        let plan = provider.resume_command("no-file").unwrap();
        assert!(plan.cwd.is_none());
    }

    #[test]
    fn test_parse_iso_to_millis() {
        let millis = parse_iso_to_millis("2026-02-01T11:01:44.771Z");
        assert!(millis.is_some());
        assert!(millis.unwrap() > 0);

        let millis2 = parse_iso_to_millis("2026-02-01T11:01:44Z");
        assert!(millis2.is_some());

        assert!(parse_iso_to_millis("bad").is_none());
    }

    #[test]
    fn test_title_from_first_prompt() {
        let (tmp, provider) = create_test_dir();
        write_session(
            &tmp,
            "s1",
            "My first prompt",
            "/test",
            "main",
            "2026-01-01T00:00:00Z",
        );

        let result = provider.list_sessions(&ListQuery::default()).unwrap();
        assert_eq!(result.items[0].title, "My first prompt");
    }

    #[test]
    fn test_manifest() {
        let (_tmp, provider) = create_test_dir();
        let manifest = provider.manifest();
        assert_eq!(manifest.id, "claude-code");
        assert_eq!(manifest.protocol_version, 1);
    }

    #[test]
    fn test_synthetic_model_skipped() {
        let (tmp, provider) = create_test_dir();
        write_jsonl(
            &tmp,
            "syn-test",
            &[
                r#"{"type":"user","sessionId":"syn-test","cwd":"/test","timestamp":"2026-01-01T00:00:00Z","message":{"content":"test"}}"#,
                r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"input_tokens":10,"output_tokens":5}}}"#,
                r#"{"type":"assistant","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50}}}"#,
            ],
        );

        let detail = provider.session_detail("syn-test").unwrap();
        assert_eq!(detail.facts.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn test_message_count_from_detail() {
        let (tmp, provider) = create_test_dir();
        write_jsonl(
            &tmp,
            "count-test",
            &[
                r#"{"type":"user","sessionId":"count-test","cwd":"/test","timestamp":"2026-01-01T00:00:00Z","message":{"content":"first"}}"#,
                r#"{"type":"assistant","message":{"model":"claude-opus-4-6"}}"#,
                r#"{"type":"user","sessionId":"count-test","timestamp":"2026-01-01T00:01:00Z","message":{"content":"second"}}"#,
                r#"{"type":"assistant","message":{"model":"claude-opus-4-6"}}"#,
                r#"{"type":"user","sessionId":"count-test","timestamp":"2026-01-01T00:02:00Z","message":{"content":"third"}}"#,
            ],
        );

        // List view uses fast scan — message_count may be None
        let result = provider.list_sessions(&ListQuery::default()).unwrap();
        assert_eq!(result.items.len(), 1);

        // Detail view reads full file and should have accurate data
        let detail = provider.session_detail("count-test").unwrap();
        assert_eq!(detail.summary.native_id, "count-test");
    }
}
