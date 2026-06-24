use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{
    EdgeType, GraphEdge, GraphNode, GraphNodeType, GraphRecord, Message, RecordType, Role,
    SourceRef, ToolCall, ToolResult,
};

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;
pub mod pi;

/// Incremental-read hint supplied by live recording.
///
/// Adapters may ignore hints and rescan their roots, but path-aware adapters can
/// use [`ReadHint::Paths`] to limit work to files that changed since the last
/// poll.
#[derive(Debug, Clone)]
pub enum ReadHint {
    Full,
    Paths(Vec<PathBuf>),
}

/// Passive parser for one agent client's session artifacts.
///
/// Implementations should not run agents, mutate source artifacts, or depend on
/// private credentials. They translate files/databases/logs already produced by
/// an agent into normalized [`GraphRecord`] values and preserve source
/// provenance through `SourceRef`.
pub trait AgentAdapter {
    /// Stable adapter name used by CLI/config and `source.agent-adapter`.
    fn name(&self) -> &'static str;

    /// Read normalized records from `roots` and emit them in source order.
    fn read(
        &self,
        roots: &[PathBuf],
        hint: ReadHint,
        emit: &mut dyn FnMut(GraphRecord) -> Result<()>,
    ) -> Result<()>;
}

/// Registry of named source adapters.
///
/// The registry is intentionally simple so downstream users can build a custom
/// binary with extra adapters while reusing the CLI and record backends.
pub struct AdapterRegistry {
    factories: HashMap<String, Box<dyn Fn() -> Box<dyn AgentAdapter>>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Return all built-in adapters shipped by this crate.
    pub fn builtins() -> Self {
        Self::new()
            .with("pi", || Box::new(pi::PiAdapter))
            .with("codex", || Box::new(codex::CodexAdapter))
            .with("claude", || Box::new(claude::ClaudeAdapter))
            .with("claude-code", || Box::new(claude::ClaudeAdapter))
            .with("gemini", || Box::new(gemini::GeminiAdapter))
            .with("gemini-cli", || Box::new(gemini::GeminiAdapter))
            .with("opencode", || Box::new(opencode::OpenCodeAdapter))
    }

    /// Add or replace a named adapter factory.
    pub fn with(
        mut self,
        name: impl Into<String>,
        factory: impl Fn() -> Box<dyn AgentAdapter> + 'static,
    ) -> Self {
        self.factories.insert(name.into(), Box::new(factory));
        self
    }

    pub fn get(&self, name: &str) -> Option<Box<dyn AgentAdapter>> {
        self.factories.get(name).map(|factory| factory())
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names = self
            .factories
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::builtins()
    }
}

/// Look up a built-in adapter by CLI/config name.
pub fn by_name(name: &str) -> Option<Box<dyn AgentAdapter>> {
    AdapterRegistry::builtins().get(name)
}

pub(crate) fn input_paths(roots: &[PathBuf], hint: ReadHint) -> Vec<PathBuf> {
    match hint {
        ReadHint::Full => roots.to_vec(),
        ReadHint::Paths(paths) if paths.is_empty() => roots.to_vec(),
        ReadHint::Paths(paths) => paths,
    }
}

pub(crate) fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        collect_file(path, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_file(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)
            .with_context(|| format!("reading artifact directory {}", path.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            collect_file(&entry.path(), files)?;
        }
    } else if path.is_file() {
        files.push(canonicalize_best_effort(path));
    }
    Ok(())
}

pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn read_jsonl(path: &Path) -> Result<Vec<(usize, Value)>> {
    read_jsonl_with_mode(path, JsonlMode::Strict)
}

pub(crate) fn read_jsonl_lossy(path: &Path) -> Result<Vec<(usize, Value)>> {
    read_jsonl_with_mode(path, JsonlMode::Lossy)
}

enum JsonlMode {
    Strict,
    Lossy,
}

