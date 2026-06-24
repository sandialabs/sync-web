use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use agent_recorder::{
    adapters, import_records,
    integrity::{
        integrity_status, rekey_state, verify_indexed_record, verify_indexed_records,
        verify_record, IntegrityAlignment, IntegrityKey, IntegrityRecordAdapter,
        VerificationStatus,
    },
    records,
    records::{RecordAdapter, RecordReader, RecordSelector},
    EdgeType, GraphNodeType, GraphRecord, ReadHint, Recorder,
};
use anyhow::Result;
use serde_json::Value;

#[derive(Default)]
struct MemoryRecordAdapter {
    records: Vec<GraphRecord>,
}

impl RecordAdapter for MemoryRecordAdapter {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn log(&mut self, record: &GraphRecord) -> Result<()> {
        self.records.push(record.clone());
        Ok(())
    }
}

struct CountingReader {
    records: Vec<GraphRecord>,
    reads: RefCell<Vec<u64>>,
}

impl RecordReader for CountingReader {
    fn name(&self) -> &'static str {
        "counting"
    }

    fn read(
        &self,
        selector: RecordSelector,
        emit: &mut dyn FnMut(agent_recorder::records::IndexedGraphRecord) -> Result<()>,
    ) -> Result<()> {
        for (index, record) in self.records.iter().enumerate() {
            let index = index as u64;
            if selector.contains(index) {
                self.reads.borrow_mut().push(index);
                emit(agent_recorder::records::IndexedGraphRecord {
                    index,
                    record: record.clone(),
                })?;
            }
        }
        Ok(())
    }
}

fn fixture_path(agent: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(agent)
}

fn import_fixture(agent: &str) -> Result<Vec<GraphRecord>> {
    import_fixture_as(agent, agent)
}

fn import_fixture_as(adapter_name: &str, fixture_name: &str) -> Result<Vec<GraphRecord>> {
    let adapter = adapters::by_name(adapter_name).expect("known adapter");
    let mut sink = MemoryRecordAdapter::default();
    import_records(adapter.as_ref(), &[fixture_path(fixture_name)], &mut sink)?;
    Ok(sink.records)
}

#[test]
fn imports_pi_fixture() -> Result<()> {
    assert_fixture("pi", 7, 6)
}

#[test]
fn imports_codex_fixture() -> Result<()> {
    assert_fixture("codex", 4, 3)
}

#[test]
fn imports_claude_fixture() -> Result<()> {
    assert_fixture("claude", 4, 3)
}

#[test]
fn imports_opencode_fixture() -> Result<()> {
    assert_fixture("opencode", 4, 3)
}

#[test]
fn imports_opencode_table_export_fixture() -> Result<()> {
    assert_opencode_export_fixture("opencode-export", None)
}

#[test]
fn imports_opencode_combined_export_fixture() -> Result<()> {
    assert_opencode_export_fixture("opencode-combined-export", Some("/tmp/opencode-combined"))
}

fn assert_opencode_export_fixture(fixture: &str, cwd: Option<&str>) -> Result<()> {
    let adapter = adapters::by_name("opencode").expect("known adapter");
    let mut sink = MemoryRecordAdapter::default();
    import_records(adapter.as_ref(), &[fixture_path(fixture)], &mut sink)?;

    assert_eq!(message_count(&sink.records), 4);
    assert_eq!(follows_edges(&sink.records), 3);
    assert!(sink.records.iter().any(|record| {
        serde_json::to_value(record)
            .ok()
            .and_then(|value| {
                value["node"]["tool-calls"]
                    .as_array()
                    .map(|calls| !calls.is_empty())
            })
            .unwrap_or(false)
    }));
    if let Some(cwd) = cwd {
        assert!(sink.records.iter().all(|record| {
            serde_json::to_value(record)
                .ok()
                .is_some_and(|value| value["node"]["cwd"] == cwd)
        }));
    }
    Ok(())
}

