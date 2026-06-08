use journal_sdk::{Config, JOURNAL};
use log::info;
use rocket::config::Config as RocketConfig;
use rocket::data::{Limits, ToByteUnit};
use rocket::response::content::{RawHtml, RawText};
use rocket::serde::json::Json;
use rocket::{get, post, routes};
use serde_json::Value;
use std::io::{self, Read};
use std::net::{IpAddr, Ipv6Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MICRO: f64 = 1000000.0;

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
    <head>
 <h2>Journal SDK Home</h2>
    </head>
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
    <head>
 <h2>Journal Interface</h2>
    </head>
    <body style="padding: 0 20px; font-family: 'Consolas'">
 <textarea id="query" rows="8" cols="128" spellcheck="false"></textarea>
 </br>
 </br>
 <button type="button" onclick="customSubmit('application/scheme')">Scheme</button>
 <button type="button" onclick="customSubmit('application/json')">JSON</button>
 </br>
 <ul id="history">
 </ul>
 <script>
     function customSubmit(contentType) {
  let query = document.getElementById('query').value;
  fetch('', {
      method: 'POST',
      headers: { 'Content-Type': contentType },
      body: query,
  }).then(response => {
      return response.text();
  }).catch(error => {
      return "Error: uh oh, not sure what happened";
  }).then(result => {
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

#[rocket::main]
async fn main() {
    let config = Config::new();

    env_logger::init();

    if &config.boot != "" {
        let result = JOURNAL.evaluate(&config.boot);
        info!("Boot: {}", result);
    }

    if &config.evaluate != "" {
        let query = if &config.evaluate == "-" {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .expect("Failed to read query from stdin");
            buffer
        } else {
            config.evaluate.clone()
        };
        let result = JOURNAL.evaluate(&query);
        println!("{}", result);
        return;
    }

    let mut rocket_config = RocketConfig::default();
    rocket_config.port = config.port;
    rocket_config.address = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0));
    rocket_config.limits = Limits::new()
        .limit("string", 64_i32.mebibytes())
        .limit("json", 64_i32.mebibytes());

    if config.step != "" {
        tokio::spawn(async move {
            let mut step = 0;
            let start = ((SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get system time")
                .as_micros() as f64
                / (config.period * MICRO))
                .ceil()
                * (config.period * MICRO)) as u128;

            loop {
                let until = start + step * (config.period * MICRO) as u128;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Failed to get system time")
                    .as_micros();
                if now < until {
                    tokio::time::sleep(Duration::from_micros(
                        (until - now)
                            .try_into()
                            .expect("Failed to convert duration"),
                    ))
                    .await;
                }
                let result = JOURNAL.evaluate(&config.step);
                info!("Step ({:.6}): {}", until as f64 / MICRO, result);
                step += 1;
            }
        });
    }

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
