#![allow(warnings, unused)]

use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const S7_C_SHA256: &str = "9bdf20cf62f6bda998d3cd3375935e26c1a345f6546ba73274e49ccc9b0bd3d6";
const S7_H_SHA256: &str = "3790fafd2b6650877db9e2d7d84c7e66823db329481183ba3752d8eda256467c";

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn replace_exact(source: String, label: &str, old: &str, new: &str) -> String {
    let count = source.matches(old).count();
    assert_eq!(count, 1, "s7 patch anchor {label:?} matched {count} times");
    source.replacen(old, new, 1)
}

fn patch_s7(out_path: &Path, msvc: bool) -> (PathBuf, PathBuf) {
    let patched_dir = out_path.join("patched-s7");
    fs::create_dir_all(&patched_dir).expect("Failed to create patched s7 directory");

    let s7_c_path = patched_dir.join("s7.c");
    let s7_h_path = patched_dir.join("s7.h");
    let wrapper_path = patched_dir.join("wrapper.h");

    let vendor_c = fs::read("external/s7/s7.c").expect("Failed to read s7.c");
    let vendor_h = fs::read("external/s7/s7.h").expect("Failed to read s7.h");
    assert_eq!(sha256(&vendor_c), S7_C_SHA256, "vendored s7.c hash drift");
    assert_eq!(sha256(&vendor_h), S7_H_SHA256, "vendored s7.h hash drift");

    let mut s7_c = String::from_utf8(vendor_c).expect("s7.c is not UTF-8");
    let mut s7_h = String::from_utf8(vendor_h).expect("s7.h is not UTF-8");

    if msvc {
        s7_h = replace_exact(
            s7_h,
            "MSVC C11 bool header",
            "#ifndef __cplusplus\n#ifndef _MSC_VER\n  #include <stdbool.h>\n#else\n#ifndef true\n  #define bool\tunsigned char\n  #define true\t1\n  #define false\t0\n#endif\n#endif\n#endif\n",
            "#ifndef __cplusplus\n  #include <stdbool.h>\n#endif\n",
        );
        s7_c = replace_exact(
            s7_c,
            "MSVC noreturn declaration",
            "#ifdef _MSC_VER\n  #define noreturn _Noreturn /* deprecated in C23 */\n#else\n  #define noreturn __attribute__((noreturn))\n  /* this is ok in gcc/g++/clang and tcc; pure attribute is rarely applicable here, and does not seem to be helpful (maybe safe_strlen) */\n#endif\n",
            "#if defined(_MSC_VER) && !(defined(__STDC_VERSION__) && (__STDC_VERSION__ >= 199901L))\n  #define noreturn _Noreturn /* deprecated in C23 */\n#elif defined(_MSC_VER)\n  #include <stdbool.h>\n#else\n  #define noreturn __attribute__((noreturn))\n  /* this is ok in gcc/g++/clang and tcc; pure attribute is rarely applicable here, and does not seem to be helpful (maybe safe_strlen) */\n#endif\n",
        );
        s7_c = replace_exact(
            s7_c,
            "MSVC C11 noreturn definition",
            "#include <stdint.h>\n#include <inttypes.h>\n#include <setjmp.h>\n\n#ifdef _MSC_VER\n",
            "#include <stdint.h>\n#include <inttypes.h>\n#include <setjmp.h>\n\n#if defined(_MSC_VER) && (defined(__STDC_VERSION__) && (__STDC_VERSION__ >= 199901L))\n  #define noreturn _Noreturn\n#endif\n\n#ifdef _MSC_VER\n",
        );
    }

    fs::write(&s7_c_path, &s7_c).expect("Failed to write patched s7.c");
    fs::write(&s7_h_path, &s7_h).expect("Failed to write patched s7.h");
    fs::write(&wrapper_path, "#include \"s7.h\"\n").expect("Failed to write patched wrapper.h");
    fs::write(
        out_path.join("s7-patch-receipt.txt"),
        format!(
            "vendor_s7_c_sha256={S7_C_SHA256}\nvendor_s7_h_sha256={S7_H_SHA256}\ngenerated_s7_c_sha256={}\ngenerated_s7_h_sha256={}\nmeter_replacements=0\nmsvc_replacements={}\n",
            sha256(s7_c.as_bytes()),
            sha256(s7_h.as_bytes()),
            if msvc { 3 } else { 0 },
        ),
    )
    .expect("Failed to write s7 patch receipt");

    (s7_c_path, wrapper_path)
}

fn main() {
    println!("cargo:rustc-link-search=/path/to/lib");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=external/s7/s7.c");
    println!("cargo:rerun-if-changed=external/s7/s7.h");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target = env::var("TARGET").unwrap_or_default();
    let msvc = target.ends_with("windows-msvc");
    let (s7_source, wrapper_header) = patch_s7(&out_path, msvc);

    let mut s7_build = cc::Build::new();
    s7_build
        .file(&s7_source)
        .flag(format!("-DDEFAULT_PRINT_LENGTH={}", isize::MAX).as_str())
        .flag("-DWITH_PURE_S7=1")
        .flag("-DWITH_SYSTEM_EXTRAS=0")
        .flag("-DWITH_C_LOADER=0")
        .warnings(false);
    if msvc {
        s7_build.flag("/std:c11");
    }
    s7_build.compile("evaluator");

    let mut bindings = bindgen::Builder::default()
        .header(wrapper_header.to_string_lossy())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks));
    if msvc {
        bindings = bindings.clang_arg("-std=c11");
    }
    let bindings = bindings.generate().expect("Unable to generate bindings");

    if msvc {
        let mut bindings = bindings.to_string();
        bindings = replace_exact(
            bindings,
            "MSVC s7_make_boolean binding",
            "pub fn s7_make_boolean(sc: *mut s7_scheme, x: ::std::os::raw::c_uchar) -> s7_pointer;",
            "pub fn s7_make_boolean(sc: *mut s7_scheme, x: bool) -> s7_pointer;",
        );
        bindings = replace_exact(
            bindings,
            "MSVC use_write binding",
            "use_write: ::std::os::raw::c_uchar,",
            "use_write: bool,",
        );
        bindings = replace_exact(
            bindings,
            "MSVC rest_arg binding",
            "rest_arg: ::std::os::raw::c_uchar,",
            "rest_arg: bool,",
        );
        let mut boolean_returns = 0;
        let bindings = bindings
            .lines()
            .map(|line| {
                if (line.contains("pub fn s7_is_") || line.contains("pub fn s7_boolean("))
                    && line.ends_with(") -> ::std::os::raw::c_uchar;")
                {
                    boolean_returns += 1;
                    line.replace(") -> ::std::os::raw::c_uchar;", ") -> bool;")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(boolean_returns, 51, "MSVC boolean return binding drift");
        fs::write(
            out_path.join("bindings-patch-receipt.txt"),
            "exact_field_replacements=3\nboolean_return_replacements=51\n",
        )
        .expect("Couldn't write bindings patch receipt");
        fs::write(out_path.join("bindings.rs"), bindings).expect("Couldn't write bindings!");
    } else {
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