#[test]
fn imports_generated_synthetic_suite() -> Result<()> {
    let cases = [
        ("pi", "generated-pi", 83, 75, 39, 39),
        ("pi", "generated-pi-resume", 4, 3, 0, 0),
        ("codex", "generated-codex", 150, 142, 50, 50),
        ("claude", "generated-claude", 94, 86, 35, 35),
        ("gemini", "generated-gemini", 44, 40, 22, 22),
        ("opencode", "generated-opencode", 44, 36, 45, 0),
    ];

    for (adapter, fixture, expected_messages, expected_follows, expected_calls, expected_results) in
        cases
    {
        let records = import_fixture_as(adapter, fixture)?;
        assert_eq!(
            message_count(&records),
            expected_messages,
            "message count for {fixture}"
        );
        assert_eq!(
            follows_edges(&records),
            expected_follows,
            "follows-message count for {fixture}"
        );
        let values = records
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        let tool_calls = values
            .iter()
            .filter_map(|value| value["node"]["tool-calls"].as_array())
            .map(Vec::len)
            .sum::<usize>();
        let tool_results = values
            .iter()
            .filter_map(|value| value["node"]["tool-results"].as_array())
            .map(Vec::len)
            .sum::<usize>();
        assert_eq!(tool_calls, expected_calls, "tool calls for {fixture}");
        assert_eq!(tool_results, expected_results, "tool results for {fixture}");
    }
    Ok(())
}

