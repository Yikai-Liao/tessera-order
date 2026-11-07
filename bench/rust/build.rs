// SPDX-License-Identifier: BSD-2-Clause

//! Records build provenance and builds optional pinned native competitors.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{env, fs, path::Path, process::Command};

/// Runs a provenance query and reads its standard output.
fn read_command_output(cmd: &str, args: &[&str]) -> String {
    let result = Command::new(cmd)
        .args(args)
        .output()
        .expect("run build provenance command");
    assert!(
        result.status.success(),
        "{cmd} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8_lossy(&result.stdout).trim().to_owned()
}
/// Hashes source paths and contents in a deterministic order.
fn hash_tree(path: &Path, digest: &mut Sha256) {
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.file_name().unwrap() == "__pycache__" {
                continue;
            }
            hash_tree(&entry, digest);
        }
    } else {
        digest.update(path.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update(fs::read(path).unwrap());
    }
}
/// Writes build provenance and compiles optional native adapters.
fn main() {
    for key in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CC",
        "CXX",
        "AR",
        "CFLAGS",
        "CXXFLAGS",
        "LDFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let reference = read_command_output("git", &["rev-parse", "--symbolic-full-name", "HEAD"]);
    for name in ["HEAD", "index", "packed-refs", reference.as_str()] {
        let path = read_command_output("git", &["rev-parse", "--git-path", name]);
        println!("cargo:rerun-if-changed={path}");
    }
    let out = env::var("OUT_DIR").unwrap();
    let mut digest = Sha256::new();
    for path in [
        "../../rust/tessera-sort/src",
        "src",
        "native",
        "build.rs",
        "Cargo.toml",
        "../../Cargo.toml",
        "../../Cargo.lock",
    ] {
        hash_tree(Path::new(path), &mut digest);
    }
    let mut features: Vec<_> = env::vars()
        .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
        .map(|(k, _)| k)
        .collect();
    features.sort();
    let mut value = json!({
        "schema": 1,
        "commit": read_command_output("git", &["rev-parse", "HEAD"]),
        "dirty": !read_command_output("git", &["status", "--porcelain"]).is_empty(),
        "rustc": read_command_output(&env::var("RUSTC").unwrap_or_else(|_| "rustc".into()), &["-vV"]),
        "source_sha256": format!("{:x}", digest.finalize()),
        "cargo_lock": fs::read_to_string("../../Cargo.lock").unwrap(),
        "features": features,
    });
    for key in [
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
        "RUSTC_LINKER",
    ] {
        value[key.to_lowercase()] = json!(env::var(key).unwrap_or_default());
    }
    if env::var_os("CARGO_FEATURE_COMPARE_NATIVE").is_some() {
        native(Path::new(&out));
        value["native_sources"] = json!(fs::read_to_string("native/sources.toml").unwrap());
        value["native_build"] = serde_json::from_slice::<Value>(
            &fs::read(Path::new(&out).join("native-build.json")).unwrap(),
        )
        .unwrap();
        for (key, variable, fallback) in [("cc", "CC", "cc"), ("cxx", "CXX", "c++")] {
            let compiler = env::var(variable).unwrap_or_else(|_| fallback.into());
            value[key] = json!(read_command_output(&compiler, &["--version"]));
        }
    }
    fs::write(
        Path::new(&out).join("provenance.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
/// Builds pinned native adapters and registers their linker inputs.
fn native(out: &Path) {
    assert_eq!(
        env::var("TARGET").unwrap(),
        "x86_64-unknown-linux-gnu",
        "compare-native requires Linux x86-64 GNU"
    );
    assert!(
        Path::new("../vendor/highway/.tessera-source.json").exists(),
        "run python3 bench/fetch.py before building compare-native"
    );
    println!("cargo:rerun-if-changed=../vendor");
    let status = Command::new("python3")
        .args(["native/build.py"])
        .arg(out)
        .status()
        .unwrap();
    assert!(status.success(), "native build failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    for lib in ["tessera_native", "hwy_contrib", "hwy", "x86simdsort"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=m");
    println!(
        "cargo:rustc-link-arg-bin=memory={}",
        out.join("memory.o").display()
    );
    for symbol in [
        "malloc",
        "calloc",
        "realloc",
        "free",
        "aligned_alloc",
        "posix_memalign",
    ] {
        println!("cargo:rustc-link-arg-bin=memory=-Wl,--wrap={symbol}");
    }
}
