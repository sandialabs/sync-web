use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(path) = args.next() else {
        eprintln!("usage: {} FILE.scm", Path::new(&program).display());
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: {} FILE.scm", Path::new(&program).display());
        return ExitCode::from(2);
    }
    let path = Path::new(&path);
    let Ok(source) = fs::read_to_string(path) else {
        eprintln!("input file not found: {}", path.display());
        return ExitCode::from(1);
    };

    let output = std::thread::Builder::new()
        .name("s7-rust-main".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || match s7_rust::run_source(&source) {
            Ok(value) => value.to_string(),
            Err(err) => if err.args.is_empty() { err.tag } else { err.to_scheme() },
        })
        .expect("failed to start evaluator thread")
        .join()
        .expect("evaluator thread panicked");

    println!("{}", output);
    ExitCode::SUCCESS
}