#[test]
fn generated_synthetic_suite_normalizes_common_fields() -> Result<()> {
    for (adapter, fixture) in [
        ("pi", "generated-pi"),
        ("codex", "generated-codex"),
        ("claude", "generated-claude"),
        ("gemini", "generated-gemini"),
        ("opencode", "generated-opencode"),
    ] {
        let values = import_fixture_as(adapter, fixture)?
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?;
        if fixture != "generated-opencode" {
            assert!(
                values.iter().any(|value| {
                    value["node"]["cwd"]
                        .as_str()
                        .is_some_and(|cwd| cwd.starts_with("/tmp/agent-recorder-suite/"))
                }),
                "cwd for {fixture}"
            );
        }
        assert!(
            values
                .iter()
                .any(|value| value["node"].get("timestamp").is_some()),
            "timestamp for {fixture}"
        );
        if fixture != "generated-codex" {
            assert!(
                values
                    .iter()
                    .any(|value| value["node"].get("model").is_some()),
                "model for {fixture}"
            );
        }
        assert!(
            values.iter().all(|value| {
                value["node"]["metadata"].get("provider").is_none()
                    && value["node"]["metadata"].get("model").is_none()
            }),
            "provider/model metadata dedupe for {fixture}"
        );
    }

    let resume = import_fixture_as("pi", "generated-pi-resume")?
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    let content = resume
        .iter()
        .filter_map(|value| value["node"]["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains("blue-otter-17"));
    Ok(())
}

#[test]
fn generated_fixtures_are_sanitized() -> Result<()> {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let needles = [
        "tdinh",
        "Thien",
        "/home/tdinh",
        "projects/sync-web",
        "/srv/sync-web",
        ".codex/skills",
        "encrypted_content",
        "thinkingSignature",
        "api_key",
        "cookie",
    ];
    for dir in [
        "generated-pi",
        "generated-pi-resume",
        "generated-codex",
        "generated-claude",
        "generated-gemini",
        "generated-opencode",
    ] {
        for entry in walk_files(&fixture_root.join(dir))? {
            let text = fs::read_to_string(&entry)?;
            for needle in needles {
                assert!(
                    !text.contains(needle),
                    "found {needle:?} in {}",
                    entry.display()
                );
            }
        }
    }
    Ok(())
}

#[test]
fn graph_records_are_message_nodes_with_sequence_edges() -> Result<()> {
    let records = import_fixture("pi")?;
    let first = &records[0];
    let second = &records[1];

    let value = serde_json::to_value(first)?;
    assert_eq!(value["type"], "agent-record");
    assert_eq!(value["node"]["type"], "message");
    assert!(value.get("node").is_some());
    assert!(value["node"].get("content").is_some());
    assert!(value["node"].get("tool-calls").is_none());
    assert!(value["node"].get("tool_calls").is_none());

    assert_eq!(second.edges.len(), 1);
    assert_eq!(second.edges[0].edge_type, EdgeType::FollowsMessage);
    assert_eq!(second.edges[0].target, first.id());
    Ok(())
}

#[test]
fn fixtures_emit_normalized_timestamps() -> Result<()> {
    let pi = import_fixture("pi")?;
    let pi_values = pi
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(pi_values[0]["node"]["timestamp"], "2026-01-01T00:00:00Z");
    assert_eq!(pi_values[0]["node"]["cwd"], "/tmp/pi-fixture");
    assert_eq!(pi_values[1]["node"]["provider"], "openai");
    assert_eq!(pi_values[1]["node"]["model"], "gpt-test");
    assert!(pi_values[1]["node"]["metadata"].get("provider").is_none());
    assert!(pi_values[1]["node"]["metadata"].get("model").is_none());
    assert_eq!(pi_values[5]["node"]["timestamp"], "2026-01-01T00:00:04Z");

    let adapter = adapters::by_name("opencode").expect("known adapter");
    let mut sink = MemoryRecordAdapter::default();
    import_records(
        adapter.as_ref(),
        &[fixture_path("opencode-export")],
        &mut sink,
    )?;
    let first = serde_json::to_value(&sink.records[0])?;
    let second = serde_json::to_value(&sink.records[1])?;
    assert_eq!(first["node"]["timestamp"], "1970-01-01T00:16:40Z");
    assert_eq!(second["node"]["timestamp"], "1970-01-01T00:16:42Z");
    Ok(())
}

#[test]
fn pi_fixture_preserves_tool_and_compaction_edges() -> Result<()> {
    let records = import_fixture("pi")?;
    assert!(records.iter().any(|record| {
        let value = serde_json::to_value(record).ok();
        value
            .as_ref()
            .and_then(|value| value["node"]["tool-results"].as_array())
            .is_some_and(|results| !results.is_empty())
            && record
                .edges
                .iter()
                .any(|edge| edge.edge_type == EdgeType::FollowsMessage)
    }));
    assert!(records.iter().any(|record| {
        let value = serde_json::to_value(record).ok();
        value
            .as_ref()
            .and_then(|value| value["node"]["metadata"]["compaction"].as_bool())
            == Some(true)
            && record.edges.iter().any(|edge| {
                edge.edge_type == EdgeType::Summarizes && edge.target.starts_with("msg_")
            })
    }));
    Ok(())
}

#[test]
fn recorder_baseline_suppresses_existing_records() -> Result<()> {
    let adapter = adapters::by_name("pi").expect("known adapter");
    let roots = vec![fixture_path("pi")];
    let mut recorder = Recorder::new();

    let baseline = recorder.baseline(adapter.as_ref(), &roots)?;
    assert_eq!(baseline.records, 7);
    assert_eq!(baseline.duplicates, 0);

    let mut sink = MemoryRecordAdapter::default();
    let live = recorder.log_new(adapter.as_ref(), &roots, ReadHint::Full, &mut sink)?;
    assert_eq!(live.records, 0);
    assert_eq!(live.duplicates, 7);
    assert!(sink.records.is_empty());
    Ok(())
}

#[test]
fn cli_import_writes_existing_records_once() -> Result<()> {
    let out = std::env::temp_dir().join(format!(
        "agent-recorder-cli-import-{}.jsonl",
        std::process::id()
    ));
    let _ = fs::remove_file(&out);
    agent_recorder::cli::run_with_registry_from(
        adapters::AdapterRegistry::builtins(),
        [
            "agent-recorder".to_string(),
            "import".to_string(),
            "--agent".to_string(),
            "pi".to_string(),
            "--agent-data".to_string(),
            fixture_path("pi").display().to_string(),
            "--recorder".to_string(),
            "file".to_string(),
            "--recorder-data".to_string(),
            out.display().to_string(),
        ],
    )?;
    let records = read_jsonl_records(&out)?;
    assert_eq!(message_count(&records), 7);
    let _ = fs::remove_file(out);
    Ok(())
}

#[test]
fn jsonl_record_adapter_writes_flat_records() -> Result<()> {
    let adapter = adapters::by_name("pi").expect("known adapter");
    let out = std::env::temp_dir().join(format!(
        "agent-recorder-jsonl-test-{}.jsonl",
        std::process::id()
    ));

    {
        let mut sink = records::jsonl_file(&out)?;
        import_records(adapter.as_ref(), &[fixture_path("pi")], &mut sink)?;
    }

    let text = fs::read_to_string(&out)?;
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 7);
    for line in lines {
        let value: Value = serde_json::from_str(line)?;
        assert_eq!(value["type"], "agent-record");
        assert_eq!(value["node"]["type"], "message");
        assert!(value.get("node").is_some());
        assert!(value["node"]
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .starts_with("msg_"));
    }

    let _ = fs::remove_file(out);
    Ok(())
}

#[test]
fn jsonl_record_reader_reads_by_index_and_range() -> Result<()> {
    let adapter = adapters::by_name("pi").expect("known adapter");
    let out = std::env::temp_dir().join(format!(
        "agent-recorder-jsonl-read-test-{}.jsonl",
        std::process::id()
    ));

    {
        let mut sink = records::jsonl_file(&out)?;
        import_records(adapter.as_ref(), &[fixture_path("pi")], &mut sink)?;
    }

    let reader = records::jsonl_reader(&out)?;
    let mut single = Vec::new();
    reader.read(RecordSelector::Index(2), &mut |record| {
        single.push(record);
        Ok(())
    })?;
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].index, 2);
    assert!(single[0].record.id().starts_with("msg_"));

    let mut range = Vec::new();
    reader.read(RecordSelector::Range { start: 1, end: 4 }, &mut |record| {
        range.push(record);
        Ok(())
    })?;
    assert_eq!(
        range.iter().map(|record| record.index).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let _ = fs::remove_file(out);
    Ok(())
}

