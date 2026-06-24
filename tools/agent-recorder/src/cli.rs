use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;

use crate::adapters::AdapterRegistry;
use crate::integrity::{
    integrity_status, load_state, rekey_state, verify_indexed_records, IntegrityKey,
    IntegrityRecordAdapter, VerificationStatus, ALGORITHM,
};
use crate::records::{
    sync_web::{
        default_path_prefix, parse_path_prefix, secret_from_literal_or_env, SyncWebAuth,
        SyncWebConfig, SyncWebMode, SyncWebRecordAdapter, SyncWebRecordReader,
    },
    RecordAdapter, RecordReader, RecordRegistry, RecordSelector,
};
use crate::{import_records, AgentAdapter, GraphRecord, ImportReport, ReadHint};

#[derive(Debug, Parser)]
#[command(name = "agent-recorder")]
#[command(version)]
#[command(about = "Normalize AI-agent session artifacts into provenance records")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run(RunCommand),
    Import(ImportCommand),
    Read(ReadCommand),
    Rekey(RekeyCommand),
    Status(StatusCommand),
    Verify(VerifyCommand),
    Schema(SchemaCommand),
    Adapters(AdaptersCommand),
}

#[derive(Debug, clap::Args)]
struct RunCommand {
    #[command(flatten)]
    common: ImportLikeArgs,
    #[arg(
        long,
        default_value_t = 2000,
        help = "Polling interval for live recording, in milliseconds"
    )]
    poll_interval_ms: u64,
}

#[derive(Debug, clap::Args)]
struct ImportCommand {
    #[command(flatten)]
    common: ImportLikeArgs,
}

#[derive(Debug, clap::Args)]
struct ImportLikeArgs {
    #[arg(long, help = "Read defaults from this TOML config file")]
    config: Option<PathBuf>,
    #[command(flatten)]
    agent: AgentInputArgs,
    #[command(flatten)]
    output: RecordOutputArgs,
    #[command(flatten)]
    sync_web: SyncWebCliArgs,
    #[command(flatten)]
    integrity: IntegrityCliArgs,
}

#[derive(Debug, clap::Args)]
struct ReadCommand {
    #[command(flatten)]
    input: RecordInputArgs,
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(
        long,
        value_name = "KEY",
        help = "Verify HMAC integrity with this key before printing records"
    )]
    integrity_key: Option<String>,
    #[arg(
        long,
        value_name = "VAR",
        help = "Read HMAC integrity key from this environment variable before printing records"
    )]
    integrity_key_env: Option<String>,
}

#[derive(Debug, clap::Args)]
struct VerifyCommand {
    #[command(flatten)]
    input: RecordInputArgs,
    #[command(flatten)]
    selector: SelectorArgs,
    #[arg(long, default_value = ALGORITHM, hide = true)]
    integrity: String,
    #[arg(
        long,
        value_name = "VAR",
        help = "Read HMAC integrity key from this environment variable"
    )]
    integrity_key_env: Option<String>,
    #[arg(long, value_name = "KEY", help = "Verify HMAC integrity with this key")]
    integrity_key: Option<String>,
}

#[derive(Debug, clap::Args)]
struct RekeyCommand {
    #[command(flatten)]
    input: RecordInputArgs,
    #[command(flatten)]
    state: IntegrityStateArgs,
    #[arg(
        long,
        value_name = "VAR",
        help = "Read new integrity root key from this environment variable"
    )]
    integrity_key_env: Option<String>,
    #[arg(
        long,
        value_name = "KEY",
        help = "New integrity root key for future records"
    )]
    integrity_key: Option<String>,
}

#[derive(Debug, clap::Args)]
struct StatusCommand {
    #[command(flatten)]
    input: RecordInputArgs,
    #[command(flatten)]
    state: IntegrityStateArgs,
}

#[derive(Debug, clap::Args)]
struct SchemaCommand {
    #[arg(
        long,
        value_enum,
        default_value_t = SchemaFormat::Jsonld,
        help = "Schema format to print"
    )]
    format: SchemaFormat,
    #[arg(long, help = "Print the schema file path instead of file contents")]
    path: bool,
}

