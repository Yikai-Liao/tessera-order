// SPDX-License-Identifier: BSD-2-Clause

fn emit_version(manifest: &str, variable: &str) {
    println!("cargo:rerun-if-changed={manifest}");
    let contents = std::fs::read_to_string(manifest).expect("version manifest must be readable");
    let version = contents
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            if name.trim() != "version" {
                return None;
            }
            value.trim().strip_prefix('"')?.split_once('"').map(|v| v.0)
        })
        .expect("version manifest must contain a quoted version");
    println!("cargo:rustc-env={variable}={version}");
}

fn main() {
    // Maturin moves pyproject.toml to the sdist root while retaining the
    // Rust crate under python/ alongside its packaged path dependency.
    let manifest = if std::path::Path::new("pyproject.toml").is_file() {
        "pyproject.toml"
    } else {
        "../pyproject.toml"
    };
    emit_version(manifest, "TESSERA_PY_VERSION");
}