#[test]
fn integrity_wrapper_signs_jsonl_records_and_verifies_range() -> Result<()> {
    let adapter = adapters::by_name("pi").expect("known adapter");
    let out = std::env::temp_dir().join(format!(
        "agent-recorder-integrity-test-{}.jsonl",
        std::process::id()
    ));
    let state = std::env::temp_dir().join(format!(
        "agent-recorder-integrity-test-{}.state.json",
        std::process::id()
    ));
    let key = IntegrityKey::from_secret("test-integrity-secret");

    {
        let sink = records::jsonl_file(&out)?;
        let mut sink = IntegrityRecordAdapter::create(Box::new(sink), &state, Some(key.clone()))?;
        import_records(adapter.as_ref(), &[fixture_path("pi")], &mut sink)?;
    }

    let state_after_import = agent_recorder::integrity::load_state(&state)?;
    assert_eq!(state_after_import.next_index, 7);
    assert!(state_after_import
        .future_keys
        .iter()
        .all(|key| key.target >= 7));
    assert!(state_after_import
        .future_keys
        .iter()
        .all(|key| key.target != 0));

    let mut memory =
        IntegrityRecordAdapter::create(Box::new(MemoryRecordAdapter::default()), &state, None)?;
    memory.log(&import_fixture("pi")?[0])?;
    let state_after_keyless_append = agent_recorder::integrity::load_state(&state)?;
    assert_eq!(state_after_keyless_append.next_index, 8);
    assert!(state_after_keyless_append
        .future_keys
        .iter()
        .all(|key| key.target >= 8));

    let reader = records::jsonl_reader(&out)?;
    let mut checked = 0;
    reader.read(RecordSelector::Range { start: 0, end: 7 }, &mut |indexed| {
        let verification = verify_record(indexed.index, &indexed.record, &key)?;
        assert_eq!(verification.status, VerificationStatus::Verified);
        checked += 1;
        Ok(())
    })?;
    assert_eq!(checked, 7);

    let verified = verify_indexed_record(&reader, 6, &key)?;
    assert_eq!(verified.index, 6);

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    Ok(())
}