#[derive(Debug, clap::Args)]
struct AdaptersCommand {
    #[arg(long, help = "List agent adapters only")]
    agent: bool,
    #[arg(long, help = "List writable record adapters only")]
    recorder: bool,
    #[arg(long, help = "List readable record backends only")]
    reader: bool,
}

#[derive(Debug, clap::Args)]
struct AgentInputArgs {
    #[arg(
        long,
        help = "Source agent adapter name, e.g. pi, codex, claude, gemini, opencode"
    )]
    agent: Option<String>,
    #[arg(
        long = "agent-data",
        value_name = "AGENT_DATA",
        help = "Agent artifact file or directory to read"
    )]
    agent_data: Option<String>,
}

#[derive(Debug, clap::Args)]
struct RecordOutputArgs {
    #[arg(
        long,
        help = "Destination record adapter, e.g. file, sync-web, otel; omit to print records"
    )]
    recorder: Option<String>,
    #[arg(
        long = "recorder-data",
        value_name = "RECORDER_DATA",
        help = "Recorder backend path, endpoint, or URL for the selected recorder"
    )]
    recorder_data: Option<String>,
}

#[derive(Debug, clap::Args)]
struct RecordInputArgs {
    #[arg(
        long,
        default_value = "file",
        help = "Readable record backend, e.g. file or sync-web"
    )]
    recorder: String,
    #[arg(
        long = "recorder-data",
        value_name = "RECORDER_DATA",
        help = "Recorder backend path, endpoint, or URL to read"
    )]
    recorder_data: String,
    #[command(flatten)]
    sync_web: SyncWebCliArgs,
}

#[derive(Debug, clap::Args)]
struct SelectorArgs {
    #[arg(
        long,
        conflicts_with = "range",
        help = "Read or verify one absolute backend index"
    )]
    index: Option<u64>,
    #[arg(
        long,
        conflicts_with = "index",
        help = "Use a half-open START..END index range"
    )]
    range: Option<String>,
}

#[derive(Debug, Clone, clap::Args)]
struct SyncWebCliArgs {
    #[arg(long, help = "Sync Web access mode: gateway or direct-journal")]
    sync_web_mode: Option<String>,
    #[arg(long, value_name = "KEY", help = "Literal Sync Web gateway API key")]
    sync_web_api_key: Option<String>,
    #[arg(
        long,
        value_name = "VAR",
        help = "Environment variable containing the Sync Web gateway API key"
    )]
    sync_web_api_key_env: Option<String>,
    #[arg(long, value_name = "SECRET", help = "Literal direct-journal secret")]
    sync_web_journal_secret: Option<String>,
    #[arg(
        long,
        value_name = "VAR",
        help = "Environment variable containing the direct-journal secret"
    )]
    sync_web_journal_secret_env: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Slash-separated Sync Web path prefix for entry-* records"
    )]
    sync_web_path: Option<String>,
    #[arg(
        long,
        help = "First backend index for Sync Web writes; ignored when reading"
    )]
    sync_web_start_index: Option<u64>,
}

#[derive(Debug, Clone, clap::Args)]
struct IntegrityCliArgs {
    #[arg(
        long,
        value_name = "ALGORITHM",
        help = "Enable write-time integrity with this algorithm identifier, normally agent-recorder-integrity-v1"
    )]
    integrity: Option<String>,
    #[command(flatten)]
    state: OptionalIntegrityStateArgs,
    #[arg(
        long,
        value_name = "VAR",
        help = "Read integrity root key from this environment variable"
    )]
    integrity_key_env: Option<String>,
    #[arg(
        long,
        value_name = "KEY",
        help = "Integrity root key used to initialize or verify local state"
    )]
    integrity_key: Option<String>,
}

