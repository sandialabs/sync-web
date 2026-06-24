use clap::Parser;
use journal_sdk::JOURNAL;
use log::info;
use rand::{distributions::Alphanumeric, Rng};
use rocket::config::Config as RocketConfig;
use rocket::data::{Limits, ToByteUnit};
use rocket::response::content::{RawHtml, RawText};
use rocket::serde::json::Json;
use rocket::{get, post, routes};
use serde_json::Value;
use std::fs;
use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MICRO: f64 = 1_000_000.0;

const ROOT_SCM: &str = include_str!("../../../../records/lisp/root.scm");
const STANDARD_SCM: &str = include_str!("../../../../records/lisp/standard.scm");
const LOG_CHAIN_SCM: &str = include_str!("../../../../records/lisp/log-chain.scm");
const TREE_SCM: &str = include_str!("../../../../records/lisp/tree.scm");
const LEDGER_SCM: &str = include_str!("../../../../records/lisp/ledger.scm");
const DOCUMENT_SCM: &str = include_str!("../../../../records/lisp/document.scm");
const INTERFACE_SCM: &str = include_str!("../../../../records/lisp/interface.scm");

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
  <head><h2>Sync Web Ledger</h2></head>
  <body style="padding: 0 20px; font-family: 'Consolas'">
    <ul>
      <li><a href="/interface">Interface</a></li>
      <li><a href="/interface/scheme-to-json">Scheme to JSON</a></li>
      <li><a href="/interface/json-to-scheme">JSON to Scheme</a></li>
    </ul>
  </body>
</html>
"#;

const INTERFACE_HTML: &str = r#"<!DOCTYPE html>
<html>
  <head><h2>Sync Web Ledger Interface</h2></head>
  <body style="padding: 0 20px; font-family: 'Consolas'">
    <textarea id="query" rows="8" cols="128" spellcheck="false"></textarea>
    </br></br>
    <button type="button" onclick="customSubmit('application/scheme')">Scheme</button>
    <button type="button" onclick="customSubmit('application/json')">JSON</button>
    </br>
    <ul id="history"></ul>
    <script>
      function customSubmit(contentType) {
        let query = document.getElementById('query').value;
        fetch('', { method: 'POST', headers: { 'Content-Type': contentType }, body: query })
          .then(response => response.text())
          .catch(_ => "Error: uh oh, not sure what happened")
          .then(result => {
            let history = document.getElementById('history');
            let queryItem = document.createElement('li');
            let queryText = document.createElement('span');
            let resultItem = document.createElement('li');
            queryItem.style.listStyle = "'→  '";
            queryItem.style.color = 'green';
            queryText.style.color = 'gray';
            queryText.style.whiteSpace = 'pre-wrap';
            queryText.textContent = query.slice(0, 512) + (query.length > 512 ? ' ...' : '');
            queryItem.appendChild(queryText);
            resultItem.style.listStyle = "'   '";
            resultItem.style.whiteSpace = 'pre-wrap';
            resultItem.textContent = result;
            history.prepend(resultItem);
            history.prepend(queryItem);
          })
      }
    </script>
  </body>
</html>
"#;

fn ledger_version() -> &'static str {
    include_str!("../../../../VERSION").trim()
}

#[derive(Parser, Debug)]
#[command(
    author,
    name = "ledger",
    version = ledger_version(),
    about = "Run a lightweight local Sync Web ledger"
)]
struct Args {
    #[arg(short, long, default_value = "ledger-data/journal", help = "Path to the ledger journal database")]
    database: PathBuf,

    #[arg(short, long, default_value_t = 8192, help = "Port to access the webserver")]
    port: u16,

    #[arg(short = 'c', long, default_value_t = 2.0, help = "Number of seconds between ledger steps")]
    period: f64,

    #[arg(short, long, default_value = "", help = "Evaluate a ledger query and exit immediately")]
    evaluate: String,

    #[arg(long, default_value = "", help = "Root/interface secret; generated locally when omitted")]
    secret: String,

    #[arg(long, default_value_t = false, help = "Reinstall/update embedded records in an existing database")]
    update_records: bool,

    #[arg(long, default_value = "1024", help = "Ledger retention window")]
    window: String,

    #[arg(long, default_value = "", help = "Public interface URL advertised in ledger info")]
    interface: String,

    #[arg(long, default_value = "", help = "Journal name advertised in ledger info")]
    name: String,

    #[arg(long, default_value = "push", help = "Bridge publish policy: push|pull|none")]
    bridge_publish: String,

    #[arg(long, default_value = "pull", help = "Bridge subscribe policy: push|pull|none")]
    bridge_subscribe: String,
}

#[get("/")]
async fn index() -> RawHtml<String> {
    RawHtml(String::from(INDEX_HTML))
}

#[get("/interface", format = "text/html")]
async fn inform_interface() -> RawHtml<String> {
    RawHtml(String::from(INTERFACE_HTML))
}

#[post("/interface", format = "application/json", data = "<query>", rank = 1)]
async fn evaluate_interface_json(query: Json<Value>) -> Json<Value> {
    Json(JOURNAL.evaluate_json(query.into_inner()))
}

#[post("/interface", data = "<query>", rank = 2)]
async fn evaluate_interface_scheme(query: &str) -> String {
    JOURNAL.evaluate(query)
}

#[get("/interface/scheme-to-json", format = "text/html")]
async fn inform_scheme_to_json() -> RawHtml<String> {
    RawHtml(String::from(INTERFACE_HTML))
}

