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

// SHA-256 over length-framed paths and file contents; no build dependency is needed.
fn sha256(mut bytes: Vec<u8>) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bits = (bytes.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend_from_slice(&bits.to_be_bytes());
    let mut hash = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in bytes.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (word, chunk) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*chunk);
        }
        for i in 16..64 {
            let x = w[i - 15];
            let y = w[i - 2];
            w[i] = w[i - 16]
                .wrapping_add(x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3))
                .wrapping_add(w[i - 7])
                .wrapping_add(y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10));
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = hash;
        for i in 0..64 {
            let t1 = h
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            (a, b, c, d, e, f, g, h) = (t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g);
        }
        for (value, add) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *value = value.wrapping_add(add);
        }
    }
    hash.iter().map(|word| format!("{word:08x}")).collect()
}

fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn collect_sources(
    path: &std::path::Path,
    name: &str,
    files: &mut Vec<(String, std::path::PathBuf)>,
) {
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        for entry in std::fs::read_dir(path).expect("source directory must be readable") {
            let entry = entry.expect("source entry must be readable");
            collect_sources(
                &entry.path(),
                &format!("{name}/{}", entry.file_name().to_string_lossy()),
                files,
            );
        }
    } else {
        files.push((name.to_owned(), path.to_owned()));
    }
}

fn main() {
    use std::path::Path;
    // Maturin moves pyproject.toml to the sdist root while retaining the
    // binding crate and its sibling Rust dependency at their relative paths.
    let manifest = if Path::new("pyproject.toml").is_file() {
        "pyproject.toml"
    } else {
        "../pyproject.toml"
    };
    emit_version(manifest, "TESSERA_PY_VERSION");
    let core = Path::new("../rust/tessera-order");
    let mut files = Vec::new();
    for (path, name) in [
        (Path::new("src").to_owned(), "python/src"),
        ("build.rs".into(), "python/build.rs"),
        ("Cargo.toml".into(), "python/Cargo.toml"),
        ("Cargo.lock".into(), "python/Cargo.lock"),
        (manifest.into(), "python/pyproject.toml"),
        (core.join("src"), "rust/tessera-order/src"),
        (core.join("Cargo.toml"), "rust/tessera-order/Cargo.toml"),
    ] {
        collect_sources(&path, name, &mut files);
    }
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut source = Vec::new();
    for (name, path) in files {
        let contents = std::fs::read(path).expect("build source must be readable");
        source.extend_from_slice(&(name.len() as u64).to_be_bytes());
        source.extend_from_slice(name.as_bytes());
        source.extend_from_slice(&(contents.len() as u64).to_be_bytes());
        source.extend_from_slice(&contents);
    }
    // An sdist may live inside an unrelated checkout; only tracked binding
    // manifests authorize Git provenance from the surrounding repository.
    let in_checkout =
        command("git", &["ls-files", "--error-unmatch", "--", "Cargo.toml"]).is_some();
    let commit = in_checkout
        .then(|| command("git", &["rev-parse", "HEAD"]))
        .flatten()
        .unwrap_or_else(|| "unknown".into());
    let dirty = in_checkout
        .then(|| {
            command(
                "git",
                &[
                    "status",
                    "--porcelain",
                    "--",
                    ".",
                    "../rust/tessera-order",
                    "../Cargo.toml",
                ],
            )
        })
        .flatten()
        .map_or(
            "unknown",
            |status| if status.is_empty() { "false" } else { "true" },
        );
    if in_checkout {
        if Path::new("../Cargo.toml").is_file() {
            println!("cargo:rerun-if-changed=../Cargo.toml");
        }
        // Follow both worktree HEAD and symbolic branch refs, including packed refs.
        for args in [
            vec!["rev-parse", "--git-path", "HEAD"],
            vec!["rev-parse", "--git-path", "packed-refs"],
            vec!["rev-parse", "--git-path", "index"],
        ] {
            if let Some(path) = command("git", &args).filter(|path| Path::new(path).exists()) {
                println!("cargo:rerun-if-changed={path}");
            }
        }
        if let Some(branch) = command("git", &["symbolic-ref", "-q", "HEAD"])
            && let Some(path) = command("git", &["rev-parse", "--git-path", &branch])
                .filter(|path| Path::new(path).exists())
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    for name in [
        "RUSTC",
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_FEATURE_ABI3_PY311",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    for (name, value) in [
        ("COMMIT", commit),
        ("DIRTY", dirty.into()),
        ("SOURCE_SHA256", sha256(source)),
        (
            "RUSTC",
            command(&rustc, &["--version"]).unwrap_or_else(|| "unknown".into()),
        ),
        (
            "TARGET",
            std::env::var("TARGET").unwrap_or_else(|_| "unknown".into()),
        ),
        (
            "PROFILE",
            std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into()),
        ),
        (
            "OPT_LEVEL",
            std::env::var("OPT_LEVEL").unwrap_or_else(|_| "unknown".into()),
        ),
        (
            "RUSTFLAGS",
            std::env::var("CARGO_ENCODED_RUSTFLAGS")
                .unwrap_or_default()
                .replace('\u{1f}', " "),
        ),
        (
            "ABI",
            if std::env::var_os("CARGO_FEATURE_ABI3_PY311").is_some() {
                "cp311-abi3"
            } else {
                "cpython"
            }
            .into(),
        ),
    ] {
        println!("cargo:rustc-env=TESSERA_BUILD_{name}={value}");
    }
}
