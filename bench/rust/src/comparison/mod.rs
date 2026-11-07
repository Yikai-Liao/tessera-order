// SPDX-License-Identifier: BSD-2-Clause

mod check;
mod host;
#[cfg(feature = "compare-native")]
mod native;
mod run;
mod suite;
mod variant;

use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use suite::Case;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BUILD: &str = if cfg!(feature = "portable-simd") {
    "portable-simd"
} else {
    "default"
};

struct Options {
    command: String,
    suite: String,
    group: Option<String>,
    out: PathBuf,
    min_ns: u64,
    cap: usize,
}
impl Options {
    fn parse(memory: bool) -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let command = if memory {
            "memory".into()
        } else {
            args.next().unwrap_or_else(|| "help".into())
        };
        let mut o = Self {
            command,
            suite: "smoke".into(),
            group: None,
            out: "bench/results/local".into(),
            min_ns: 5_000_000,
            cap: 256 << 20,
        };
        while let Some(key) = args.next() {
            if key == "--help" || key == "-h" {
                o.command = "help".into();
                return Ok(o);
            }
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {key}"))?;
            match key.as_str() {
                "--suite" => o.suite = value,
                "--group" => o.group = Some(value),
                "--out" => o.out = value.into(),
                "--sample-ms" => {
                    o.min_ns = value
                        .parse::<u64>()?
                        .checked_mul(1_000_000)
                        .filter(|&n| n > 0)
                        .ok_or("sample duration must be positive")?
                }
                "--memory-mib" => {
                    o.cap = value
                        .parse::<usize>()?
                        .checked_mul(1 << 20)
                        .filter(|&n| n > 0)
                        .ok_or("memory cap must be positive")?
                }
                _ => return Err(format!("unknown option {key}").into()),
            }
        }
        Ok(o)
    }
}
fn json_file(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn same_or_write(path: &Path, value: &Value) -> Result<()> {
    if path.exists() {
        let old: Value = serde_json::from_slice(&fs::read(path)?)?;
        if old != *value {
            return Err(format!("{} differs from this run", path.display()).into());
        }
    } else {
        json_file(path, value)?;
    }
    Ok(())
}
fn line(file: &mut File, value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
fn manifest(o: &Options, cases: &[Case]) -> Value {
    let mut provenance: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/provenance.json")))
            .expect("build provenance is valid JSON");
    use sha2::{Digest, Sha256};
    provenance["binary_sha256"] = json!(format!(
        "{:x}",
        Sha256::digest(
            fs::read(std::env::current_exe().expect("executable path")).expect("read executable")
        )
    ));
    provenance["suite"] = json!(o.suite);
    provenance["suite_version"] = json!(1);
    provenance["cases"] = json!(
        cases
            .iter()
            .map(|c| {
                let probe = Case {
                    len: 32,
                    distribution: "random".into(),
                    ..c.clone()
                };
                let input = variant::Input::new(&probe);
                let supported: Vec<_> = variant::variants()
                    .iter()
                    .filter(|v| v.prepare(&input, 0).is_some())
                    .map(|v| v.name)
                    .collect();
                json!({"ty":c.ty,"distribution":c.distribution,"len":c.len,"variants":supported})
            })
            .collect::<Vec<_>>()
    );
    provenance["build"] = json!(BUILD);
    provenance["min_sample_ns"] = json!(o.min_ns);
    provenance["prepared_cap_bytes"] = json!(o.cap);
    provenance["blocks"] = json!(run::BLOCKS);
    provenance["variants"] = json!(
        variant::variants()
            .iter()
            .map(|v| json!({"name":v.name,"stable":v.stable}))
            .collect::<Vec<_>>()
    );
    #[cfg(feature = "compare-native")]
    {
        provenance["dispatch"] = native::dispatch();
    }
    provenance
}
fn setup(o: &Options) -> Result<()> {
    fs::create_dir_all(&o.out)?;
    same_or_write(&o.out.join("host.json"), &host::fingerprint())?;
    let all = suite::cases(&o.suite, None)?;
    let mut provenance = manifest(o, &all);
    if o.command == "memory" {
        provenance.as_object_mut().unwrap().remove("min_sample_ns");
        provenance
            .as_object_mut()
            .unwrap()
            .remove("prepared_cap_bytes");
        same_or_write(
            &o.out.join(format!("memory-manifest-{BUILD}.json")),
            &provenance,
        )?;
    } else {
        let file = if BUILD == "default" {
            "manifest.json".to_owned()
        } else {
            format!("manifest-{BUILD}.json")
        };
        same_or_write(&o.out.join(file), &provenance)?;
    }
    Ok(())
}
fn memory_rows(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    BufReader::new(File::open(path)?)
        .lines()
        .map(|l| Ok(serde_json::from_str(&l?)?))
        .collect()
}
fn block_rows(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path)?;
    let complete = data.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    data[..complete]
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| Ok(serde_json::from_slice(l)?))
        .collect()
}