#[test]
fn integrity_status_reports_alignment() -> Result<()> {
    let (out, state, _key) = write_integrity_fixture("status")?;
    let reader = records::jsonl_reader(&out)?;
    let state_value = agent_recorder::integrity::load_state(&state)?;
    let status = integrity_status(&reader, &state_value)?;
    assert_eq!(status.state_next_index, 7);
    assert_eq!(status.backend_next_index, 7);
    assert_eq!(status.backend_latest_index, Some(6));
    assert_eq!(status.pending_key_count, state_value.future_keys.len());
    assert_eq!(status.alignment, IntegrityAlignment::Aligned);

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    Ok(())
}

#[test]
fn integrity_status_reports_one_step_repairable_without_mutating_state() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("status-repairable")?;
    let lagging_state = lagging_integrity_state("status-repairable-copy", &out, &key, 6)?;
    let before = fs::read_to_string(&lagging_state)?;
    let state_value = agent_recorder::integrity::load_state(&lagging_state)?;
    let reader = records::jsonl_reader(&out)?;
    let status = integrity_status(&reader, &state_value)?;
    let after = fs::read_to_string(&lagging_state)?;

    assert_eq!(status.backend_next_index, 7);
    assert_eq!(status.alignment, IntegrityAlignment::OneStepRepairable);
    assert_eq!(before, after);

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(lagging_state);
    Ok(())
}

#[test]
fn rekey_resets_state_at_current_index_without_marking_records() -> Result<()> {
    let (out, state, old_key) = write_integrity_fixture("rekey")?;
    let new_key = IntegrityKey::from_secret("new-integrity-secret");
    let reader = records::jsonl_reader(&out)?;
    let status = rekey_state(&reader, &state, &new_key)?;
    assert_eq!(status.state_next_index, 7);
    assert_eq!(status.alignment, IntegrityAlignment::Aligned);

    let rekeyed = agent_recorder::integrity::load_state(&state)?;
    assert_eq!(rekeyed.next_index, 7);
    assert_eq!(rekeyed.key_id, new_key.key_id());
    assert_eq!(rekeyed.future_keys.len(), 1);
    assert_eq!(rekeyed.future_keys[0].source, 7);
    assert_eq!(rekeyed.future_keys[0].target, 7);

    {
        let sink = records::jsonl_file(&out)?;
        let mut sink = IntegrityRecordAdapter::create_checked(
            Box::new(sink),
            &state,
            None,
            Some(&records::jsonl_reader(&out)?),
        )?;
        sink.log(&import_fixture("pi")?[0])?;
    }

    let reader = records::jsonl_reader(&out)?;
    assert!(verify_indexed_record(&reader, 7, &new_key).is_ok());
    assert!(verify_indexed_record(&reader, 7, &old_key).is_err());

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    Ok(())
}

#[test]
fn range_verification_reuses_fetched_records() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("range-cache")?;
    let records = read_jsonl_records(&out)?;
    let reader = CountingReader {
        records,
        reads: RefCell::new(Vec::new()),
    };

    let verified = verify_indexed_records(&reader, 0..7, &key)?;
    assert_eq!(verified.len(), 7);
    let reads = reader.reads.borrow();
    let unique = reads
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        reads.len(),
        unique.len(),
        "range verification reread indexes: {reads:?}"
    );
    assert!(reads.len() <= 7, "unexpected extra reads: {reads:?}");

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    Ok(())
}

#[test]
fn integrity_startup_repairs_one_written_record() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("repair")?;
    let repaired_state = lagging_integrity_state("repair-copy", &out, &key, 6)?;

    let reader = records::jsonl_reader(&out)?;
    let sink = MemoryRecordAdapter::default();
    let _adapter = IntegrityRecordAdapter::create_checked(
        Box::new(sink),
        &repaired_state,
        Some(key.clone()),
        Some(&reader),
    )?;
    let repaired = agent_recorder::integrity::load_state(&repaired_state)?;
    assert_eq!(repaired.next_index, 7);
    assert!(repaired.future_keys.iter().all(|future| future.target >= 7));

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(repaired_state);
    Ok(())
}

