use anyhow::{Context, Result};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{records::RecordAdapter, GraphNode, GraphRecord};

pub struct OtelRecordAdapter {
    endpoint: String,
}

impl OtelRecordAdapter {
    pub fn create(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        anyhow::ensure!(!endpoint.is_empty(), "otel endpoint must not be empty");
        Ok(Self { endpoint })
    }
}

impl RecordAdapter for OtelRecordAdapter {
    fn name(&self) -> &'static str {
        "otel"
    }

    fn log(&mut self, record: &GraphRecord) -> Result<()> {
        let payload = otlp_logs_payload(record)?;
        ureq::post(&self.endpoint)
            .set("content-type", "application/json")
            .send_string(&payload.to_string())
            .with_context(|| format!("posting OTLP logs to {}", self.endpoint))?;
        Ok(())
    }
}

fn otlp_logs_payload(record: &GraphRecord) -> Result<Value> {
    Ok(json!({
        "resourceLogs": [
            {
                "resource": {
                    "attributes": [
                        string_attr("service.name", "agent-recorder"),
                        string_attr("telemetry.sdk.language", "rust"),
                    ]
                },
                "scopeLogs": [
                    {
                        "scope": {"name": "agent-recorder"},
                        "logRecords": [otlp_log_record(record)?]
                    }
                ]
            }
        ]
    }))
}

fn otlp_log_record(record: &GraphRecord) -> Result<Value> {
    let mut log = json!({
        "severityText": "INFO",
        "body": {"stringValue": serde_json::to_string(record)?},
        "attributes": attributes(record),
    });

    if let Some(timestamp) = record_timestamp(record).and_then(timestamp_unix_nanos) {
        log.as_object_mut()
            .expect("log is object")
            .insert("timeUnixNano".to_string(), json!(timestamp));
    }

    Ok(log)
}

