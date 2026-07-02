use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Source adapters for passive parsing of agent-owned session artifacts.
pub mod adapters;
/// Command-line interface wiring.
pub mod cli;
/// Optional key-evolving HMAC integrity support for indexed records.
pub mod integrity;
/// Output and readable backend adapters for normalized graph records.
pub mod records;

pub use adapters::{AgentAdapter, ReadHint};
pub use integrity::IntegrityMetadata;
pub use records::{IndexedGraphRecord, RecordAdapter, RecordReader, RecordSelector};

/// A flat provenance graph envelope emitted by `agent-recorder`.
///
/// Records intentionally keep transport shape simple: a single `node`, zero or
/// more typed `edges`, and optional integrity metadata. Tool calls and tool
/// results are annotations on message nodes rather than separate graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GraphRecord {
    #[serde(rename = "type")]
    pub record_type: RecordType,
    pub node: GraphNode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<GraphEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<IntegrityMetadata>,
}

impl GraphRecord {
    /// Return a copy with identity and integrity cleared for stable hashing.
    pub fn without_id(&self) -> Self {
        let mut record = self.clone();
        record.node.clear_id();
        record.integrity = None;
        record
    }

    /// Return the node identifier carried by this record.
    pub fn id(&self) -> &str {
        self.node.id()
    }

    /// Compute the deterministic content identifier for this record.
    ///
    /// Existing node ids and integrity metadata are ignored so imports are
    /// idempotent across adapters and output backends.
    pub fn stable_id(&self) -> Result<String> {
        let prefix = match self.node.node_type() {
            GraphNodeType::Message => "msg",
            GraphNodeType::Diagnostic => "diag",
        };
        Ok(format!(
            "{}_{}",
            prefix,
            sha256_hex(&canonical_json_bytes(&self.without_id())?)
        ))
    }