#[derive(Debug, Clone, clap::Args)]
struct OptionalIntegrityStateArgs {
    #[arg(long, help = "Private local integrity state file")]
    integrity_state: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct IntegrityStateArgs {
    #[arg(long, help = "Private local integrity state file")]
    integrity_state: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SchemaFormat {
    Jsonld,
    Turtle,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    #[serde(default)]
    run: RunConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RunConfig {
    agent: Option<String>,
    agent_data: Option<String>,
    recorder: Option<String>,
    recorder_data: Option<String>,
    sync_web_mode: Option<String>,
    sync_web_api_key: Option<String>,
    sync_web_api_key_env: Option<String>,
    sync_web_journal_secret: Option<String>,
    sync_web_journal_secret_env: Option<String>,
    sync_web_path: Option<String>,
    sync_web_start_index: Option<u64>,
    integrity: Option<String>,
    integrity_state: Option<PathBuf>,
    integrity_key_env: Option<String>,
    integrity_key: Option<String>,
    poll_interval_ms: Option<u64>,
}

#[derive(Debug)]
struct RunArgs {
    agent: String,
    agent_data: String,
    recorder: Option<String>,
    recorder_data: Option<String>,
    sync_web: SyncWebArgs,
    integrity: IntegrityArgs,
    poll_interval_ms: u64,
}

#[derive(Debug, Clone, Default)]
struct SyncWebArgs {
    mode: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    journal_secret: Option<String>,
    journal_secret_env: Option<String>,
    path: Option<String>,
    start_index: Option<u64>,
}

#[derive(Debug, Clone, Default)]
struct IntegrityArgs {
    algorithm: Option<String>,
    state: Option<PathBuf>,
    key_env: Option<String>,
    key: Option<String>,
}

impl From<SyncWebCliArgs> for SyncWebArgs {
    fn from(args: SyncWebCliArgs) -> Self {
        Self {
            mode: args.sync_web_mode,
            api_key: args.sync_web_api_key,
            api_key_env: args.sync_web_api_key_env,
            journal_secret: args.sync_web_journal_secret,
            journal_secret_env: args.sync_web_journal_secret_env,
            path: args.sync_web_path,
            start_index: args.sync_web_start_index,
        }
    }
}

impl From<IntegrityCliArgs> for IntegrityArgs {
    fn from(args: IntegrityCliArgs) -> Self {
        Self {
            algorithm: args.integrity,
            state: args.state.integrity_state,
            key_env: args.integrity_key_env,
            key: args.integrity_key,
        }
    }
}

pub fn run_with_registry(adapter_registry: AdapterRegistry) -> Result<()> {
    run_with_registries(adapter_registry, RecordRegistry::builtins())
}

pub fn run_with_registries(
    adapter_registry: AdapterRegistry,
    record_registry: RecordRegistry,
) -> Result<()> {
    run_with_registries_from(adapter_registry, record_registry, std::env::args_os())
}

pub fn run_with_registry_from<I, T>(adapter_registry: AdapterRegistry, args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    run_with_registries_from(adapter_registry, RecordRegistry::builtins(), args)
}

pub fn run_with_registries_from<I, T>(
    adapter_registry: AdapterRegistry,
    record_registry: RecordRegistry,
    args: I,
) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::Run(command) => cmd_run(command, &adapter_registry, &record_registry),
        Command::Import(command) => cmd_import(command, &adapter_registry, &record_registry),
        Command::Read(command) => cmd_read(command, &record_registry),
        Command::Rekey(command) => cmd_rekey(command, &record_registry),
        Command::Status(command) => cmd_status(command, &record_registry),
        Command::Verify(command) => cmd_verify(command, &record_registry),
        Command::Schema(command) => cmd_schema(command),
        Command::Adapters(command) => cmd_adapters(command, &adapter_registry, &record_registry),
    }
}

fn cmd_run(
    command: RunCommand,
    adapter_registry: &AdapterRegistry,
    record_registry: &RecordRegistry,
) -> Result<()> {
    let args = resolve_import_like_args(&command.common, Some(command.poll_interval_ms))?;
    let adapter = adapter_registry
        .get(&args.agent)
        .with_context(|| format!("unknown agent adapter: {}", args.agent))?;
    let mut sink = create_record_adapter(
        record_registry,
        args.recorder.as_deref(),
        args.recorder_data.as_deref(),
        &args.sync_web,
        &args.integrity,
    )?;
    let roots = vec![PathBuf::from(args.agent_data)];
    let mut seen = HashSet::new();
    let baseline = baseline_records(adapter.as_ref(), &roots, &mut seen)?;
    eprintln!(
        "baseline observed {} records ({} duplicates); recording new records only",
        baseline.records, baseline.duplicates
    );
    let poll_interval = Duration::from_millis(args.poll_interval_ms);
    loop {
        let report = if let Some(sink) = sink.as_mut() {
            log_new_records(
                adapter.as_ref(),
                &roots,
                crate::ReadHint::Full,
                &mut seen,
                |record| sink.log(record),
            )?
        } else {
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            log_new_records(
                adapter.as_ref(),
                &roots,
                crate::ReadHint::Full,
                &mut seen,
                |record| write_graph_record(&mut stdout, record),
            )?
        };
        if report.records > 0 {
            eprintln!(
                "logged {} new records ({} already seen)",
                report.records, report.duplicates
            );
        }
        thread::sleep(poll_interval);
    }
}

fn cmd_import(
    command: ImportCommand,
    adapter_registry: &AdapterRegistry,
    record_registry: &RecordRegistry,
) -> Result<()> {
    let args = resolve_import_like_args(&command.common, None)?;
    let adapter = adapter_registry
        .get(&args.agent)
        .with_context(|| format!("unknown agent adapter: {}", args.agent))?;
    let mut sink = create_record_adapter(
        record_registry,
        args.recorder.as_deref(),
        args.recorder_data.as_deref(),
        &args.sync_web,
        &args.integrity,
    )?;
    let roots = vec![PathBuf::from(args.agent_data)];
    let report = if let Some(sink) = sink.as_mut() {
        import_records(adapter.as_ref(), &roots, sink.as_mut())?
    } else {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let mut seen = HashSet::new();
        log_new_records(
            adapter.as_ref(),
            &roots,
            crate::ReadHint::Full,
            &mut seen,
            |record| write_graph_record(&mut stdout, record),
        )?
    };
    eprintln!(
        "imported {} records ({} duplicates)",
        report.records, report.duplicates
    );
    Ok(())
}

fn cmd_read(command: ReadCommand, record_registry: &RecordRegistry) -> Result<()> {
    let selector = resolve_selector(command.selector)?;
    let sync_web = SyncWebArgs::from(command.input.sync_web);
    let reader = create_record_reader(
        record_registry,
        &command.input.recorder,
        &command.input.recorder_data,
        &sync_web,
    )?;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    if command.integrity_key.is_some() || command.integrity_key_env.is_some() {
        let key = resolve_integrity_key(&IntegrityArgs {
            algorithm: Some(ALGORITHM.to_string()),
            state: None,
            key_env: command.integrity_key_env,
            key: command.integrity_key,
        })?;
        let verified = verify_indexed_records(reader.as_ref(), selector_indices(selector)?, &key)?;
        for indexed in verified {
            serde_json::to_writer(&mut stdout, &indexed)?;
            stdout.write_all(b"\n")?;
        }
        return Ok(());
    }

    reader.read(selector, &mut |record| {
        serde_json::to_writer(&mut stdout, &record)?;
        stdout.write_all(b"\n")?;
        Ok(())
    })?;
    Ok(())
}

fn cmd_rekey(command: RekeyCommand, record_registry: &RecordRegistry) -> Result<()> {
    let sync_web = SyncWebArgs::from(command.input.sync_web);
    let reader = create_record_reader(
        record_registry,
        &command.input.recorder,
        &command.input.recorder_data,
        &sync_web,
    )?;
    let key = resolve_integrity_key(&IntegrityArgs {
        algorithm: Some(ALGORITHM.to_string()),
        state: Some(command.state.integrity_state.clone()),
        key_env: command.integrity_key_env,
        key: command.integrity_key,
    })?;
    let status = rekey_state(reader.as_ref(), &command.state.integrity_state, &key)?;
    write_pretty_json(&status)
}

fn cmd_status(command: StatusCommand, record_registry: &RecordRegistry) -> Result<()> {
    let sync_web = SyncWebArgs::from(command.input.sync_web);
    let reader = create_record_reader(
        record_registry,
        &command.input.recorder,
        &command.input.recorder_data,
        &sync_web,
    )?;
    let state = load_state(&command.state.integrity_state)?;
    let status = integrity_status(reader.as_ref(), &state)?;
    write_pretty_json(&status)
}

fn cmd_verify(command: VerifyCommand, record_registry: &RecordRegistry) -> Result<()> {
    if command.integrity != ALGORITHM {
        bail!("unsupported integrity algorithm: {}", command.integrity);
    }
    let selector = resolve_selector(command.selector)?;
    let sync_web = SyncWebArgs::from(command.input.sync_web);
    let reader = create_record_reader(
        record_registry,
        &command.input.recorder,
        &command.input.recorder_data,
        &sync_web,
    )?;
    let key = resolve_integrity_key(&IntegrityArgs {
        algorithm: Some(command.integrity),
        state: None,
        key_env: command.integrity_key_env,
        key: command.integrity_key,
    })?;
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let verified = verify_indexed_records(reader.as_ref(), selector_indices(selector)?, &key)?;
    for indexed in verified {
        let verification = crate::integrity::VerificationResult {
            index: indexed.index,
            status: VerificationStatus::Verified,
            message: None,
        };
        serde_json::to_writer(&mut stdout, &verification)?;
        stdout.write_all(b"\n")?;
    }
    Ok(())
}

fn cmd_schema(command: SchemaCommand) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    if command.path {
        stdout.write_all(schema_path(command.format).as_bytes())?;
        stdout.write_all(b"\n")?;
    } else {
        let contents = schema_contents(command.format);
        stdout.write_all(contents.as_bytes())?;
        if !contents.ends_with('\n') {
            stdout.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn cmd_adapters(
    command: AdaptersCommand,
    adapter_registry: &AdapterRegistry,
    record_registry: &RecordRegistry,
) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let any_filter = command.agent || command.recorder || command.reader;
    let list_agents = command.agent || !any_filter;
    let list_recorders = command.recorder || !any_filter;
    let list_readers = command.reader || !any_filter;
    if list_agents {
        for name in adapter_registry.names() {
            writeln!(stdout, "agent\t{name}")?;
        }
    }
    if list_recorders {
        for name in record_registry.names() {
            writeln!(stdout, "recorder\t{name}")?;
        }
    }
    if list_readers {
        for name in record_registry.reader_names() {
            writeln!(stdout, "reader\t{name}")?;
        }
    }
    Ok(())
}

fn resolve_import_like_args(
    command: &ImportLikeArgs,
    poll_interval_ms: Option<u64>,
) -> Result<RunArgs> {
    resolve_run_args(
        command.config.as_deref(),
        command.agent.agent.clone(),
        command.agent.agent_data.clone(),
        command.output.recorder.clone(),
        command.output.recorder_data.clone(),
        SyncWebArgs::from(command.sync_web.clone()),
        IntegrityArgs::from(command.integrity.clone()),
        poll_interval_ms,
    )
}

fn resolve_selector(args: SelectorArgs) -> Result<RecordSelector> {
    resolve_read_selector(args.index, args.range)
}

fn write_pretty_json(value: &impl serde::Serialize) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

const JSONLD_SCHEMA_RELATIVE_PATH: &str = "schema/agent-recorder.context.jsonld";
const TURTLE_SCHEMA_RELATIVE_PATH: &str = "schema/agent-recorder.ttl";

fn schema_path(format: SchemaFormat) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}/{}", schema_relative_path(format))
}