#[test]
fn integrity_startup_rejects_backend_more_than_one_ahead() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("backend-too-far")?;
    let lagging_state = lagging_integrity_state("backend-too-far-copy", &out, &key, 5)?;
    let reader = records::jsonl_reader(&out)?;
    let error = integrity_create_error(
        Box::new(MemoryRecordAdapter::default()),
        &lagging_state,
        Some(key),
        Some(&reader),
    );
    assert!(error
        .to_string()
        .contains("more than one record behind backend"));

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(lagging_state);
    Ok(())
}

#[test]
fn integrity_startup_rejects_corrupted_repair_record() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("corrupted-repair")?;
    let lagging_state = lagging_integrity_state("corrupted-repair-copy", &out, &key, 6)?;
    let mut records = read_jsonl_records(&out)?;
    if let agent_recorder::GraphNode::Message(message) = &mut records[6].node {
        message.content.push_str(" tampered");
    }
    let corrupted = out.with_extension("corrupted-repair.jsonl");
    write_jsonl_records(&corrupted, &records)?;

    let reader = records::jsonl_reader(&corrupted)?;
    let error = integrity_create_error(
        Box::new(MemoryRecordAdapter::default()),
        &lagging_state,
        Some(key),
        Some(&reader),
    );
    assert!(error
        .to_string()
        .contains("does not match recoverable integrity state"));

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(lagging_state);
    let _ = fs::remove_file(corrupted);
    Ok(())
}

#[test]
fn integrity_startup_rejects_missing_repair_key() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("missing-repair-key")?;
    let lagging_state = lagging_integrity_state("missing-repair-key-copy", &out, &key, 6)?;
    let mut state_value = agent_recorder::integrity::load_state(&lagging_state)?;
    state_value.future_keys.retain(|future| future.target != 6);
    fs::write(&lagging_state, serde_json::to_string_pretty(&state_value)?)?;

    let reader = records::jsonl_reader(&out)?;
    let error = integrity_create_error(
        Box::new(MemoryRecordAdapter::default()),
        &lagging_state,
        Some(key),
        Some(&reader),
    );
    assert!(error
        .to_string()
        .contains("integrity state is missing event key for index 6"));

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(lagging_state);
    Ok(())
}

#[test]
fn integrity_startup_rejects_state_ahead_of_backend() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("state-ahead")?;
    let truncated = out.with_extension("truncated.jsonl");
    let mut records = read_jsonl_records(&out)?;
    records.pop();
    write_jsonl_records(&truncated, &records)?;

    let reader = records::jsonl_reader(&truncated)?;
    let sink = MemoryRecordAdapter::default();
    assert!(IntegrityRecordAdapter::create_checked(
        Box::new(sink),
        &state,
        Some(key),
        Some(&reader),
    )
    .is_err());

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(truncated);
    Ok(())
}

#[test]
fn integrity_example_record_verifies() -> Result<()> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("integrity-agent-record.jsonld");
    let record = serde_json::from_str::<GraphRecord>(&fs::read_to_string(path)?)?;
    let key = IntegrityKey::from_secret("agent-recorder example integrity key");
    let verification = verify_record(0, &record, &key)?;
    assert_eq!(verification.status, VerificationStatus::Verified);
    Ok(())
}

#[test]
fn integrity_verification_rejects_tampered_record() -> Result<()> {
    let (out, state, key) = write_integrity_fixture("tamper")?;
    let mut records = read_jsonl_records(&out)?;

    records[3]
        .integrity
        .as_mut()
        .expect("integrity")
        .authenticator = "tampered".to_string();
    let tampered_authenticator = out.with_extension("tampered-authenticator.jsonl");
    write_jsonl_records(&tampered_authenticator, &records)?;
    let reader = records::jsonl_reader(&tampered_authenticator)?;
    assert!(verify_indexed_record(&reader, 3, &key).is_err());

    records = read_jsonl_records(&out)?;
    if let GraphNodeType::Message = records[3].node.node_type() {
        if let agent_recorder::GraphNode::Message(message) = &mut records[3].node {
            message.content.push_str(" tampered");
        }
    }
    let tampered_payload = out.with_extension("tampered-payload.jsonl");
    write_jsonl_records(&tampered_payload, &records)?;
    let reader = records::jsonl_reader(&tampered_payload)?;
    assert!(verify_indexed_record(&reader, 3, &key).is_err());

    let _ = fs::remove_file(out);
    let _ = fs::remove_file(state);
    let _ = fs::remove_file(tampered_authenticator);
    let _ = fs::remove_file(tampered_payload);
    Ok(())
}

