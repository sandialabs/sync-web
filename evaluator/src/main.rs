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
        .spawn(move || {
            let result=if std::env::var_os("S7_UNIFIED").as_deref()==Some(std::ffi::OsStr::new("1")){s7_rust::run_source_unified_output(&source)}else{s7_rust::run_source_output(&source)};
            match result{Ok(output)|Err(output)=>output}
        })
        .expect("failed to start evaluator thread")
        .join()
        .expect("evaluator thread panicked");

    println!("{}", output);
    ExitCode::SUCCESS
}