fn schema_relative_path(format: SchemaFormat) -> &'static str {
    match format {
        SchemaFormat::Jsonld => JSONLD_SCHEMA_RELATIVE_PATH,
        SchemaFormat::Turtle => TURTLE_SCHEMA_RELATIVE_PATH,
    }
}

fn schema_contents(format: SchemaFormat) -> &'static str {
    match format {
        SchemaFormat::Jsonld => include_str!("../schema/agent-recorder.context.jsonld"),
        SchemaFormat::Turtle => include_str!("../schema/agent-recorder.ttl"),
    }
}

fn baseline_records(
    adapter: &dyn AgentAdapter,
    roots: &[PathBuf],
    seen: &mut HashSet<String>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    adapter.read(roots, ReadHint::Full, &mut |record| {
        let record = ensure_stable_id(record)?;
        if seen.insert(record.id().to_string()) {
            report.records += 1;
        } else {
            report.duplicates += 1;
        }
        Ok(())
    })?;
    Ok(report)
}

fn log_new_records(
    adapter: &dyn AgentAdapter,
    roots: &[PathBuf],
    hint: ReadHint,
    seen: &mut HashSet<String>,
    mut emit: impl FnMut(&GraphRecord) -> Result<()>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    adapter.read(roots, hint, &mut |record| {
        let record = ensure_stable_id(record)?;
        if seen.insert(record.id().to_string()) {
            emit(&record)?;
            report.records += 1;
        } else {
            report.duplicates += 1;
        }
        Ok(())
    })?;
    Ok(report)
}

