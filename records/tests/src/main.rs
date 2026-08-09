use journal_sdk::JOURNAL;
use journal_sdk::evaluator::{
    Evaluator, Primitive, obj2str, s7_boolean, s7_caddr, s7_cadr, s7_car, s7_cdr,
    s7_close_input_port, s7_error, s7_integer, s7_is_boolean, s7_is_integer, s7_is_null,
    s7_is_proper_list, s7_is_string, s7_list, s7_list_length, s7_make_integer, s7_make_string,
    s7_make_symbol, s7_open_input_string, s7_pointer, s7_read, s7_scheme, s7_string,
};
use journal_sdk::test_support::driver_primitives;
use scenario::{Delivery, ScenarioAction, ScenarioEvent, ScenarioWorld, run_scenario};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::ffi::{CStr, CString};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

mod scenario;

static TRACE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static WORLD: OnceLock<Mutex<ScenarioWorld>> = OnceLock::new();

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuiteManifest {
    #[serde(rename = "case")]
    cases: Vec<SuiteCase>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SuiteKind {
    Unit,
    Interface,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SuiteCase {
    name: String,
    kind: SuiteKind,
    test: PathBuf,
    #[serde(default)]
    arguments: Vec<PathBuf>,
}

fn trace_file() -> &'static Mutex<Option<File>> {
    TRACE.get_or_init(|| Mutex::new(None))
}

unsafe fn raise_scenario_error(sc: *mut s7_scheme, message: impl AsRef<str>) -> s7_pointer {
    let message = CString::new(message.as_ref().replace('\0', "\\0"))
        .expect("sanitized scenario error contains a null byte");
    unsafe {
        s7_error(
            sc,
            s7_make_symbol(sc, c"scenario-error".as_ptr()),
            s7_list(sc, 1, s7_make_string(sc, message.as_ptr())),
        )
    }
}

unsafe fn parse_action(
    sc: *mut s7_scheme,
    entry: s7_pointer,
    index: usize,
) -> Result<ScenarioAction, String> {
    unsafe {
        if !s7_is_proper_list(sc, entry) {
            return Err(format!("Scenario action {index} must be a proper list"));
        }
        let length = s7_list_length(sc, entry);
        if length != 3 && length != 4 {
            return Err(format!(
                "Scenario action {index} must have shape (url expression schedule [tick]): {}",
                obj2str(sc, entry)
            ));
        }

        let url = s7_car(entry);
        if !s7_is_string(url) {
            return Err(format!("Scenario action {index} URL must be a string"));
        }
        let url = CStr::from_ptr(s7_string(url))
            .to_str()
            .map_err(|_| format!("Scenario action {index} URL is not valid UTF-8"))?
            .to_string();
        let expression = obj2str(sc, s7_cadr(entry));
        let schedule = s7_caddr(entry);
        if !s7_is_proper_list(sc, schedule) {
            return Err(format!(
                "Scenario action {index} schedule must be a proper list"
            ));
        }
        let mut parsed_schedule = Vec::new();
        let mut remaining = schedule;
        while !s7_is_null(sc, remaining) {
            let delivery = s7_car(remaining);
            if s7_is_integer(delivery) && s7_integer(delivery) >= 0 {
                parsed_schedule.push(Delivery::After(s7_integer(delivery) as u64));
            } else if s7_is_boolean(delivery) && !s7_boolean(sc, delivery) {
                parsed_schedule.push(Delivery::Drop);
            } else {
                return Err(format!(
                    "Scenario action {index} schedule entry must be a non-negative integer or #f: {}",
                    obj2str(sc, delivery)
                ));
            }
            remaining = s7_cdr(remaining);
        }
        let tick = if length == 4 {
            let tick = s7_car(s7_cdr(s7_cdr(s7_cdr(entry))));
            if !s7_is_integer(tick) || s7_integer(tick) < 0 {
                return Err(format!(
                    "Scenario action {index} tick must be a non-negative integer: {}",
                    obj2str(sc, tick)
                ));
            }
            s7_integer(tick) as u64
        } else {
            0
        };
        Ok(ScenarioAction {
            url,
            expression,
            schedule: parsed_schedule,
            tick,
        })
    }
}

unsafe fn parse_actions(
    sc: *mut s7_scheme,
    actions: s7_pointer,
) -> Result<Vec<ScenarioAction>, String> {
    unsafe {
        if !s7_is_proper_list(sc, actions) {
            return Err("run-scenario expects a proper action list".to_string());
        }
        let mut parsed = Vec::new();
        let mut current = actions;
        while !s7_is_null(sc, current) {
            parsed.push(parse_action(sc, s7_car(current), parsed.len())?);
            current = s7_cdr(current);
        }
        Ok(parsed)
    }
}

fn write_trace(event: &ScenarioEvent) {
    let value = match event {
        ScenarioEvent::Message {
            sequence,
            time,
            action,
            message,
            kind,
            source,
            target,
            latency,
            dropped,
        } => json!({
            "type": "message",
            "sequence": sequence,
            "time": time,
            "action": action,
            "message": message,
            "kind": kind,
            "source": source,
            "target": target,
            "latency": latency,
            "dropped": dropped,
        }),
        ScenarioEvent::Result {
            sequence,
            time,
            action,
            value,
        } => json!({
            "type": "result",
            "sequence": sequence,
            "time": time,
            "action": action,
            "value": value,
        }),
    };
    if let Some(file) = trace_file()
        .lock()
        .expect("scenario trace lock was poisoned")
        .as_mut()
    {
        let _ = writeln!(file, "{value}");
    }
}

unsafe fn read_result(sc: *mut s7_scheme, result: &str) -> Result<s7_pointer, String> {
    unsafe {
        let result = CString::new(result)
            .map_err(|_| "Scenario result contains an unexpected null byte".to_string())?;
        let port = s7_open_input_string(sc, result.as_ptr());
        let value = s7_read(sc, port);
        s7_close_input_port(sc, port);
        Ok(value)
    }
}

fn primitive_run_scenario() -> Primitive {
    unsafe extern "C" fn run(sc: *mut s7_scheme, args: s7_pointer) -> s7_pointer {
        unsafe {
            let actions = match parse_actions(sc, s7_car(args)) {
                Ok(actions) => actions,
                Err(error) => return raise_scenario_error(sc, error),
            };
            let results = match run_scenario(
                &mut WORLD
                    .get_or_init(|| Mutex::new(ScenarioWorld::new()))
                    .lock()
                    .expect("scenario world lock was poisoned"),
                actions,
                |event| write_trace(&event),
            ) {
                Ok(results) => results,
                Err(error) => return raise_scenario_error(sc, error.to_string()),
            };
            match read_result(sc, &format!("({})", results.join(" "))) {
                Ok(value) => value,
                Err(error) => raise_scenario_error(sc, error),
            }
        }
    }

    Primitive::new(
        run,
        c"run-scenario",
        c"(run-scenario actions) runs a deterministic test schedule and returns results in action order",
        1,
        0,
        false,
    )
}

fn primitive_scenario_submit() -> Primitive {
    unsafe extern "C" fn submit(sc: *mut s7_scheme, args: s7_pointer) -> s7_pointer {
        unsafe {
            let action = match parse_action(sc, s7_car(args), 0) {
                Ok(action) => action,
                Err(error) => return raise_scenario_error(sc, error),
            };
            match WORLD
                .get_or_init(|| Mutex::new(ScenarioWorld::new()))
                .lock()
                .expect("scenario world lock was poisoned")
                .submit(action)
            {
                Ok(action) => s7_make_integer(sc, action as i64),
                Err(error) => raise_scenario_error(sc, error.to_string()),
            }
        }
    }

    Primitive::new(
        submit,
        c"scenario-submit",
        c"(scenario-submit action) submits one deterministic scenario action",
        1,
        0,
        false,
    )
}

fn primitive_scenario_await() -> Primitive {
    unsafe extern "C" fn await_next(sc: *mut s7_scheme, _args: s7_pointer) -> s7_pointer {
        unsafe {
            let result = WORLD
                .get_or_init(|| Mutex::new(ScenarioWorld::new()))
                .lock()
                .expect("scenario world lock was poisoned")
                .await_next(|event| write_trace(&event));
            match result {
                Ok(result) => match read_result(sc, &result) {
                    Ok(value) => value,
                    Err(error) => raise_scenario_error(sc, error),
                },
                Err(error) => raise_scenario_error(sc, error.to_string()),
            }
        }
    }

    Primitive::new(
        await_next,
        c"scenario-await",
        c"(scenario-await) awaits the oldest submitted scenario action",
        0,
        0,
        false,
    )
}

fn usage() -> String {
    concat!(
        "usage: records-test [--trace FILE] TEST-FILE [ARGUMENT-FILE ...]\n",
        "       records-test --unit TEST-FILE [ARGUMENT-FILE ...]\n",
        "       records-test --suite SUITE-FILE [--jobs N]"
    )
    .to_string()
}

fn main() {
    match run() {
        Ok(result) => println!("{result}"),
        Err(error) => {
            eprintln!("records-test: {error}");
            std::process::exit(1);
        }
    }
}

fn application_from_files(paths: &[PathBuf]) -> Result<String, String> {
    let test_path = &paths[0];
    let test = fs::read_to_string(test_path)
        .map_err(|error| format!("Could not read {}: {error}", test_path.display()))?;
    let mut application = format!("({test}");
    for path in &paths[1..] {
        let argument = fs::read_to_string(path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        application.push_str(" '");
        application.push_str(&argument);
    }
    application.push(')');
    Ok(application)
}

fn checked_result(result: String) -> Result<String, String> {
    if result.starts_with("(error '") {
        Err(result)
    } else {
        Ok(result)
    }
}

fn evaluate_files(paths: &[PathBuf]) -> Result<String, String> {
    let application = application_from_files(paths)?;
    let mut primitives = driver_primitives();
    primitives.push(primitive_run_scenario());
    primitives.push(primitive_scenario_submit());
    primitives.push(primitive_scenario_await());
    let evaluator = Evaluator::new(HashMap::new(), primitives);
    checked_result(evaluator.evaluate(&application))
}

fn evaluate_unit_files(paths: &[PathBuf]) -> Result<String, String> {
    checked_result(JOURNAL.evaluate(&application_from_files(paths)?))
}

fn suite_path(base: &std::path::Path, path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn run_suite(path: &std::path::Path, jobs: usize) -> Result<String, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let suite: SuiteManifest = toml::from_str(&source)
        .map_err(|error| format!("Could not parse {}: {error}", path.display()))?;
    if suite.cases.is_empty() {
        return Err(format!("Suite contains no cases: {}", path.display()));
    }

    let base = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate records-test executable: {error}"))?;
    let mut cases = Vec::with_capacity(suite.cases.len());
    for case in suite.cases {
        if case.name.trim().is_empty() {
            return Err(format!("Suite case has an empty name: {}", path.display()));
        }
        let mut paths = vec![suite_path(base, &case.test)];
        paths.extend(
            case.arguments
                .iter()
                .map(|argument| suite_path(base, argument)),
        );
        cases.push((case.name, case.kind, paths));
    }

    let next = AtomicUsize::new(0);
    let results = Mutex::new(
        (0..cases.len())
            .map(|_| None)
            .collect::<Vec<Option<Result<std::process::Output, String>>>>(),
    );
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(cases.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= cases.len() {
                        break;
                    }
                    let (name, kind, paths) = &cases[index];
                    let mut command = Command::new(&executable);
                    if matches!(kind, SuiteKind::Unit) {
                        command.arg("--unit");
                    }
                    let result = command
                        .args(paths)
                        .output()
                        .map_err(|error| format!("Could not run {name}: {error}"));
                    results.lock().expect("suite results lock was poisoned")[index] = Some(result);
                }
            });
        }
    });

    let results = results
        .into_inner()
        .expect("suite results lock was poisoned");
    let mut output = Vec::with_capacity(cases.len() + 1);
    for ((name, _, _), result) in cases.into_iter().zip(results) {
        let child = result.expect("suite worker did not record a result")?;
        let stdout = String::from_utf8_lossy(&child.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&child.stderr).trim().to_string();
        if !child.status.success() {
            let details = if stderr.is_empty() { &stdout } else { &stderr };
            return Err(format!(
                "{} failed{}{}",
                name,
                if details.is_empty() { "" } else { ":\n" },
                details,
            ));
        }
        output.push(format!("--- {name} ---\n{stdout}"));
    }
    output.push(format!("Success ({} cases)", output.len()));
    Ok(output.join("\n"))
}

