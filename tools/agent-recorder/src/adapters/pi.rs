use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use crate::{
    adapters::{
        collect_files, input_paths, read_jsonl_lossy, string_field, AgentAdapter,
        ConversationBuilder, ReadHint,
    },
    GraphRecord, Role, ToolCall, ToolResult,
};

pub struct PiAdapter;

impl AgentAdapter for PiAdapter {
    fn name(&self) -> &'static str {
        "pi"
    }

    fn read(
        &self,
        roots: &[PathBuf],
        hint: ReadHint,
        emit: &mut dyn FnMut(GraphRecord) -> Result<()>,
    ) -> Result<()> {
        for path in collect_files(&input_paths(roots, hint))? {
            if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                for record in parse_file(self.name(), &path)? {
                    emit(record)?;
                }
            }
        }
        Ok(())
    }
}

fn parse_file(adapter: &str, path: &Path) -> Result<Vec<GraphRecord>> {
    let mut builder = ConversationBuilder::new(adapter, path.to_path_buf());
    let mut source_ids = HashMap::new();

    for (line, value) in read_jsonl_lossy(path)? {
        let locator = format!("line:{line}");
        match string_field(&value, "type").as_deref() {
            Some("session") => builder.set_cwd(string_field(&value, "cwd")),
            Some("session_info") | Some("model_change") => {}
            Some("message") => {
                if let Some((source_id, node_id)) = parse_message(&mut builder, &value, locator)? {
                    source_ids.insert(source_id, node_id);
                }
            }
            Some("compaction") => parse_compaction(&mut builder, &source_ids, &value, locator)?,
            _ => {}
        }
    }

    Ok(builder.finish())
}

fn parse_message(
    builder: &mut ConversationBuilder<'_>,
    value: &Value,
    locator: String,
) -> Result<Option<(String, String)>> {
    let source_id = string_field(value, "id");
    let message = value.get("message").unwrap_or(value);
    match string_field(message, "role").as_deref() {
        Some("user") => {
            let node_id = builder.push_message_with_metadata(
                Role::User,
                content_text(message),
                locator,
                json!({ "timestamp": message.get("timestamp").cloned() }),
            )?;
            Ok(source_id.map(|source_id| (source_id, node_id)))
        }
        Some("assistant") => {
            let node_id = builder.push_assistant_message(
                content_text(message),
                tool_calls_from_content(message),
                locator.clone(),
                json!({
                    "locator": locator,
                    "timestamp": message.get("timestamp").cloned(),
                    "provider": string_field(message, "provider"),
                    "model": string_field(message, "model"),
                    "stop-reason": string_field(message, "stopReason"),
                }),
            )?;
            Ok(source_id.map(|source_id| (source_id, node_id)))
        }
        Some("toolResult") | Some("tool") => {
            let node_id = builder.push_tool_result(
                ToolResult {
                    name: string_field(message, "toolName")
                        .or_else(|| string_field(message, "name")),
                    content: Some(content_text(message)),
                    is_error: message
                        .get("isError")
                        .or_else(|| message.get("is_error"))
                        .and_then(Value::as_bool),
                    metadata: json!({ "timestamp": message.get("timestamp").cloned() }),
                },
                locator,
            )?;
            Ok(source_id.map(|source_id| (source_id, node_id)))
        }
        _ => Ok(None),
    }
}

fn parse_compaction(
    builder: &mut ConversationBuilder<'_>,
    source_ids: &HashMap<String, String>,
    value: &Value,
    locator: String,
) -> Result<()> {
    let targets = ["firstKeptEntryId", "parentId"]
        .iter()
        .filter_map(|key| string_field(value, key))
        .filter_map(|source_id| source_ids.get(&source_id).cloned())
        .collect::<Vec<_>>();
    builder.push_summary_message(
        string_field(value, "summary").unwrap_or_default(),
        locator.clone(),
        json!({
            "locator": locator,
            "timestamp": value.get("timestamp").cloned(),
            "compaction": true,
            "tokens-before": value.get("tokensBefore").cloned(),
            "from-hook": value.get("fromHook").and_then(Value::as_bool),
        }),
        targets,
    )?;
    Ok(())
}

fn content_text(message: &Value) -> String {
    message
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| string_field(part, "text"))
                .collect::<Vec<_>>()
                .join("")
        })
        .or_else(|| string_field(message, "content"))
        .unwrap_or_default()
}

fn tool_calls_from_content(message: &Value) -> Vec<ToolCall> {
    message
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|part| string_field(part, "type").as_deref() == Some("toolCall"))
                .map(|part| ToolCall {
                    name: string_field(part, "name"),
                    arguments: part.get("arguments").cloned(),
                    metadata: json!({}),
                })
                .collect()
        })
        .unwrap_or_default()
}