fn ensure_stable_id(record: GraphRecord) -> Result<GraphRecord> {
    if record.id().is_empty() {
        record.with_stable_id()
    } else {
        Ok(record)
    }
}

fn write_graph_record(writer: &mut dyn Write, record: &GraphRecord) -> Result<()> {
    serde_json::to_writer(&mut *writer, record)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn create_record_adapter(
    registry: &RecordRegistry,
    recorder: Option<&str>,
    recorder_data: Option<&str>,
    sync_web: &SyncWebArgs,
    integrity: &IntegrityArgs,
) -> Result<Option<Box<dyn RecordAdapter>>> {
    let Some(recorder) = recorder else {
        if integrity.algorithm.is_some() {
            bail!("integrity logging requires --recorder and --recorder-data");
        }
        if recorder_data.is_some() {
            bail!("--recorder-data requires --recorder");
        }
        return Ok(None);
    };
    let recorder_data = recorder_data
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("--recorder {recorder} requires --recorder-data"))?;

    let sink: Box<dyn RecordAdapter> = if recorder == "sync-web" {
        Box::new(SyncWebRecordAdapter::create(resolve_sync_web_config(
            recorder_data,
            sync_web,
        )?)?)
    } else {
        registry
            .create(recorder, recorder_data)
            .with_context(|| format!("unknown recorder adapter: {recorder}"))??
    };

    if integrity.algorithm.is_none() {
        return Ok(Some(sink));
    }
    if integrity.algorithm.as_deref() != Some(ALGORITHM) {
        bail!(
            "unsupported integrity algorithm: {}",
            integrity.algorithm.as_deref().unwrap_or_default()
        );
    }
    let state = integrity
        .state
        .as_ref()
        .ok_or_else(|| anyhow!("integrity logging requires --integrity-state PATH"))?;
    let init_key = resolve_optional_integrity_key(integrity)?;
    let reader = create_record_reader(registry, recorder, recorder_data, sync_web)?;
    Ok(Some(Box::new(IntegrityRecordAdapter::create_checked(
        sink,
        state,
        init_key,
        Some(reader.as_ref()),
    )?)))
}