#[post("/interface/scheme-to-json", data = "<query>", rank = 1)]
async fn scheme_to_json(query: &str) -> Json<Value> {
    Json(JOURNAL.scheme_to_json(query))
}

#[get("/interface/json-to-scheme", format = "text/html")]
async fn inform_json_to_scheme() -> RawHtml<String> {
    RawHtml(String::from(INTERFACE_HTML))
}

#[post("/interface/json-to-scheme", data = "<query>", format = "json", rank = 1)]
async fn json_to_scheme(query: Json<Value>) -> RawText<String> {
    RawText(JOURNAL.json_to_scheme(query.into_inner()))
}

fn escape_scheme_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn quote_expr(expr: &str) -> String {
    format!("'{}", expr)
}

fn database_has_content(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.any(|entry| entry.is_ok()))
        .unwrap_or(false)
}

fn generate_secret() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn secret_path(database: &Path) -> PathBuf {
    database
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("ledger.secret")
}

fn resolve_secret(args: &Args) -> String {
    if !args.secret.is_empty() {
        return args.secret.clone();
    }

    let path = secret_path(&args.database);
    if let Ok(secret) = fs::read_to_string(&path) {
        return secret.trim().to_string();
    }

    let secret = generate_secret();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create ledger state directory");
    }
    fs::write(&path, format!("{secret}\n")).expect("failed to write generated ledger secret");
    eprintln!("Generated local ledger secret at {}", path.display());
    secret
}

fn install_expr(args: &Args, secret: &str, clear: bool) -> String {
    let interface = if args.interface.is_empty() {
        secret.to_string()
    } else {
        args.interface.clone()
    };
    let name = if args.name.is_empty() {
        interface.clone()
    } else {
        args.name.clone()
    };
    let clear_flag = if clear { "#t" } else { "#f" };
    let config = format!(
        "((clear? {clear_flag}) \
          (root-secret \"{}\") \
          (interface-secret \"{}\") \
          (admins ()) \
          (window {}) \
          (root {}) \
          (interface \"{}\") \
          (name \"{}\") \
          (push-enabled? #t) \
          (bridge-policy ((publish {}) (subscribe {}))))",
        escape_scheme_string(secret),
        escape_scheme_string(secret),
        args.window,
        ROOT_SCM,
        escape_scheme_string(&interface),
        escape_scheme_string(&name),
        args.bridge_publish,
        args.bridge_subscribe,
    );
    let expr = format!(
        "({interface_scm} {config} {standard} {chain} {tree} {ledger} {document})",
        interface_scm = INTERFACE_SCM,
        config = config,
        standard = quote_expr(STANDARD_SCM),
        chain = quote_expr(LOG_CHAIN_SCM),
        tree = quote_expr(TREE_SCM),
        ledger = quote_expr(LEDGER_SCM),
        document = quote_expr(DOCUMENT_SCM),
    );
    if clear {
        expr
    } else {
        format!("(*eval* \"{}\" {})", escape_scheme_string(secret), expr)
    }
}

fn install_or_update_records(args: &Args, secret: &str) {
    let has_content = database_has_content(&args.database);
    if !has_content {
        let result = JOURNAL.evaluate(&install_expr(args, secret, true));
        info!("Installed ledger records: {result}");
        if result.starts_with("(error ") {
            panic!("failed to install ledger records: {result}");
        }
    } else if args.update_records {
        let result = JOURNAL.evaluate(&install_expr(args, secret, false));
        info!("Updated ledger records: {result}");
        if result.starts_with("(error ") {
            panic!("failed to update ledger records: {result}");
        }
    }
}

#[rocket::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    let secret = resolve_secret(&args);
    if let Some(parent) = args.database.parent() {
        fs::create_dir_all(parent).expect("failed to create ledger database parent directory");
    }
    unsafe {
        std::env::set_var("SYNC_WEB_DATABASE", args.database.to_string_lossy().to_string());
    }

    install_or_update_records(&args, &secret);

    if !args.evaluate.is_empty() {
        println!("{}", JOURNAL.evaluate(&args.evaluate));
        return;
    }

    let step_secret = secret.clone();
    let period = args.period;
    tokio::spawn(async move {
        let mut step = 0;
        let start = ((SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get system time")
            .as_micros() as f64
            / (period * MICRO))
            .ceil()
            * (period * MICRO)) as u128;

        loop {
            let until = start + step * (period * MICRO) as u128;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("failed to get system time")
                .as_micros();
            if now < until {
                tokio::time::sleep(Duration::from_micros(
                    (until - now).try_into().expect("failed to convert duration"),
                ))
                .await;
            }
            let result = JOURNAL.evaluate(&format!("(*step* \"{}\")", escape_scheme_string(&step_secret)));
            info!("Step ({:.6}): {result}", until as f64 / MICRO);
            step += 1;
        }
    });

    let mut rocket_config = RocketConfig::default();
    rocket_config.port = args.port;
    rocket_config.address = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0));
    rocket_config.limits = Limits::new()
        .limit("string", 64_i32.mebibytes())
        .limit("json", 64_i32.mebibytes());

    let _ = rocket::build()
        .mount(
            "/",
            routes![
                index,
                inform_interface,
                evaluate_interface_json,
                evaluate_interface_scheme,
                inform_scheme_to_json,
                scheme_to_json,
                inform_json_to_scheme,
                json_to_scheme
            ],
        )
        .configure(rocket_config)
        .launch()
        .await;
}