    /// Fill the node id with [`GraphRecord::stable_id`].
    pub fn with_stable_id(mut self) -> Result<Self> {
        let id = self.stable_id()?;
        self.node.set_id(id);
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordType {
    AgentRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphNodeType {
    Message,
    Diagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GraphNode {
    Message(Message),
    Diagnostic(DiagnosticNode),
}

impl GraphNode {
    pub fn id(&self) -> &str {
        match self {
            GraphNode::Message(node) => &node.id,
            GraphNode::Diagnostic(node) => &node.id,
        }
    }

    pub fn set_id(&mut self, id: String) {
        match self {
            GraphNode::Message(node) => node.id = id,
            GraphNode::Diagnostic(node) => node.id = id,
        }
    }

    pub fn clear_id(&mut self) {
        self.set_id(String::new());
    }

    pub fn node_type(&self) -> GraphNodeType {
        match self {
            GraphNode::Message(_) => GraphNodeType::Message,
            GraphNode::Diagnostic(_) => GraphNodeType::Diagnostic,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GraphEdge {
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
    pub target: String,
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub metadata: Value,
}

/// A normalized conversational message.
///
/// `timestamp`, `cwd`, `provider`, and `model` are promoted to top-level
/// fields when the source exposes them. Adapter-specific leftovers belong in
/// `metadata` after removing duplicates of those common fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Message {
    #[serde(rename = "type")]
    pub node_type: GraphNodeType,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
    pub source: SourceRef,
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub metadata: Value,
}

/// A tool invocation requested by an assistant/model message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub metadata: Value,
}

/// A tool result observed in the source transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub metadata: Value,
}

/// Recorder- or adapter-generated diagnostic node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DiagnosticNode {
    #[serde(rename = "type")]
    pub node_type: GraphNodeType,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    pub source: SourceRef,
    #[serde(default = "empty_object", skip_serializing_if = "is_empty_object")]
    pub metadata: Value,
}

/// Provenance pointer back to the source artifact and source-local locator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SourceRef {
    pub agent_adapter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    System,
    Developer,
    User,
    Assistant,
    Model,
    Tool,
    Runtime,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeType {
    FollowsMessage,
    ParentMessage,
    Summarizes,
    InferredFrom,
}

/// Count summary returned by import and live-recording operations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ImportReport {
    pub records: usize,
    pub duplicates: usize,
}

/// Idempotent import/live recorder state.
///
/// `Recorder` keeps only stable ids it has already seen. `run` uses
/// [`Recorder::baseline`] at startup and then repeatedly calls
/// [`Recorder::log_new`] so only newly appearing source records are emitted.
#[derive(Debug, Default)]
pub struct Recorder {
    seen: HashSet<String>,
}

impl Recorder {
    /// Create an empty recorder with no seen records.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read existing source records into the duplicate-suppression set.
    ///
    /// No records are written to a sink. This is the core distinction between
    /// `run` and `import`: live recording baselines first to avoid backfilling a
    /// user's historical session directory by accident.
    pub fn baseline(
        &mut self,
        adapter: &dyn AgentAdapter,
        roots: &[PathBuf],
    ) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        adapter.read(roots, ReadHint::Full, &mut |record| {
            let record = ensure_stable_id(record)?;
            if self.seen.insert(record.id().to_string()) {
                report.records += 1;
            } else {
                report.duplicates += 1;
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Parse source records and write only records not seen before.
    pub fn log_new(
        &mut self,
        adapter: &dyn AgentAdapter,
        roots: &[PathBuf],
        hint: ReadHint,
        sink: &mut dyn RecordAdapter,
    ) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        adapter.read(roots, hint, &mut |record| {
            let record = ensure_stable_id(record)?;
            if self.seen.insert(record.id().to_string()) {
                sink.log(&record)?;
                report.records += 1;
            } else {
                report.duplicates += 1;
            }
            Ok(())
        })?;
        Ok(report)
    }
}

fn ensure_stable_id(record: GraphRecord) -> Result<GraphRecord> {
    if record.id().is_empty() {
        record.with_stable_id()
    } else {
        Ok(record)
    }
}

/// One-shot backfill from a source adapter into a record adapter.
pub fn import_records(
    adapter: &dyn AgentAdapter,
    roots: &[PathBuf],
    sink: &mut dyn RecordAdapter,
) -> Result<ImportReport> {
    let mut recorder = Recorder::new();
    recorder.log_new(adapter, roots, ReadHint::Full, sink)
}

/// Serialize JSON with deterministic object-key ordering.
///
/// This is used for stable ids and integrity payload hashes. It is intentionally
/// small and JSON-specific rather than a general-purpose canonicalization
/// framework.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let mut out = Vec::new();
    write_canonical_json(&value, &mut out)?;
    Ok(out)
}

/// Return lowercase hexadecimal SHA-256 for `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

pub(crate) fn is_empty_object(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.is_empty())
}

fn write_canonical_json(value: &Value, out: &mut Vec<u8>) -> Result<()> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(value) => {
            out.extend_from_slice(if *value { &b"true"[..] } else { &b"false"[..] })
        }
        Value::Number(value) => out.extend_from_slice(value.to_string().as_bytes()),
        Value::String(value) => serde_json::to_writer(out, value)?,
        Value::Array(values) => {
            out.push(b'[');
            for (index, item) in values.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_canonical_json(item, out)?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            out.push(b'{');
            let sorted: BTreeMap<_, _> = map.iter().collect();
            for (index, (key, value)) in sorted.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                serde_json::to_writer(&mut *out, key)?;
                out.push(b':');
                write_canonical_json(value, out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample_message(id: &str) -> GraphRecord {
        GraphRecord {
            record_type: RecordType::AgentRecord,
            node: GraphNode::Message(Message {
                node_type: GraphNodeType::Message,
                id: id.to_string(),
                timestamp: None,
                role: Role::User,
                content: "hello".to_string(),
                cwd: None,
                provider: None,
                model: None,
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: SourceRef {
                    agent_adapter: "test".to_string(),
                    path: None,
                    locator: Some("line:1".to_string()),
                },
                metadata: json!({}),
            }),
            edges: Vec::new(),
            integrity: None,
        }
    }

    #[test]
    fn serializes_kebab_case_flat_records() {
        let record = sample_message("msg_example");
        let value = serde_json::to_value(record).unwrap();
        assert_eq!(value["type"], "agent-record");
        assert_eq!(value["node"]["type"], "message");
        assert!(value["node"].get("tool-calls").is_none());
        assert!(value["node"].get("tool_calls").is_none());
    }

    #[test]
    fn stable_ids_ignore_existing_id() {
        let first = sample_message("").stable_id().unwrap();
        let second = sample_message("msg_existing").stable_id().unwrap();
        assert_eq!(first, second);
        assert!(first.starts_with("msg_"));
    }
}
