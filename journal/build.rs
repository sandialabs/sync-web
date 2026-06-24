#![allow(warnings, unused)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn patch_s7_for_msvc_c11(out_path: &Path) -> (PathBuf, PathBuf) {
    let patched_dir = out_path.join("patched-s7");
    fs::create_dir_all(&patched_dir).expect("Failed to create patched s7 directory");

    let s7_c_path = patched_dir.join("s7.c");
    let s7_h_path = patched_dir.join("s7.h");
    let wrapper_path = patched_dir.join("wrapper.h");

    let s7_c = fs::read_to_string("external/s7/s7.c").expect("Failed to read s7.c");
    let s7_h = fs::read_to_string("external/s7/s7.h").expect("Failed to read s7.h");

    let s7_h = s7_h.replace(
        "#ifndef __cplusplus\n#ifndef _MSC_VER\n  #include <stdbool.h>\n#else\n#ifndef true\n  #define bool\tunsigned char\n  #define true\t1\n  #define false\t0\n#endif\n#endif\n#endif\n",
        "#ifndef __cplusplus\n  #include <stdbool.h>\n#endif\n",
    );

    let s7_c = s7_c
        .replace(
            "#ifdef _MSC_VER\n  #define noreturn _Noreturn /* deprecated in C23 */\n#else\n  #define noreturn __attribute__((noreturn))\n  /* this is ok in gcc/g++/clang and tcc; pure attribute is rarely applicable here, and does not seem to be helpful (maybe safe_strlen) */\n#endif\n",
            "#if defined(_MSC_VER) && !(defined(__STDC_VERSION__) && (__STDC_VERSION__ >= 199901L))\n  #define noreturn _Noreturn /* deprecated in C23 */\n#elif defined(_MSC_VER)\n  #include <stdbool.h>\n#else\n  #define noreturn __attribute__((noreturn))\n  /* this is ok in gcc/g++/clang and tcc; pure attribute is rarely applicable here, and does not seem to be helpful (maybe safe_strlen) */\n#endif\n",
        )
        .replace(
            "#include <stdint.h>\n#include <inttypes.h>\n#include <setjmp.h>\n\n#ifdef _MSC_VER\n",
            "#include <stdint.h>\n#include <inttypes.h>\n#include <setjmp.h>\n\n#if defined(_MSC_VER) && (defined(__STDC_VERSION__) && (__STDC_VERSION__ >= 199901L))\n  #define noreturn _Noreturn\n#endif\n\n#ifdef _MSC_VER\n",
        );

    fs::write(&s7_c_path, s7_c).expect("Failed to write patched s7.c");
    fs::write(&s7_h_path, s7_h).expect("Failed to write patched s7.h");
    fs::write(&wrapper_path, "#include \"s7.h\"\n").expect("Failed to write patched wrapper.h");

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
    let (s7_source, wrapper_header) = if target.ends_with("windows-msvc") {
        patch_s7_for_msvc_c11(&out_path)
    } else {
        (PathBuf::from("external/s7/s7.c"), PathBuf::from("wrapper.h"))
    };

    let mut s7_build = cc::Build::new();
    s7_build
	.file(&s7_source)
	.flag(format!("-DDEFAULT_PRINT_LENGTH={}", isize::MAX).as_str())
	.flag("-DWITH_PURE_S7=1")
	.flag("-DWITH_SYSTEM_EXTRAS=0")
	.flag("-DWITH_C_LOADER=0")
	.warnings(false);
    if target.ends_with("windows-msvc") {
        s7_build.flag("/std:c11");
    }
    s7_build.compile("evaluator");

    let mut bindings = bindgen::Builder::default()
	.header(wrapper_header.to_string_lossy())
	.parse_callbacks(Box::new(bindgen::CargoCallbacks));
    if target.ends_with("windows-msvc") {
        bindings = bindings.clang_arg("-std=c11");
    }
    let bindings = bindings
	.generate()
	.expect("Unable to generate bindings");

    if target.ends_with("windows-msvc") {
        let bindings = bindings
            .to_string()
            .replace(
                "pub fn s7_make_boolean(sc: *mut s7_scheme, x: ::std::os::raw::c_uchar) -> s7_pointer;",
                "pub fn s7_make_boolean(sc: *mut s7_scheme, x: bool) -> s7_pointer;",
            )
            .replace(
                "use_write: ::std::os::raw::c_uchar,",
                "use_write: bool,",
            )
            .replace(
                "rest_arg: ::std::os::raw::c_uchar,",
                "rest_arg: bool,",
            );
        let bindings = bindings
            .lines()
            .map(|line| {
                if (line.contains("pub fn s7_is_") || line.contains("pub fn s7_boolean("))
                    && line.ends_with(") -> ::std::os::raw::c_uchar;")
                {
                    line.replace(") -> ::std::os::raw::c_uchar;", ") -> bool;")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(out_path.join("bindings.rs"), bindings).expect("Couldn't write bindings!");
    } else {
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