fn resolve_optional_integrity_key(args: &IntegrityArgs) -> Result<Option<IntegrityKey>> {
    match (&args.key, &args.key_env) {
        (None, None) => Ok(None),
        _ => resolve_integrity_key(args).map(Some),
    }
}

fn resolve_integrity_key(args: &IntegrityArgs) -> Result<IntegrityKey> {
    let secret = match (&args.key, &args.key_env) {
        (Some(_), Some(_)) => {
            bail!("provide either --integrity-key or --integrity-key-env, not both")
        }
        (Some(value), None) => value.clone(),
        (None, Some(name)) => {
            std::env::var(name).with_context(|| format!("reading integrity key from ${name}"))?
        }
        (None, None) => bail!("integrity requires --integrity-key-env or --integrity-key"),
    };
    Ok(IntegrityKey::from_secret(secret))
}

fn create_record_reader(
    registry: &RecordRegistry,
    recorder: &str,
    input: &str,
    sync_web: &SyncWebArgs,
) -> Result<Box<dyn RecordReader>> {
    if recorder == "sync-web" {
        return Ok(Box::new(SyncWebRecordReader::create(
            resolve_sync_web_config(input, sync_web)?,
        )?));
    }
    registry
        .create_reader(recorder, input)
        .with_context(|| format!("unknown readable recorder adapter: {recorder}"))?
}

