use std::env;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

use crate::{
    records::{IndexedGraphRecord, RecordAdapter, RecordReader, RecordSelector},
    GraphRecord,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncWebMode {
    Gateway,
    DirectJournal,
}

#[derive(Debug, Clone)]
pub enum SyncWebAuth {
    GatewayApiKey(String),
    JournalSecret(String),
}

#[derive(Debug, Clone)]
pub struct SyncWebConfig {
    pub endpoint: String,
    pub mode: SyncWebMode,
    pub auth: SyncWebAuth,
    pub path_prefix: Vec<String>,
    pub start_index: u64,
}

pub struct SyncWebRecordAdapter {
    client: SyncWebClient,
    next_index: u64,
}

pub struct SyncWebRecordReader {
    client: SyncWebClient,
}

#[derive(Debug, Clone)]
struct SyncWebClient {
    endpoint: String,
    mode: SyncWebMode,
    auth: SyncWebAuth,
    path_prefix: Vec<String>,
    agent: ureq::Agent,
}

impl SyncWebConfig {
    pub fn gateway(endpoint: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            mode: SyncWebMode::Gateway,
            auth: SyncWebAuth::GatewayApiKey(api_key.into()),
            path_prefix: default_path_prefix(),
            start_index: 0,
        }
    }

    pub fn direct_journal(endpoint: impl Into<String>, journal_secret: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            mode: SyncWebMode::DirectJournal,
            auth: SyncWebAuth::JournalSecret(journal_secret.into()),
            path_prefix: default_path_prefix(),
            start_index: 0,
        }
    }
}

impl SyncWebRecordAdapter {
    pub fn create(config: SyncWebConfig) -> Result<Self> {
        let next_index = config.start_index;
        Ok(Self {
            client: SyncWebClient::create(config)?,
            next_index,
        })
    }
}

impl SyncWebRecordReader {
    pub fn create(config: SyncWebConfig) -> Result<Self> {
        Ok(Self {
            client: SyncWebClient::create(config)?,
        })
    }
}

impl RecordAdapter for SyncWebRecordAdapter {
    fn name(&self) -> &'static str {
        "sync-web"
    }

    fn log(&mut self, record: &GraphRecord) -> Result<()> {
        let index = self.next_index;
        self.client.set_record(index, record)?;
        self.next_index += 1;
        Ok(())
    }
}