fn read_jsonl_with_mode(path: &Path, mode: JsonlMode) -> Result<Vec<(usize, Value)>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(value) => values.push((index + 1, value)),
            Err(error) if matches!(mode, JsonlMode::Strict) => {
                return Err(error)
                    .with_context(|| format!("parsing {} line {}", path.display(), index + 1));
            }
            Err(_) => {}
        }
    }
    Ok(values)
}

pub(crate) struct ConversationBuilder<'a> {
    adapter: &'a str,
    path: PathBuf,
    previous_message_id: Option<String>,
    cwd: Option<PathBuf>,
    records: Vec<GraphRecord>,
}

impl<'a> ConversationBuilder<'a> {
    pub(crate) fn new(adapter: &'a str, path: PathBuf) -> Self {
        Self {
            adapter,
            path,
            previous_message_id: None,
            cwd: None,
            records: Vec::new(),
        }
    }

    pub(crate) fn set_cwd(&mut self, cwd: Option<String>) {
        if let Some(cwd) = cwd.filter(|cwd| !cwd.is_empty()) {
            self.cwd = Some(PathBuf::from(cwd));
        }
    }

    pub(crate) fn push_message_with_metadata(
        &mut self,
        role: Role,
        content: String,
        locator: String,
        metadata: Value,
    ) -> Result<String> {
        self.push_message_record(
            Message {
                node_type: GraphNodeType::Message,
                id: String::new(),
                timestamp: timestamp(&metadata),
                role,
                content,
                cwd: self.cwd.clone(),
                provider: provider(&metadata),
                model: model(&metadata),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: self.source_ref(locator.clone()),
                metadata,
            },
            locator,
            Vec::new(),
        )
    }

    pub(crate) fn push_assistant_message(
        &mut self,
        content: String,
        tool_calls: Vec<ToolCall>,
        locator: String,
        metadata: Value,
    ) -> Result<String> {
        self.push_message_record(
            Message {
                node_type: GraphNodeType::Message,
                id: String::new(),
                timestamp: timestamp(&metadata),
                role: Role::Assistant,
                content,
                cwd: self.cwd.clone(),
                provider: provider(&metadata),
                model: model(&metadata),
                tool_calls,
                tool_results: Vec::new(),
                source: self.source_ref(locator.clone()),
                metadata,
            },
            locator,
            Vec::new(),
        )
    }

    pub(crate) fn push_summary_message(
        &mut self,
        content: String,
        locator: String,
        metadata: Value,
        summarized_targets: Vec<String>,
    ) -> Result<String> {
        self.push_message_record(
            Message {
                node_type: GraphNodeType::Message,
                id: String::new(),
                timestamp: timestamp(&metadata),
                role: Role::System,
                content,
                cwd: self.cwd.clone(),
                provider: provider(&metadata),
                model: model(&metadata),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: self.source_ref(locator.clone()),
                metadata,
            },
            locator,
            summarized_targets,
        )
    }

    pub(crate) fn push_tool_result(
        &mut self,
        result: ToolResult,
        locator: String,
    ) -> Result<String> {
        self.push_message_record(
            Message {
                node_type: GraphNodeType::Message,
                id: String::new(),
                timestamp: timestamp(&result.metadata),
                role: Role::Tool,
                content: result.content.clone().unwrap_or_default(),
                cwd: self.cwd.clone(),
                provider: provider(&result.metadata),
                model: model(&result.metadata),
                tool_calls: Vec::new(),
                tool_results: vec![result],
                source: self.source_ref(locator.clone()),
                metadata: json!({}),
            },
            locator,
            Vec::new(),
        )
    }