fn resolve_sync_web_config(endpoint: &str, args: &SyncWebArgs) -> Result<SyncWebConfig> {
    let mode = match args.mode.as_deref().unwrap_or("gateway") {
        "gateway" => SyncWebMode::Gateway,
        "direct" | "direct-journal" | "journal" => SyncWebMode::DirectJournal,
        other => bail!("unknown Sync Web mode: {other}"),
    };
    let path_prefix = match &args.path {
        Some(path) => parse_path_prefix(path)?,
        None => default_path_prefix(),
    };
    let auth = match mode {
        SyncWebMode::Gateway => {
            let token = secret_from_literal_or_env(args.api_key.clone(), args.api_key_env.clone())?
                .ok_or_else(|| anyhow!("sync-web gateway mode requires --sync-web-api-key or --sync-web-api-key-env"))?;
            SyncWebAuth::GatewayApiKey(token)
        }
        SyncWebMode::DirectJournal => {
            let secret = secret_from_literal_or_env(
                args.journal_secret.clone(),
                args.journal_secret_env.clone(),
            )?
            .ok_or_else(|| anyhow!("sync-web direct-journal mode requires --sync-web-journal-secret or --sync-web-journal-secret-env"))?;
            SyncWebAuth::JournalSecret(secret)
        }
    };
    Ok(SyncWebConfig {
        endpoint: endpoint.to_string(),
        mode,
        auth,
        path_prefix,
        start_index: args.start_index.unwrap_or(0),
    })
}

fn resolve_read_selector(index: Option<u64>, range: Option<String>) -> Result<RecordSelector> {
    match (index, range) {
        (Some(index), None) => Ok(RecordSelector::Index(index)),
        (None, Some(range)) => parse_range(&range),
        (None, None) => Err(anyhow!(
            "read requires either --index N or --range START..END"
        )),
        (Some(_), Some(_)) => Err(anyhow!("read accepts only one of --index or --range")),
    }
}

fn selector_indices(selector: RecordSelector) -> Result<Vec<u64>> {
    match selector {
        RecordSelector::Index(index) => Ok(vec![index]),
        RecordSelector::Range { start, end } => Ok((start..end).collect()),
    }
}

fn parse_range(range: &str) -> Result<RecordSelector> {
    let (start, end) = range
        .split_once("..")
        .ok_or_else(|| anyhow!("range must use START..END syntax"))?;
    if start.is_empty() || end.is_empty() {
        return Err(anyhow!("range must include both START and END"));
    }
    let start = start
        .parse::<u64>()
        .with_context(|| format!("invalid range start: {start}"))?;
    let end = end
        .parse::<u64>()
        .with_context(|| format!("invalid range end: {end}"))?;
    if start >= end {
        return Err(anyhow!("range start must be less than range end"));
    }
    Ok(RecordSelector::Range { start, end })
}

fn resolve_run_args(
    config_path: Option<&Path>,
    agent: Option<String>,
    agent_data: Option<String>,
    recorder: Option<String>,
    recorder_data: Option<String>,
    sync_web: SyncWebArgs,
    integrity: IntegrityArgs,
    poll_interval_ms: Option<u64>,
) -> Result<RunArgs> {
    let config = load_config(config_path)?;
    let run = config.run;
    let poll_interval_ms = poll_interval_ms.or(run.poll_interval_ms).unwrap_or(2000);
    if poll_interval_ms == 0 {
        bail!("poll interval must be greater than zero");
    }

    Ok(RunArgs {
        agent: required("agent", agent.or(run.agent))?,
        agent_data: required("agent-data", agent_data.or(run.agent_data))?,
        recorder: recorder.or(run.recorder),
        recorder_data: recorder_data.or(run.recorder_data),
        sync_web: SyncWebArgs {
            mode: sync_web.mode.or(run.sync_web_mode),
            api_key: sync_web.api_key.or(run.sync_web_api_key),
            api_key_env: sync_web.api_key_env.or(run.sync_web_api_key_env),
            journal_secret: sync_web.journal_secret.or(run.sync_web_journal_secret),
            journal_secret_env: sync_web
                .journal_secret_env
                .or(run.sync_web_journal_secret_env),
            path: sync_web.path.or(run.sync_web_path),
            start_index: sync_web.start_index.or(run.sync_web_start_index),
        },
        integrity: IntegrityArgs {
            algorithm: integrity.algorithm.or(run.integrity),
            state: integrity.state.or(run.integrity_state),
            key_env: integrity.key_env.or(run.integrity_key_env),
            key: integrity.key.or(run.integrity_key),
        },
        poll_interval_ms,
    })
}