fn verify_memory_build(o: &Options) -> Result<()> {
    let path = o.out.join(format!("memory-manifest-{BUILD}.json"));
    let memory: Value = serde_json::from_slice(&fs::read(path)?)?;
    let timing: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/provenance.json")))?;
    for field in [
        "schema",
        "source_sha256",
        "rustc",
        "target",
        "profile",
        "features",
        "cargo_encoded_rustflags",
        "native_build",
        "native_sources",
    ] {
        if memory[field] != timing[field] {
            return Err(
                format!("memory executable differs from timing executable: {field}").into(),
            );
        }
    }
    Ok(())
}

fn scratch(c: &Case, rows: &[Value]) -> Option<usize> {
    rows.iter()
        .find(|r| r["case_id"] == c.id() && r["variant"] == "tessera" && r["build"] == BUILD)
        .and_then(|r| r["peak_extra_bytes"].as_u64())
        .map(|n| n as usize)
}
fn validate(cases: &[Case]) -> Result<()> {
    for c in cases {
        let input = variant::Input::new(c);
        for v in variant::variants() {
            if let Some(mut k) = v.prepare(&input, 32_768) {
                run::sample(&mut *k, 1).map_err(|e| format!("{} {}: {e}", c.id(), v.name))?;
            }
        }
    }
    println!("Validated {} cases ({BUILD})", cases.len());
    Ok(())
}
fn measure(o: &Options, cases: &[Case]) -> Result<()> {
    setup(o)?;
    let mem_path = o.out.join("memory.jsonl");
    let mem = memory_rows(&mem_path)?;
    if cases.iter().any(|c| scratch(c, &mem).is_none()) {
        let exe = std::env::current_exe()?.with_file_name("memory");
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--suite", &o.suite, "--out"]).arg(&o.out);
        if let Some(g) = &o.group {
            cmd.args(["--group", g]);
        }
        if !cmd.status()?.success() {
            return Err("memory measurement failed".into());
        }
    }
    verify_memory_build(o)?;
    let mem = memory_rows(&mem_path)?;
    let path = o.out.join("blocks.jsonl");
    let existing = block_rows(&path)?;
    let completed: BTreeSet<_> = existing
        .iter()
        .filter(|r| r["kind"] == "observation" && r["phase"] == "end" && r["build"] == BUILD)
        .filter_map(|r| r["group"].as_str().map(str::to_owned))
        .collect();
    // An interrupted group is replaced as a unit so every retained group has nine complete blocks.
    let retained: Vec<_> = existing
        .into_iter()
        .filter(|r| {
            r["build"] != BUILD || r["group"].as_str().is_some_and(|g| completed.contains(g))
        })
        .collect();
    let temporary = path.with_extension("tmp");
    {
        let mut f = File::create(&temporary)?;
        for r in retained {
            line(&mut f, &r)?;
        }
    }
    fs::rename(temporary, &path)?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let groups: BTreeSet<_> = cases.iter().map(Case::group).collect();
    for group in groups {
        if completed.contains(&group) {
            continue;
        }
        eprintln!("Measuring {group} ({BUILD})");
        let mut start = host::observation(&group, "start");
        start["build"] = json!(BUILD);
        line(&mut f, &start)?;
        for c in cases.iter().filter(|c| c.group() == group) {
            let bytes = scratch(c, &mem)
                .ok_or_else(|| format!("missing Tessera memory measurement: {}", c.id()))?;
            run::measure_case(c, bytes, o.min_ns, o.cap, BUILD, |sample| {
                line(&mut f, &sample)
            })?;
        }
        let mut end = host::observation(&group, "end");
        end["build"] = json!(BUILD);
        line(&mut f, &end)?;
    }
    Ok(())
}

