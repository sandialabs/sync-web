use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use crate::{
    adapters::{
        collect_files, input_paths, read_jsonl_lossy, string_field, text_from_content,
        AgentAdapter, ConversationBuilder, ReadHint,
    },
    GraphRecord, Role, ToolCall, ToolResult,
};

pub struct GeminiAdapter;

impl AgentAdapter for GeminiAdapter {
    fn name(&self) -> &'static str {
        "gemini"
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
    let mut entries = Vec::<Value>::new();
    let mut by_id = HashMap::<String, usize>::new();

    for (line, value) in read_jsonl_lossy(path)? {
        if let Some(cwd) = cwd_from_row(&value) {
            builder.set_cwd(Some(cwd));
        }

        if value.get("$set").is_some() {
            continue;
        }
        let Some(record_type) = string_field(&value, "type") else {
            continue;
        };
        if !matches!(
            record_type.as_str(),
            "user" | "gemini" | "assistant" | "system"
        ) {
            continue;
        }

        let mut value = value;
        if let Some(object) = value.as_object_mut() {
            object.insert("locator".to_string(), json!(format!("line:{line}")));
        }

        if let Some(id) = string_field(&value, "id") {
            if matches!(record_type.as_str(), "gemini" | "assistant") {
                if let Some(index) = by_id.get(&id).copied() {
                    entries[index] = value;
                    continue;
                }
                by_id.insert(id, entries.len());
            }
        }
        entries.push(value);
    }

    for value in entries {
        let locator = string_field(&value, "locator").unwrap_or_default();
        match string_field(&value, "type").as_deref() {
            Some("user") if is_tool_result_row(&value) => {
                for result in tool_results_from_content(&value) {
                    builder.push_tool_result(result, locator.clone())?;
                }
            }
            Some("user") => {
                builder.push_message_with_metadata(
                    Role::User,
                    text_from_content(value.get("content").unwrap_or(&Value::Null))
                        .unwrap_or_default(),
                    locator,
                    metadata(&value),
                )?;
            }
            Some("gemini") | Some("assistant") => {
                builder.push_assistant_message(
                    string_field(&value, "content").unwrap_or_default(),
                    tool_calls(&value),
                    locator,
                    metadata(&value),
                )?;
            }
            Some("system") => {
                builder.push_message_with_metadata(
                    Role::System,
                    text_from_content(value.get("content").unwrap_or(&Value::Null))
                        .unwrap_or_default(),
                    locator,
                    metadata(&value),
                )?;
            }
            _ => {}
        }
    }

    Ok(builder.finish())
}

fn cwd_from_row(value: &Value) -> Option<String> {
    let set_messages = value
        .get("$set")
        .and_then(|set| set.get("messages"))
        .and_then(Value::as_array)?;
    let text = set_messages
        .iter()
        .filter_map(|message| text_from_content(message.get("content").unwrap_or(&Value::Null)))
        .find(|text| text.contains("Workspace Directories:"))?;
    workspace_dir_from_context(&text)
}

fn workspace_dir_from_context(text: &str) -> Option<String> {
    let mut in_workspace_section = false;
    for line in text.lines() {
        if line.contains("Workspace Directories:") {
            in_workspace_section = true;
            continue;
        }
        if in_workspace_section {
            let trimmed = line.trim();
            if let Some(path) = trimmed.strip_prefix("- ") {
                return Some(path.trim().to_string());
            }
            if trimmed.starts_with("**") || trimmed.is_empty() {
                in_workspace_section = false;
            }
        }
    }
    None
}

fn is_tool_result_row(value: &Value) -> bool {
    value
        .get("content")
        .and_then(Value::as_array)
        .is_some_and(|parts| {
            parts
                .iter()
                .any(|part| part.get("functionResponse").is_some())
        })
}

fn tool_calls(value: &Value) -> Vec<ToolCall> {
    value
        .get("toolCalls")
        .and_then(Value::as_array)
        .map(|calls| calls.iter().map(tool_call).collect())
        .unwrap_or_default()
}

fn tool_call(value: &Value) -> ToolCall {
    ToolCall {
        name: string_field(value, "name"),
        arguments: value.get("args").cloned(),
        metadata: json!({
            "id": string_field(value, "id"),
            "status": string_field(value, "status"),
            "timestamp": value.get("timestamp").cloned(),
            "display-name": string_field(value, "displayName"),
            "description": string_field(value, "description"),
        }),
    }
}

fn tool_results_from_content(value: &Value) -> Vec<ToolResult> {
    value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("functionResponse"))
        .map(|response| ToolResult {
            name: string_field(response, "name"),
            content: response_text(response),
            is_error: response
                .get("response")
                .and_then(|response| response.get("error"))
                .is_some()
                .then_some(true),
            metadata: json!({
                "id": string_field(response, "id"),
                "timestamp": value.get("timestamp").cloned(),
            }),
        })
        .collect()
}

fn response_text(response: &Value) -> Option<String> {
    response.get("response").and_then(|response| {
        string_field(response, "output")
            .or_else(|| string_field(response, "error"))
            .or_else(|| serde_json::to_string(response).ok())
    })
}

fn metadata(value: &Value) -> Value {
    json!({
        "timestamp": value.get("timestamp").cloned(),
        "model": string_field(value, "model"),
        "tokens": value.get("tokens").cloned(),
        "session-id": string_field(value, "sessionId"),
        "kind": string_field(value, "kind"),
    })
}