fn integrity_create_error(
    sink: Box<dyn RecordAdapter>,
    state: &Path,
    key: Option<IntegrityKey>,
    reader: Option<&dyn RecordReader>,
) -> anyhow::Error {
    match IntegrityRecordAdapter::create_checked(sink, state, key, reader) {
        Ok(_) => panic!("expected integrity adapter creation to fail"),
        Err(error) => error,
    }
}

fn lagging_integrity_state(
    label: &str,
    source_records: &Path,
    key: &IntegrityKey,
    records_to_apply: usize,
) -> Result<PathBuf> {
    let records = read_jsonl_records(source_records)?;
    let state = std::env::temp_dir().join(format!(
        "agent-recorder-integrity-{label}-{}.state.json",
        std::process::id()
    ));
    let _ = fs::remove_file(&state);
    let mut sink = IntegrityRecordAdapter::create(
        Box::new(MemoryRecordAdapter::default()),
        &state,
        Some(key.clone()),
    )?;
    for record in records.iter().take(records_to_apply) {
        sink.log(record)?;
    }
    Ok(state)
}

fn write_integrity_fixture(label: &str) -> Result<(PathBuf, PathBuf, IntegrityKey)> {
    let adapter = adapters::by_name("pi").expect("known adapter");
    let out = std::env::temp_dir().join(format!(
        "agent-recorder-integrity-{label}-{}.jsonl",
        std::process::id()
    ));
    let state = std::env::temp_dir().join(format!(
        "agent-recorder-integrity-{label}-{}.state.json",
        std::process::id()
    ));
    let key = IntegrityKey::from_secret(format!("test-integrity-secret-{label}"));
    let _ = fs::remove_file(&out);
    let _ = fs::remove_file(&state);
    let sink = records::jsonl_file(&out)?;
    let mut sink = IntegrityRecordAdapter::create(Box::new(sink), &state, Some(key.clone()))?;
    import_records(adapter.as_ref(), &[fixture_path("pi")], &mut sink)?;
    Ok((out, state, key))
}

fn read_jsonl_records(path: &Path) -> Result<Vec<GraphRecord>> {
    let text = fs::read_to_string(path)?;
    text.lines()
        .map(|line| Ok(serde_json::from_str::<GraphRecord>(line)?))
        .collect()
}

fn write_jsonl_records(path: &Path, records: &[GraphRecord]) -> Result<()> {
    let mut text = String::new();
    for record in records {
        text.push_str(&serde_json::to_string(record)?);
        text.push('\n');
    }
    fs::write(path, text)?;
    Ok(())
}

fn assert_fixture(agent: &str, expected_messages: usize, expected_follows: usize) -> Result<()> {
    let records = import_fixture(agent)?;
    assert_eq!(
        message_count(&records),
        expected_messages,
        "message count for {agent}"
    );
    assert_eq!(
        follows_edges(&records),
        expected_follows,
        "follows-message count for {agent}"
    );
    assert!(records.iter().all(|record| !record.id().is_empty()));
    Ok(())
}

fn message_count(records: &[GraphRecord]) -> usize {
    records
        .iter()
        .filter(|record| record.node.node_type() == GraphNodeType::Message)
        .count()
}

fn follows_edges(records: &[GraphRecord]) -> usize {
    records
        .iter()
        .flat_map(|record| &record.edges)
        .filter(|edge| edge.edge_type == EdgeType::FollowsMessage)
        .count()
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(walk_files(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}