fn run() -> Result<String, String> {
    let mut paths = Vec::<PathBuf>::new();
    let mut trace = None::<PathBuf>;
    let mut suite = None::<PathBuf>;
    let mut unit = false;
    let mut jobs = None::<usize>;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--trace" {
            trace = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| "--trace requires a file path".to_string())?,
            ));
        } else if argument == "--unit" {
            if unit {
                return Err("--unit may be specified only once".to_string());
            }
            unit = true;
        } else if argument == "--jobs" {
            if jobs.is_some() {
                return Err("--jobs may be specified only once".to_string());
            }
            let value = args
                .next()
                .ok_or_else(|| "--jobs requires a positive integer".to_string())?;
            let value = value
                .parse::<usize>()
                .map_err(|_| format!("--jobs requires a positive integer: {value}"))?;
            if value == 0 {
                return Err("--jobs requires a positive integer".to_string());
            }
            jobs = Some(value);
        } else if argument == "--suite" {
            if suite.is_some() {
                return Err("--suite may be specified only once".to_string());
            }
            suite = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| "--suite requires a file path".to_string())?,
            ));
        } else if argument == "--help" || argument == "-h" {
            return Ok(usage());
        } else {
            paths.push(PathBuf::from(argument));
        }
    }

    if let Some(suite) = suite {
        if !paths.is_empty() || trace.is_some() || unit {
            return Err(
                "--suite cannot be combined with positional files, --trace, or --unit".to_string(),
            );
        }
        return run_suite(&suite, jobs.unwrap_or(1));
    }
    if jobs.is_some() {
        return Err("--jobs requires --suite".to_string());
    }
    if paths.is_empty() {
        return Err(usage());
    }
    if unit {
        if trace.is_some() {
            return Err("--unit cannot be combined with --trace".to_string());
        }
        return evaluate_unit_files(&paths);
    }

    *trace_file()
        .lock()
        .expect("scenario trace lock was poisoned") = match trace {
        Some(path) => Some(
            File::create(&path)
                .map_err(|error| format!("Could not create {}: {error}", path.display()))?,
        ),
        None => None,
    };

    evaluate_files(&paths)
}