fn load_config(config_path: Option<&Path>) -> Result<Config> {
    if let Some(path) = config_path {
        return read_config(path);
    }
    let Some(path) = default_config_path() else {
        return Ok(Config::default());
    };
    read_config(&path)
}

fn read_config(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn default_config_path() -> Option<PathBuf> {
    let path = PathBuf::from("agent-recorder.toml");
    path.exists().then_some(path)
}

fn required(name: &str, value: Option<String>) -> Result<String> {
    value.filter(|value| !value.is_empty()).ok_or_else(|| {
        anyhow!(
            "missing required run option `{name}`; set it in agent-recorder.toml or pass --{name}"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_intended_subcommands() {
        use clap::CommandFactory;

        let mut names = Cli::command()
            .get_subcommands()
            .map(|command| command.get_name().to_string())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(
            names,
            vec!["adapters", "import", "read", "rekey", "run", "schema", "status", "verify"]
        );
    }

    #[test]
    fn schema_helpers_return_packaged_files() {
        assert!(schema_path(SchemaFormat::Jsonld).ends_with("agent-recorder.context.jsonld"));
        assert!(schema_path(SchemaFormat::Turtle).ends_with("agent-recorder.ttl"));
        assert!(schema_contents(SchemaFormat::Jsonld).contains("agent-record"));
        assert!(schema_contents(SchemaFormat::Turtle).contains("ar:AgentRecord"));
    }

    #[test]
    fn run_args_merge_config_and_cli() {
        let path = std::env::temp_dir().join(format!(
            "agent-recorder-config-test-{}.toml",
            std::process::id()
        ));
        fs::write(
            &path,
            r#"
[run]
agent = "pi"
agent-data = "tests/fixtures/pi"
recorder = "file"
recorder-data = "records.jsonl"
poll-interval-ms = 500
"#,
        )
        .unwrap();

        let args = resolve_run_args(
            Some(&path),
            Some("codex".to_string()),
            None,
            None,
            Some("override.jsonl".to_string()),
            SyncWebArgs::default(),
            IntegrityArgs::default(),
            None,
        )
        .unwrap();

        assert_eq!(args.agent, "codex");
        assert_eq!(args.agent_data, "tests/fixtures/pi");
        assert_eq!(args.recorder.as_deref(), Some("file"));
        assert_eq!(args.recorder_data.as_deref(), Some("override.jsonl"));
        assert_eq!(args.poll_interval_ms, 500);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn run_args_reject_zero_poll_interval() {
        let error = resolve_run_args(
            None,
            Some("pi".to_string()),
            Some("tests/fixtures/pi".to_string()),
            Some("file".to_string()),
            Some("records.jsonl".to_string()),
            SyncWebArgs::default(),
            IntegrityArgs::default(),
            Some(0),
        )
        .unwrap_err();
        assert!(error.to_string().contains("poll interval"));
    }

    #[test]
    fn read_selector_accepts_index() {
        assert_eq!(
            resolve_read_selector(Some(7), None).unwrap(),
            RecordSelector::Index(7)
        );
    }

    #[test]
    fn read_selector_accepts_half_open_range() {
        assert_eq!(
            resolve_read_selector(None, Some("3..9".to_string())).unwrap(),
            RecordSelector::Range { start: 3, end: 9 }
        );
    }

    #[test]
    fn read_selector_rejects_empty_or_descending_range() {
        assert!(resolve_read_selector(None, Some("3..3".to_string())).is_err());
        assert!(resolve_read_selector(None, Some("9..3".to_string())).is_err());
        assert!(resolve_read_selector(None, Some("3..".to_string())).is_err());
    }
}
