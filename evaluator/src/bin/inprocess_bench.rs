use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn median(xs: &mut [u128]) -> u128 {
    xs.sort_unstable();
    xs[xs.len() / 2]
}

fn main() -> ExitCode {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(path) = args.next() else {
        eprintln!("usage: {} FILE.scm [repeats] [warmups]", Path::new(&program).display());
        return ExitCode::from(2);
    };
    let repeats = args.next().and_then(|s| s.into_string().ok()).and_then(|s| s.parse::<usize>().ok()).unwrap_or(20);
    let warmups = args.next().and_then(|s| s.into_string().ok()).and_then(|s| s.parse::<usize>().ok()).unwrap_or(3);
    if args.next().is_some() {
        eprintln!("usage: {} FILE.scm [repeats] [warmups]", Path::new(&program).display());
        return ExitCode::from(2);
    }
    let path = Path::new(&path);
    let Ok(source) = fs::read_to_string(path) else {
        eprintln!("input file not found: {}", path.display());
        return ExitCode::from(1);
    };

    let result = std::thread::Builder::new()
        .name("s7-rust-inprocess-bench".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || s7_rust::run_source_output_repeated(&source, warmups, repeats))
        .expect("failed to start evaluator thread")
        .join()
        .expect("evaluator thread panicked");

    match result {
        Ok((output, timings)) => {
            if timings.is_empty() {
                eprintln!("repeats must be greater than zero");
                return ExitCode::from(2);
            }
            let mut sorted = timings.clone();
            let med = median(&mut sorted);
            let min = *sorted.first().unwrap();
            let max = *sorted.last().unwrap();
            println!("output: {}", output);
            println!("repeats: {}", repeats);
            println!("warmups: {}", warmups);
            println!("median_ns: {}", med);
            println!("min_ns: {}", min);
            println!("max_ns: {}", max);
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("{}", err);
            ExitCode::SUCCESS
        }
    }
}