fn attributes(record: &GraphRecord) -> Vec<Value> {
    let mut attrs = vec![
        string_attr("agent_recorder.node_id", record.id()),
        string_attr(
            "agent_recorder.node_type",
            serde_json::to_value(record.node.node_type())
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        int_attr("agent_recorder.edge_count", record.edges.len() as i64),
    ];

    match &record.node {
        GraphNode::Message(message) => {
            attrs.push(string_attr(
                "agent_recorder.role",
                serde_json::to_value(message.role.clone())
                    .ok()
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .unwrap_or_else(|| "unknown".to_string()),
            ));
            if let Some(cwd) = &message.cwd {
                attrs.push(string_attr("agent_recorder.cwd", cwd.display().to_string()));
            }
            if let Some(provider) = &message.provider {
                attrs.push(string_attr("agent_recorder.provider", provider));
            }
            if let Some(model) = &message.model {
                attrs.push(string_attr("agent_recorder.model", model));
            }
            attrs.extend(source_attrs(
                &message.source.agent_adapter,
                &message.source.path,
                &message.source.locator,
            ));
        }
        GraphNode::Diagnostic(diagnostic) => {
            if let Some(cwd) = &diagnostic.cwd {
                attrs.push(string_attr("agent_recorder.cwd", cwd.display().to_string()));
            }
            if let Some(severity) = &diagnostic.severity {
                attrs.push(string_attr("agent_recorder.diagnostic.severity", severity));
            }
            attrs.extend(source_attrs(
                &diagnostic.source.agent_adapter,
                &diagnostic.source.path,
                &diagnostic.source.locator,
            ));
        }
    }

    attrs
}

fn source_attrs(
    adapter: &str,
    path: &Option<std::path::PathBuf>,
    locator: &Option<String>,
) -> Vec<Value> {
    let mut attrs = vec![string_attr("agent_recorder.source.adapter", adapter)];
    if let Some(path) = path {
        attrs.push(string_attr(
            "agent_recorder.source.path",
            path.display().to_string(),
        ));
    }
    if let Some(locator) = locator {
        attrs.push(string_attr("agent_recorder.source.locator", locator));
    }
    attrs
}

fn record_timestamp(record: &GraphRecord) -> Option<&str> {
    match &record.node {
        GraphNode::Message(message) => message.timestamp.as_deref(),
        GraphNode::Diagnostic(diagnostic) => diagnostic.timestamp.as_deref(),
    }
}

fn timestamp_unix_nanos(timestamp: &str) -> Option<String> {
    let parsed = OffsetDateTime::parse(timestamp, &Rfc3339).ok()?;
    Some((parsed.unix_timestamp_nanos()).to_string())
}

fn string_attr(key: impl Into<String>, value: impl ToString) -> Value {
    json!({"key": key.into(), "value": {"stringValue": value.to_string()}})
}

fn int_attr(key: impl Into<String>, value: i64) -> Value {
    json!({"key": key.into(), "value": {"intValue": value.to_string()}})
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    use serde_json::Value;

    use super::*;
    use crate::{EdgeType, GraphEdge, GraphNodeType, Message, RecordType, Role, SourceRef};

    fn sample_record() -> GraphRecord {
        GraphRecord {
            record_type: RecordType::AgentRecord,
            node: GraphNode::Message(Message {
                node_type: GraphNodeType::Message,
                id: "msg_test".to_string(),
                timestamp: Some("1970-01-01T00:00:01Z".to_string()),
                role: Role::Assistant,
                content: "hello".to_string(),
                cwd: Some(PathBuf::from("/tmp/example")),
                provider: Some("openai".to_string()),
                model: Some("gpt-test".to_string()),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: SourceRef {
                    agent_adapter: "pi".to_string(),
                    path: Some(PathBuf::from("session.jsonl")),
                    locator: Some("line:2".to_string()),
                },
                metadata: json!({}),
            }),
            edges: vec![GraphEdge {
                edge_type: EdgeType::FollowsMessage,
                target: "msg_prev".to_string(),
                metadata: json!({}),
            }],
            integrity: None,
        }
    }

    #[test]
    fn builds_otlp_logs_payload() -> Result<()> {
        let payload = otlp_logs_payload(&sample_record())?;
        let log = &payload["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0];
        assert_eq!(log["timeUnixNano"], "1000000000");
        assert_eq!(log["severityText"], "INFO");

        let body: GraphRecord = serde_json::from_str(log["body"]["stringValue"].as_str().unwrap())?;
        assert_eq!(body.id(), "msg_test");

        let attrs = log["attributes"].as_array().unwrap();
        assert!(has_attr(attrs, "agent_recorder.role", "assistant"));
        assert!(has_attr(attrs, "agent_recorder.cwd", "/tmp/example"));
        assert!(has_attr(attrs, "agent_recorder.provider", "openai"));
        assert!(has_attr(attrs, "agent_recorder.model", "gpt-test"));
        assert!(has_attr(attrs, "agent_recorder.source.adapter", "pi"));
        Ok(())
    }

    #[test]
    fn posts_otlp_payload_over_http() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let handle = thread::spawn(move || -> Result<String> {
            let (mut stream, _) = listener.accept()?;
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let n = stream.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let text = String::from_utf8_lossy(&request);
                    let length = content_length(&text).unwrap_or(0);
                    let header_end = text.find("\r\n\r\n").unwrap() + 4;
                    if request.len() >= header_end + length {
                        break;
                    }
                }
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")?;
            Ok(String::from_utf8(request)?)
        });

        let mut adapter = OtelRecordAdapter::create(format!("http://{addr}/v1/logs"))?;
        adapter.log(&sample_record())?;
        let request = handle.join().expect("server thread")?;
        assert!(request.starts_with("POST /v1/logs HTTP/1.1"));
        assert!(request.contains("content-type: application/json"));
        assert!(request.contains("msg_test"));
        assert!(request.contains("agent_recorder.source.adapter"));
        Ok(())
    }

    fn has_attr(attrs: &[Value], key: &str, value: &str) -> bool {
        attrs
            .iter()
            .any(|attr| attr["key"] == key && attr["value"]["stringValue"].as_str() == Some(value))
    }

    fn content_length(request: &str) -> Option<usize> {
        request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
    }
}
