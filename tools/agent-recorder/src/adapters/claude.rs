use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use crate::{
    adapters::{
        collect_files, input_paths, read_jsonl, string_field, text_from_content, AgentAdapter,
        ConversationBuilder, ReadHint,
    },
    GraphRecord, Role, ToolCall, ToolResult,
};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "claude"
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

    for (line, value) in read_jsonl(path)? {
        let locator = format!("line:{line}");
        builder.set_cwd(string_field(&value, "cwd"));
        let message = value.get("message").unwrap_or(&value);
        let record_type = string_field(&value, "type").or_else(|| string_field(message, "role"));
        match record_type.as_deref() {
            Some("user") => parse_user(&mut builder, &value, message, locator)?,
            Some("assistant") => parse_assistant(&mut builder, &value, message, locator)?,
            Some("system") => {
                builder.push_message_with_metadata(
                    Role::System,
                    text_from_content(message.get("content").unwrap_or(&Value::Null))
                        .unwrap_or_default(),
                    locator,
                    timestamp_metadata(&value, message),
                )?;
            }
            _ => {}
        }
    }

    Ok(builder.finish())
}

fn parse_user(
    builder: &mut ConversationBuilder<'_>,
    row: &Value,
    message: &Value,
    locator: String,
) -> Result<()> {
    let content = message.get("content").unwrap_or(&Value::Null);
    let tool_results = content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|part| string_field(part, "type").as_deref() == Some("tool_result"))
                .map(|part| ToolResult {
                    name: string_field(part, "name"),
                    content: text_from_content(part.get("content").unwrap_or(&Value::Null)),
                    is_error: part.get("is_error").and_then(Value::as_bool),
                    metadata: timestamp_metadata(row, message),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if tool_results.is_empty() {
        builder.push_message_with_metadata(
            Role::User,
            text_from_content(content).unwrap_or_default(),
            locator,
            timestamp_metadata(row, message),
        )?;
    } else {
        for result in tool_results {
            builder.push_tool_result(result, locator.clone())?;
        }
    }
    Ok(())
}

fn parse_assistant(
    builder: &mut ConversationBuilder<'_>,
    row: &Value,
    message: &Value,
    locator: String,
) -> Result<()> {
    let content = message.get("content").unwrap_or(&Value::Null);
    let text = text_from_content(content);
    let tool_calls = content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter(|part| string_field(part, "type").as_deref() == Some("tool_use"))
                .map(|part| ToolCall {
                    name: string_field(part, "name"),
                    arguments: part.get("input").cloned(),
                    metadata: json!({}),
                })
                .collect()
        })
        .unwrap_or_default();
    builder.push_assistant_message(text.unwrap_or_default(), tool_calls, locator.clone(), {
        let mut metadata = timestamp_metadata(row, message);
        if let Some(object) = metadata.as_object_mut() {
            object.insert("locator".to_string(), json!(locator));
            object.insert("model".to_string(), json!(string_field(message, "model")));
        }
        metadata
    })?;
    Ok(())
}

fn timestamp_metadata(row: &Value, message: &Value) -> Value {
    json!({
        "timestamp": message.get("timestamp").or_else(|| row.get("timestamp")).cloned(),
        "created_at": message.get("created_at").or_else(|| row.get("created_at")).cloned(),
        "createdAt": message.get("createdAt").or_else(|| row.get("createdAt")).cloned(),
    })
}