/// Runs the comparison or memory command-line interface.
///
/// Parses process arguments, validates sorts, and writes a resumable results
/// directory. Returns an error for invalid arguments, incompatible provenance,
/// incorrect sort output, or failed I/O.
pub fn main(memory: bool) -> Result<()> {
    #[cfg(feature = "compare-native")]
    native::check_layouts();
    let o = Options::parse(memory)?;
    if o.command == "help" || o.command == "--help" || o.command == "-h" {
        println!(
            "compare <list|validate|run> [--suite smoke|release] [--group TYPE/SHAPE] [--out DIR] [--sample-ms N] [--memory-mib N]"
        );
        return Ok(());
    }
    let cases = suite::cases(&o.suite, o.group.as_deref())?;
    match o.command.as_str() {
        "list" => {
            for v in variant::variants() {
                let supported: Vec<_> = suite::TYPES
                    .into_iter()
                    .filter(|ty| {
                        v.prepare(
                            &variant::Input::new(&Case {
                                ty: (*ty).into(),
                                distribution: "random".into(),
                                len: 32,
                            }),
                            32_768,
                        )
                        .is_some()
                    })
                    .collect();
                println!(
                    "{}\t{}\t{}",
                    v.name,
                    if v.stable { "stable" } else { "unstable" },
                    supported.join(",")
                );
            }
            Ok(())
        }
        "validate" => validate(&cases),
        "run" => measure(&o, &cases),
        "memory" => memory_measure(&o, &cases),
        _ => Err(format!("unknown command {}", o.command).into()),
    }
}
fn memory_measure(o: &Options, cases: &[Case]) -> Result<()> {
    setup(o)?;
    let path = o.out.join("memory.jsonl");
    let mut rows = memory_rows(&path)?;
    for c in cases {
        let input = variant::Input::new(c);
        for v in variant::variants() {
            if c.len > 1_000_000 && c.ty != "u64" && v.name != "tessera" {
                continue;
            }
            if rows
                .iter()
                .any(|r| r["case_id"] == c.id() && r["variant"] == v.name && r["build"] == BUILD)
            {
                continue;
            }
            if let Some(mut k) = v.prepare(&input, scratch(c, &rows).unwrap_or(0)) {
                k.restore(1);
                super::allocation::begin();
                k.run_batch();
                let peak = super::allocation::end();
                k.check()
                    .map_err(|e| format!("{} {}: {e}", c.id(), v.name))?;
                host::check_threads()?;
                rows.push(json!({"case_id":c.id(),"variant":v.name,"build":BUILD,"peak_extra_bytes":peak,"verified":true}));
            }
        }
    }
    let temporary = path.with_extension("tmp");
    {
        let mut f = File::create(&temporary)?;
        for r in rows {
            line(&mut f, &r)?;
        }
    }
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn journal_recovers_a_truncated_tail_but_rejects_a_corrupt_complete_line() {
        let path =
            std::env::temp_dir().join(format!("tessera-journal-{}.jsonl", std::process::id()));
        std::fs::write(&path, b"{\"kind\":\"observation\"}\n{\"partial\":").unwrap();
        assert_eq!(super::block_rows(&path).unwrap().len(), 1);
        std::fs::write(&path, b"{broken}\n").unwrap();
        assert!(super::block_rows(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
