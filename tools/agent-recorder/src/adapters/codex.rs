use std::path::{Path, PathBuf};

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::{
    adapters::{
        collect_files, input_paths, read_jsonl, string_field, text_from_content, AgentAdapter,
        ConversationBuilder, ReadHint,
    },
    GraphRecord, Role, ToolCall, ToolResult,
};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn read(
        &self,
        roots: &[PathBuf],
        hint: ReadHint,
        emit: &mut dyn FnMut(GraphRecord) -> Result<()>,
    ) -> Result<()> {
        for path in collect_files(&input_paths(roots, hint))? {
            if path.extension().and_then(|ext| ext.to_str()) == Some("sqlite") {
                for record in parse_state_db(self.name(), &path)? {
                    emit(record)?;
                }
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
                for record in parse_file(self.name(), &path)? {
                    emit(record)?;
                }
            }
        }
        Ok(())
    }
}

fn parse_state_db(adapter: &str, path: &Path) -> Result<Vec<GraphRecord>> {
    let conn = Connection::open(path)?;
    let Ok(mut stmt) = conn.prepare("select rollout_path from threads order by created_at_ms, id")
    else {
        return Ok(Vec::new());
    };
    let rollout_paths = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut records = Vec::new();
    for rollout_path in rollout_paths {
        let rollout_path = PathBuf::from(rollout_path);
        if rollout_path.exists() {
            records.extend(parse_file(adapter, &rollout_path)?);
        }
    }
    Ok(records)
}

fn parse_file(adapter: &str, path: &Path) -> Result<Vec<GraphRecord>> {
    let mut builder = ConversationBuilder::new(adapter, path.to_path_buf());

    for (line, value) in read_jsonl(path)? {
        let locator = format!("line:{line}");
        let payload = value.get("payload").unwrap_or(&value);
        builder.set_cwd(string_field(payload, "cwd"));
        match string_field(&value, "type").as_deref() {
            Some("turn_context") | Some("session_meta") => {}
            Some("event_msg") => {
                parse_event_msg(&mut builder, payload, locator, value.get("timestamp"))?
            }
            Some("response_item") => {
                parse_response_item(&mut builder, payload, locator, value.get("timestamp"))?
            }
            _ => {}
        }
    }

    Ok(builder.finish())
}

fn parse_event_msg(
    builder: &mut ConversationBuilder<'_>,
    value: &Value,
    locator: String,
    timestamp: Option<&Value>,
) -> Result<()> {
    let role = match string_field(value, "role").as_deref() {
        Some("system") => Role::System,
        Some("assistant") => Role::Assistant,
        Some("tool") => Role::Tool,
        _ => Role::User,
    };
    let content = string_field(value, "message")
        .or_else(|| text_from_content(value.get("content").unwrap_or(&Value::Null)))
        .unwrap_or_default();
    if !content.is_empty() {
        builder.push_message_with_metadata(
            role,
            content,
            locator,
            json!({ "timestamp": timestamp.cloned() }),
        )?;
    }
    Ok(())
}

fn parse_response_item(
    builder: &mut ConversationBuilder<'_>,
    value: &Value,
    locator: String,
    timestamp: Option<&Value>,
) -> Result<()> {
    let item = value.get("item").unwrap_or(value);
    match string_field(item, "type").as_deref() {
        Some("message") => {
            let role = match string_field(item, "role").as_deref() {
                Some("user") => Role::User,
                Some("tool") => Role::Tool,
                _ => Role::Assistant,
            };
            let content =
                text_from_content(item.get("content").unwrap_or(&Value::Null)).unwrap_or_default();
            if matches!(role, Role::Assistant) {
                builder.push_assistant_message(
                    content,
                    Vec::new(),
                    locator.clone(),
                    json!({ "locator": locator, "timestamp": timestamp.cloned() }),
                )?;
            } else {
                builder.push_message_with_metadata(
                    role,
                    content,
                    locator,
                    json!({ "timestamp": timestamp.cloned() }),
                )?;
            }
        }
        Some("function_call") | Some("custom_tool_call") => {
            builder.push_assistant_message(
                String::new(),
                vec![ToolCall {
                    name: string_field(item, "name"),
                    arguments: item
                        .get("arguments")
                        .or_else(|| item.get("input"))
                        .and_then(|arguments| {
                            arguments
                                .as_str()
                                .and_then(|text| serde_json::from_str(text).ok())
                                .or_else(|| Some(arguments.clone()))
                        }),
                    metadata: json!({}),
                }],
                locator.clone(),
                json!({ "locator": locator, "timestamp": timestamp.cloned() }),
            )?;
        }
        Some("function_call_output") | Some("custom_tool_call_output") => {
            builder.push_tool_result(
                ToolResult {
                    name: string_field(item, "name"),
                    content: text_from_content(item.get("output").unwrap_or(&Value::Null)),
                    is_error: item.get("is_error").and_then(Value::as_bool),
                    metadata: json!({ "timestamp": timestamp.cloned() }),
                },
                locator,
            )?;
        }
        _ => {}
    }
    Ok(())
}
