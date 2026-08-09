use sha2::Digest;
use std::path::{Path, PathBuf};
use std::{env, fs};

const S7_C_SHA: &str = "9bdf20cf62f6bda998d3cd3375935e26c1a345f6546ba73274e49ccc9b0bd3d6";
const S7_H_SHA: &str = "3790fafd2b6650877db9e2d7d84c7e66823db329481183ba3752d8eda256467c";

fn replace_exact(source: String, label: &str, old: &str, new: &str) -> String {
    let count = source.matches(old).count();
    assert_eq!(count, 1, "s7 patch anchor {label:?} matched {count} times");
    source.replacen(old, new, 1)
}

fn generated_s7(out: &Path) -> PathBuf {
    let source = Path::new("../external/s7/s7.c");
    let bytes = fs::read(source).expect("read exact s7.c");
    let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
    assert_eq!(actual, S7_C_SHA, "exact s7.c identity changed");
    let header = fs::read("../external/s7/s7.h").expect("read exact s7.h");
    let actual = format!("{:x}", sha2::Sha256::digest(&header));
    assert_eq!(actual, S7_H_SHA, "exact s7.h identity changed");
    let mut text = String::from_utf8(bytes).expect("s7.c utf8");
    let rusage = "#if (!_WIN32) /* (!MS_WINDOWS) */";
    assert_eq!(text.matches(rusage).count(), 3, "rusage anchor count");
    text = text.replace(
        rusage,
        "#if (!_WIN32) && (!defined(__wasi__)) /* generated WASI guard */",
    );
    let jump = "#if defined(_MSC_VER) || defined(__MINGW32__)";
    assert_eq!(text.matches(jump).count(), 1, "setjmp anchor count");
    text = text.replace(
        jump,
        "#if defined(_MSC_VER) || defined(__MINGW32__) || defined(__wasi__)",
    );
    let path = out.join("generated-s7.c");
    fs::write(&path, text).expect("write generated s7");
    path
}

fn main() {
    println!("cargo:rerun-if-changed=../external/s7/s7.c");
    println!("cargo:rerun-if-changed=../external/s7/s7.h");
    println!("cargo:rerun-if-changed=../wrapper.h");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let source = generated_s7(&out);
    cc::Build::new()
        .file(source)
        .include("../external/s7")
        .flag("-O3")
        .flag("-DDEFAULT_PRINT_LENGTH=2147483647")
        .flag("-DINITIAL_HEAP_SIZE=32000")
        .flag("-DWITH_PURE_S7=1")
        .flag("-DWITH_SYSTEM_EXTRAS=0")
        .flag("-DWITH_C_LOADER=0")
        .flag("-D_WASI_EMULATED_PROCESS_CLOCKS")
        .flag("-mllvm")
        .flag("-wasm-enable-sjlj")
        .flag("-mllvm")
        .flag("-wasm-use-legacy-eh=false")
        .warnings(false)
        .compile("evaluator");
    bindgen::Builder::default()
        .header("../wrapper.h")
        .clang_arg("--target=x86_64-unknown-linux-gnu")
        .clang_arg("-I../external/s7")
        .generate()
        .expect("generate exact s7 bindings")
        .write_to_file(out.join("bindings.rs"))
        .expect("write bindings");
    println!("cargo:rustc-link-lib=setjmp");
    println!("cargo:rustc-link-lib=wasi-emulated-process-clocks");
}
