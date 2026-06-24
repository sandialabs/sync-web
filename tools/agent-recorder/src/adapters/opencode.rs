use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::{
    adapters::{
        collect_files, input_paths, read_jsonl, string_field, AgentAdapter, ConversationBuilder,
        ReadHint,
    },
    GraphRecord, Role, ToolCall, ToolResult,
};

pub struct OpenCodeAdapter;

#[derive(Debug, Clone)]
struct MessageRow {
    id: String,
    session_id: String,
    time_created: i64,
    data: Value,
}

#[derive(Debug, Clone)]
struct PartRow {
    id: String,
    message_id: String,
    time_created: i64,
    data: Value,
}

impl AgentAdapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn read(
        &self,
        roots: &[PathBuf],
        hint: ReadHint,
        emit: &mut dyn FnMut(GraphRecord) -> Result<()>,
    ) -> Result<()> {
        for path in input_paths(roots, hint) {
            if path.is_dir()
                && path.join("message.json").exists()
                && path.join("part.json").exists()
            {
                for record in parse_export_dir(self.name(), &path)? {
                    emit(record)?;
                }
            } else if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("db")
            {
                for record in parse_db(self.name(), &path)? {
                    emit(record)?;
                }
            } else {
                for file in collect_files(&[path])? {
                    let records = if file.extension().and_then(|ext| ext.to_str()) == Some("json") {
                        parse_combined_export_file(self.name(), &file)?.unwrap_or_else(|| {
                            parse_jsonl_file(self.name(), &file).unwrap_or_default()
                        })
                    } else {
                        parse_jsonl_file(self.name(), &file)?
                    };
                    for record in records {
                        emit(record)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn parse_export_dir(adapter: &str, dir: &Path) -> Result<Vec<GraphRecord>> {
    let messages = read_table_json(&dir.join("message.json"), |row| MessageRow {
        id: required_string(row, "id"),
        session_id: required_string(row, "session_id"),
        time_created: row
            .get("time_created")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        data: parse_data(row),
    })?;
    let parts = read_table_json(&dir.join("part.json"), |row| PartRow {
        id: required_string(row, "id"),
        message_id: required_string(row, "message_id"),
        time_created: row
            .get("time_created")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        data: parse_data(row),
    })?;
    build_from_rows(adapter, dir.to_path_buf(), messages, parts, HashMap::new())
}

fn parse_combined_export_file(adapter: &str, path: &Path) -> Result<Option<Vec<GraphRecord>>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let Some(messages_table) = table_values(&value, &["message", "messages"]) else {
        return Ok(None);
    };
    let Some(parts_table) = table_values(&value, &["part", "parts"]) else {
        return Ok(None);
    };

    let messages = messages_table
        .iter()
        .map(|row| MessageRow {
            id: required_string(row, "id"),
            session_id: required_string(row, "session_id"),
            time_created: row
                .get("time_created")
                .or_else(|| row.get("timeCreated"))
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            data: parse_data(row),
        })
        .collect::<Vec<_>>();
    let parts = parts_table
        .iter()
        .map(|row| PartRow {
            id: required_string(row, "id"),
            message_id: required_string(row, "message_id"),
            time_created: row
                .get("time_created")
                .or_else(|| row.get("timeCreated"))
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            data: parse_data(row),
        })
        .collect::<Vec<_>>();
    let cwd_by_session = table_values(&value, &["session", "sessions"])
        .into_iter()
        .flatten()
        .map(|row| {
            (
                required_string(row, "id"),
                string_field(row, "directory")
                    .or_else(|| string_field(row, "cwd"))
                    .or_else(|| string_field(row, "path")),
            )
        })
        .collect::<HashMap<_, _>>();

    Ok(Some(build_from_rows(
        adapter,
        path.to_path_buf(),
        messages,
        parts,
        cwd_by_session,
    )?))
}

fn table_values<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Vec<Value>> {
    if let Some(object) = value.as_object() {
        for name in names {
            if let Some(values) = object.get(*name).and_then(Value::as_array) {
                return Some(values);
            }
        }
        if let Some(tables) = object.get("tables") {
            if let Some(values) = table_values(tables, names) {
                return Some(values);
            }
        }
    }

    value.as_array().and_then(|tables| {
        tables.iter().find_map(|table| {
            let name = string_field(table, "name")
                .or_else(|| string_field(table, "table"))
                .or_else(|| string_field(table, "type"))?;
            if names.iter().any(|candidate| *candidate == name) {
                table
                    .get("rows")
                    .or_else(|| table.get("data"))
                    .and_then(Value::as_array)
            } else {
                None
            }
        })
    })
}

fn parse_db(adapter: &str, path: &Path) -> Result<Vec<GraphRecord>> {
    let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut messages_stmt = conn.prepare(
        "select id, session_id, time_created, data from message order by session_id, time_created, id",
    )?;
    let messages = messages_stmt
        .query_map([], |row| {
            let data: String = row.get(3)?;
            Ok(MessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                time_created: row.get(2)?,
                data: serde_json::from_str(&data).unwrap_or(Value::Null),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut parts_stmt = conn.prepare(
        "select id, message_id, time_created, data from part order by message_id, time_created, id",
    )?;
    let parts = parts_stmt
        .query_map([], |row| {
            let data: String = row.get(3)?;
            Ok(PartRow {
                id: row.get(0)?,
                message_id: row.get(1)?,
                time_created: row.get(2)?,
                data: serde_json::from_str(&data).unwrap_or(Value::Null),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let cwd_by_session = conn
        .prepare("select id, directory from session")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .ok()
            .map(|rows| rows.filter_map(Result::ok).collect::<HashMap<_, _>>())
        })
        .unwrap_or_default();

    build_from_rows(adapter, path.to_path_buf(), messages, parts, cwd_by_session)
}

fn build_from_rows(
    adapter: &str,
    source_path: PathBuf,
    mut messages: Vec<MessageRow>,
    mut parts: Vec<PartRow>,
    cwd_by_session: HashMap<String, Option<String>>,
) -> Result<Vec<GraphRecord>> {
    messages.sort_by_key(|message| {
        (
            message.session_id.clone(),
            message.time_created,
            message.id.clone(),
        )
    });
    parts.sort_by_key(|part| (part.message_id.clone(), part.time_created, part.id.clone()));

    let mut parts_by_message: HashMap<String, Vec<PartRow>> = HashMap::new();
    for part in parts {
        parts_by_message
            .entry(part.message_id.clone())
            .or_default()
            .push(part);
    }

    let mut builders: BTreeMap<String, ConversationBuilder<'_>> = BTreeMap::new();
    let mut finished = Vec::new();
    for message in messages {
        let session_id = message.session_id.clone();
        let builder = builders.entry(session_id.clone()).or_insert_with(|| {
            let mut builder = ConversationBuilder::new(adapter, source_path.clone());
            builder.set_cwd(cwd_by_session.get(&session_id).cloned().flatten());
            builder
        });
        let locator = format!("session:{}:message:{}", session_id, message.id);
        let message_parts = parts_by_message.remove(&message.id).unwrap_or_default();
        parse_message_row(builder, &message, &message_parts, locator)?;
    }

    for (_, builder) in builders {
        finished.extend(builder.finish());
    }
    Ok(finished)
}

fn parse_message_row(
    builder: &mut ConversationBuilder<'_>,
    message: &MessageRow,
    parts: &[PartRow],
    locator: String,
) -> Result<()> {
    match string_field(&message.data, "role").as_deref() {
        Some("user") => {
            builder.push_message_with_metadata(
                Role::User,
                text_parts(parts),
                locator,
                opencode_message_metadata(message),
            )?;
        }
        Some("assistant") => {
            let tool_calls = parts
                .iter()
                .filter(|part| string_field(&part.data, "type").as_deref() == Some("tool"))
                .map(tool_call_from_part)
                .collect();
            builder.push_assistant_message(
                text_parts(parts),
                tool_calls,
                locator.clone(),
                json!({
                    "locator": locator,
                    "time_created": message.time_created,
                    "time": message.data.get("time").cloned(),
                    "provider": string_field(&message.data, "providerID"),
                    "model": string_field(&message.data, "modelID"),
                }),
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn parse_jsonl_file(adapter: &str, path: &Path) -> Result<Vec<GraphRecord>> {
    let mut builder = ConversationBuilder::new(adapter, path.to_path_buf());

    for (line, value) in read_jsonl(path)? {
        let locator = format!("line:{line}");
        match string_field(&value, "type").as_deref() {
            Some("session") | Some("session.updated.1") | Some("session.created.1") => {}
            Some("message") => parse_jsonl_message(&mut builder, &value, locator)?,
            Some("part") | Some("message.part.updated.1") => {
                parse_jsonl_part(&mut builder, &value, locator)?
            }
            _ => {}
        }
    }

    Ok(builder.finish())
}

fn parse_jsonl_message(
    builder: &mut ConversationBuilder<'_>,
    value: &Value,
    locator: String,
) -> Result<()> {
    match string_field(value, "role").as_deref() {
        Some("user") => {
            builder.push_message_with_metadata(
                Role::User,
                string_field(value, "content").unwrap_or_default(),
                locator,
                opencode_json_metadata(value),
            )?;
        }
        Some("assistant") => {
            builder.push_assistant_message(
                string_field(value, "content").unwrap_or_default(),
                Vec::new(),
                locator.clone(),
                json!({
                    "locator": locator,
                    "timestamp": value.get("timestamp").cloned(),
                    "time_created": value.get("time_created").or_else(|| value.get("timeCreated")).cloned(),
                    "time": value.get("time").cloned(),
                    "provider": string_field(value, "providerID"),
                    "model": string_field(value, "modelID"),
                }),
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn parse_jsonl_part(
    builder: &mut ConversationBuilder<'_>,
    value: &Value,
    locator: String,
) -> Result<()> {
    match string_field(value, "partType")
        .or_else(|| string_field(value, "part_type"))
        .as_deref()
    {
        Some("tool") => {
            if value.get("output").is_some() || value.get("result").is_some() {
                builder.push_tool_result(
                    ToolResult {
                        name: string_field(value, "tool"),
                        content: string_field(value, "output")
                            .or_else(|| string_field(value, "result")),
                        is_error: value.get("error").and_then(Value::as_bool),
                        metadata: opencode_json_metadata(value),
                    },
                    locator,
                )?;
            } else {
                builder.push_assistant_message(
                    String::new(),
                    vec![tool_call_from_value(value)],
                    locator.clone(),
                    {
                        let mut metadata = opencode_json_metadata(value);
                        if let Some(object) = metadata.as_object_mut() {
                            object.insert("locator".to_string(), json!(locator));
                        }
                        metadata
                    },
                )?;
            }
        }
        Some("text") => {
            builder.push_assistant_message(
                string_field(value, "text")
                    .or_else(|| string_field(value, "content"))
                    .unwrap_or_default(),
                Vec::new(),
                locator.clone(),
                {
                    let mut metadata = opencode_json_metadata(value);
                    if let Some(object) = metadata.as_object_mut() {
                        object.insert("locator".to_string(), json!(locator));
                    }
                    metadata
                },
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn opencode_message_metadata(message: &MessageRow) -> Value {
    json!({
        "time_created": message.time_created,
        "time": message.data.get("time").cloned(),
    })
}

fn opencode_json_metadata(value: &Value) -> Value {
    json!({
        "timestamp": value.get("timestamp").cloned(),
        "time_created": value.get("time_created").or_else(|| value.get("timeCreated")).cloned(),
        "time": value.get("time").cloned(),
    })
}

fn read_table_json<T>(path: &Path, f: impl Fn(&Value) -> T) -> Result<Vec<T>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rows: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(rows.as_array().into_iter().flatten().map(f).collect())
}

fn parse_data(row: &Value) -> Value {
    row.get("data")
        .and_then(Value::as_str)
        .and_then(|data| serde_json::from_str(data).ok())
        .or_else(|| row.get("data").cloned())
        .unwrap_or(Value::Null)
}

fn required_string(row: &Value, key: &str) -> String {
    string_field(row, key).unwrap_or_default()
}

fn text_parts(parts: &[PartRow]) -> String {
    parts
        .iter()
        .filter(|part| string_field(&part.data, "type").as_deref() == Some("text"))
        .filter_map(|part| string_field(&part.data, "text"))
        .collect::<Vec<_>>()
        .join("")
}

fn tool_call_from_part(part: &PartRow) -> ToolCall {
    let mut call = tool_call_from_value(&part.data);
    if let Some(metadata) = call.metadata.as_object_mut() {
        metadata.insert("time_created".to_string(), json!(part.time_created));
    }
    call
}

fn tool_call_from_value(value: &Value) -> ToolCall {
    let state = value.get("state").unwrap_or(&Value::Null);
    ToolCall {
        name: string_field(value, "tool"),
        arguments: value
            .get("input")
            .cloned()
            .or_else(|| state.get("input").cloned()),
        metadata: json!({ "state": state }),
    }
}