impl RecordReader for SyncWebRecordReader {
    fn name(&self) -> &'static str {
        "sync-web"
    }

    fn read(
        &self,
        selector: RecordSelector,
        emit: &mut dyn FnMut(IndexedGraphRecord) -> Result<()>,
    ) -> Result<()> {
        match selector {
            RecordSelector::Index(index) => {
                if let Some(record) = self.client.get_record(index)? {
                    emit(IndexedGraphRecord { index, record })?;
                }
            }
            RecordSelector::Range { start, end } => {
                for index in start..end {
                    if let Some(record) = self.client.get_record(index)? {
                        emit(IndexedGraphRecord { index, record })?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl SyncWebClient {
    fn create(config: SyncWebConfig) -> Result<Self> {
        if config.endpoint.trim().is_empty() {
            bail!("Sync Web endpoint cannot be empty");
        }
        if config.path_prefix.is_empty() {
            bail!("Sync Web path prefix cannot be empty");
        }
        Ok(Self {
            endpoint: config.endpoint,
            mode: config.mode,
            auth: config.auth,
            path_prefix: config.path_prefix,
            agent: ureq::AgentBuilder::new().build(),
        })
    }

    fn set_record(&self, index: u64, record: &GraphRecord) -> Result<()> {
        let payload = hex::encode(serde_json::to_vec(record)?);
        let args = json!({
            "path": self.path(index),
            "value": {"*type/byte-vector*": payload}
        });
        let response = self.call_general("set", "set!", args)?;
        ensure_not_error(&response)?;
        Ok(())
    }

    fn get_record(&self, index: u64) -> Result<Option<GraphRecord>> {
        let response = self.call_general("get", "get", json!({ "path": self.path(index) }))?;
        if is_nothing(&response) {
            return Ok(None);
        }
        let Some(hex) = response
            .as_object()
            .and_then(|object| object.get("*type/byte-vector*"))
            .and_then(Value::as_str)
        else {
            ensure_not_error(&response)?;
            bail!("Sync Web get returned non-byte-vector value: {response}");
        };
        let bytes = hex::decode(hex).with_context(|| "decoding Sync Web byte-vector response")?;
        let record = serde_json::from_slice::<GraphRecord>(&bytes)
            .with_context(|| format!("decoding GraphRecord stored at index {index}"))?;
        Ok(Some(record))
    }

    fn path(&self, index: u64) -> Value {
        let mut path = self
            .path_prefix
            .iter()
            .map(|segment| Value::String(segment.clone()))
            .collect::<Vec<_>>();
        path.push(Value::String(index_path_segment(index)));
        Value::Array(path)
    }

    fn call_general(
        &self,
        gateway_operation: &str,
        journal_function: &str,
        args: Value,
    ) -> Result<Value> {
        match self.mode {
            SyncWebMode::Gateway => self.call_gateway(gateway_operation, args),
            SyncWebMode::DirectJournal => self.call_direct_journal(journal_function, args),
        }
    }

    fn call_gateway(&self, operation: &str, args: Value) -> Result<Value> {
        let url = gateway_operation_url(&self.endpoint, operation);
        let SyncWebAuth::GatewayApiKey(token) = &self.auth else {
            bail!("gateway Sync Web mode requires API-key auth");
        };
        let response = self
            .agent
            .post(&url)
            .set("content-type", "application/json")
            .set("authorization", &format!("Bearer {token}"))
            .send_string(&serde_json::to_string(&args)?)
            .with_context(|| format!("calling Sync Web gateway {url}"))?;
        read_json_response(response)
    }

    fn call_direct_journal(&self, function: &str, args: Value) -> Result<Value> {
        let SyncWebAuth::JournalSecret(secret) = &self.auth else {
            bail!("direct journal Sync Web mode requires journal-secret auth");
        };
        let body = json!({
            "function": function,
            "arguments": args,
            "authentication": {
                "credentials": {"*type/string*": secret}
            }
        });
        let response = self
            .agent
            .post(&self.endpoint)
            .set("content-type", "application/json")
            .send_string(&serde_json::to_string(&body)?)
            .with_context(|| format!("calling Sync Web journal {}", self.endpoint))?;
        read_json_response(response)
    }
}

pub fn default_path_prefix() -> Vec<String> {
    vec!["*state*".to_string(), "agent-recorder".to_string()]
}

pub fn index_path_segment(index: u64) -> String {
    format!("entry-{index:012}")
}

pub fn parse_path_prefix(input: &str) -> Result<Vec<String>> {
    let segments = input
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.is_empty() {
        bail!("Sync Web path prefix cannot be empty");
    }
    Ok(segments)
}

pub fn secret_from_literal_or_env(
    literal: Option<String>,
    env_name: Option<String>,
) -> Result<Option<String>> {
    match (literal, env_name) {
        (Some(value), None) if !value.is_empty() => Ok(Some(value)),
        (None, Some(name)) if !name.is_empty() => env::var(&name)
            .map(Some)
            .with_context(|| format!("reading secret from ${name}")),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(anyhow!(
            "provide either a literal secret or a secret env var, not both"
        )),
        _ => Ok(None),
    }
}

fn gateway_operation_url(endpoint: &str, operation: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    if base.ends_with("/api/v1/general") {
        format!("{base}/{operation}")
    } else if base.contains("/api/v1/general/") {
        base.to_string()
    } else {
        format!("{base}/api/v1/general/{operation}")
    }
}

fn read_json_response(response: ureq::Response) -> Result<Value> {
    let status = response.status();
    let text = response.into_string()?;
    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
    if !(200..300).contains(&status) {
        bail!("Sync Web HTTP {status}: {value}");
    }
    Ok(value)
}

fn ensure_not_error(value: &Value) -> Result<()> {
    if value
        .as_array()
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        == Some("error")
    {
        bail!("Sync Web returned error: {value}");
    }
    Ok(())
}

fn is_nothing(value: &Value) -> bool {
    value.as_str() == Some("nothing")
        || value
            .as_object()
            .and_then(|object| object.get("*type/quoted*"))
            .and_then(Value::as_str)
            == Some("nothing")
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use crate::{GraphNode, GraphNodeType, GraphRecord, Message, RecordType, Role, SourceRef};

    use super::*;

    #[test]
    fn parses_slash_path_prefix() {
        assert_eq!(
            parse_path_prefix("*state*/agent-recorder/log").unwrap(),
            vec!["*state*", "agent-recorder", "log"]
        );
    }

    #[test]
    fn builds_gateway_operation_urls() {
        assert_eq!(
            gateway_operation_url("http://localhost:8192", "set"),
            "http://localhost:8192/api/v1/general/set"
        );
        assert_eq!(
            gateway_operation_url("http://localhost:8192/api/v1/general", "get"),
            "http://localhost:8192/api/v1/general/get"
        );
    }

    #[test]
    fn gateway_adapter_writes_and_reads_indexed_records() -> Result<()> {
        let record = sample_record();
        let encoded = hex::encode(serde_json::to_vec(&record)?);
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = thread::spawn(move || -> Result<Vec<String>> {
            let mut requests = Vec::new();
            for response_body in [
                "true".to_string(),
                json!({"*type/byte-vector*": encoded}).to_string(),
            ] {
                let (mut stream, _) = listener.accept()?;
                let request = read_http_request(&mut stream)?;
                requests.push(request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream.write_all(response.as_bytes())?;
            }
            Ok(requests)
        });

        let mut config = SyncWebConfig::gateway(format!("http://{addr}"), "test-token");
        config.path_prefix = parse_path_prefix("*state*/agent-recorder/test")?;
        config.start_index = 5;
        let mut writer = SyncWebRecordAdapter::create(config.clone())?;
        writer.log(&record)?;

        let reader = SyncWebRecordReader::create(config)?;
        let mut records = Vec::new();
        reader.read(RecordSelector::Index(5), &mut |record| {
            records.push(record);
            Ok(())
        })?;

        let requests = server.join().expect("mock server panicked")?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].index, 5);
        assert_eq!(records[0].record.id(), "msg_sync_web_test");
        assert!(requests[0].starts_with("POST /api/v1/general/set HTTP/1.1"));
        assert!(requests[1].starts_with("POST /api/v1/general/get HTTP/1.1"));
        assert!(requests[0].contains("authorization: Bearer test-token"));
        assert!(requests[0]
            .contains("\"path\":[\"*state*\",\"agent-recorder\",\"test\",\"entry-000000000005\"]"));
        Ok(())
    }

    #[test]
    fn direct_journal_adapter_sends_journal_secret() -> Result<()> {
        let record = sample_record();
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = thread::spawn(move || -> Result<String> {
            let (mut stream, _) = listener.accept()?;
            let request = read_http_request(&mut stream)?;
            let response_body = "true";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes())?;
            Ok(request)
        });

        let mut config =
            SyncWebConfig::direct_journal(format!("http://{addr}/interface"), "journal-secret");
        config.path_prefix = parse_path_prefix("*state*/agent-recorder/direct")?;
        let mut writer = SyncWebRecordAdapter::create(config)?;
        writer.log(&record)?;

        let request = server.join().expect("mock server panicked")?;
        assert!(request.starts_with("POST /interface HTTP/1.1"));
        assert!(request.contains("\"function\":\"set!\""));
        assert!(request.contains("\"credentials\":{\"*type/string*\":\"journal-secret\"}"));
        assert!(request.contains(
            "\"path\":[\"*state*\",\"agent-recorder\",\"direct\",\"entry-000000000000\"]"
        ));
        Ok(())
    }

    fn sample_record() -> GraphRecord {
        GraphRecord {
            record_type: RecordType::AgentRecord,
            node: GraphNode::Message(Message {
                node_type: GraphNodeType::Message,
                id: "msg_sync_web_test".to_string(),
                timestamp: None,
                role: Role::User,
                content: "hello sync web".to_string(),
                cwd: None,
                provider: None,
                model: None,
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                source: SourceRef {
                    agent_adapter: "test".to_string(),
                    path: None,
                    locator: None,
                },
                metadata: json!({}),
            }),
            edges: Vec::new(),
            integrity: None,
        }
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Result<String> {
        let mut buffer = Vec::new();
        let mut temp = [0u8; 1024];
        loop {
            let read = stream.read(&mut temp)?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&temp[..read]);
            if let Some(header_end) = find_header_end(&buffer) {
                let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let total = header_end + 4 + content_length;
                while buffer.len() < total {
                    let read = stream.read(&mut temp)?;
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&temp[..read]);
                }
                break;
            }
        }
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