    fn push_message_record(
        &mut self,
        mut message: Message,
        _locator: String,
        summarized_targets: Vec<String>,
    ) -> Result<String> {
        sanitize_metadata(&mut message.metadata);
        for call in &mut message.tool_calls {
            sanitize_metadata(&mut call.metadata);
        }
        for result in &mut message.tool_results {
            sanitize_metadata(&mut result.metadata);
        }

        let mut edges = Vec::new();
        if let Some(previous) = &self.previous_message_id {
            edges.push(GraphEdge {
                edge_type: EdgeType::FollowsMessage,
                target: previous.clone(),
                metadata: json!({}),
            });
        }
        for target in summarized_targets {
            edges.push(GraphEdge {
                edge_type: EdgeType::Summarizes,
                target,
                metadata: json!({}),
            });
        }

        let record = GraphRecord {
            record_type: RecordType::AgentRecord,
            node: GraphNode::Message(message),
            edges,
            integrity: None,
        }
        .with_stable_id()?;
        let id = record.id().to_string();
        self.previous_message_id = Some(id.clone());
        self.records.push(record);
        Ok(id)
    }

    pub(crate) fn finish(self) -> Vec<GraphRecord> {
        self.records
    }

    fn source_ref(&self, locator: String) -> SourceRef {
        SourceRef {
            agent_adapter: self.adapter.to_string(),
            path: Some(self.path.clone()),
            locator: Some(locator),
        }
    }
}

pub(crate) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToString::to_string)
}

pub(crate) fn provider(value: &Value) -> Option<String> {
    string_field(value, "provider")
        .or_else(|| string_field(value, "providerID"))
        .or_else(|| string_field(value, "provider_id"))
}

pub(crate) fn model(value: &Value) -> Option<String> {
    string_field(value, "model")
        .or_else(|| string_field(value, "modelID"))
        .or_else(|| string_field(value, "model_id"))
}

pub(crate) fn timestamp(value: &Value) -> Option<String> {
    normalize_timestamp(value.get("timestamp"))
        .or_else(|| {
            value
                .get("time")
                .and_then(|time| normalize_timestamp(time.get("completed")))
        })
        .or_else(|| {
            value
                .get("time")
                .and_then(|time| normalize_timestamp(time.get("created")))
        })
        .or_else(|| normalize_timestamp(value.get("created_at")))
        .or_else(|| normalize_timestamp(value.get("createdAt")))
        .or_else(|| normalize_timestamp(value.get("time_created")))
        .or_else(|| normalize_timestamp(value.get("timeCreated")))
}

pub(crate) fn normalize_timestamp(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => normalize_timestamp_string(value),
        Value::Number(value) => value.as_i64().and_then(timestamp_number_to_rfc3339),
        _ => None,
    }
}

fn normalize_timestamp_string(value: &str) -> Option<String> {
    if let Ok(parsed) = OffsetDateTime::parse(value, &Rfc3339) {
        return parsed.format(&Rfc3339).ok();
    }
    value
        .parse::<i64>()
        .ok()
        .and_then(timestamp_number_to_rfc3339)
}

fn timestamp_number_to_rfc3339(value: i64) -> Option<String> {
    let datetime = if value.abs() >= 10_000_000_000 {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(value) * 1_000_000).ok()?
    } else {
        OffsetDateTime::from_unix_timestamp(value).ok()?
    };
    datetime.format(&Rfc3339).ok()
}

fn sanitize_metadata(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in [
        "locator",
        "timestamp",
        "created_at",
        "createdAt",
        "time_created",
        "timeCreated",
        "time",
        "cwd",
        "directory",
        "provider",
        "providerID",
        "provider_id",
        "model",
        "modelID",
        "model_id",
    ] {
        object.remove(key);
    }
    object.retain(|_, value| !value.is_null());
}

pub(crate) fn text_from_content(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(|part| {
                    part.as_str()
                        .map(ToString::to_string)
                        .or_else(|| string_field(part, "text"))
                        .or_else(|| string_field(part, "content"))
                })
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        Value::Object(_) => string_field(value, "text").or_else(|| string_field(value, "content")),
        _ => None,
    }
}
