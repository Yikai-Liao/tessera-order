// SPDX-License-Identifier: BSD-2-Clause

//! Command orchestration and persistent comparison results.

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

/// Fallible comparison commands and result persistence.
type Result<T> = std::result::Result<T, Box<dyn Error>>;
/// Feature configuration named in every measurement.
const BUILD: &str = if cfg!(feature = "portable-simd") {
    "portable-simd"
} else {
    "default"
};

/// Command selection, output location and sampling limits.
struct Options {
    command: String,
    suite: String,
    group: Option<String>,
    output_dir: PathBuf,
    min_sample_ns: u64,
    prepared_cap_bytes: usize,
}
impl Options {
    /// Parses the executable arguments into one comparison command.
    fn parse(memory: bool) -> Result<Self> {
        let mut args = std::env::args().skip(1);
        let command = if memory {
            "memory".into()
        } else {
            args.next().unwrap_or_else(|| "help".into())
        };
        let mut options = Self {
            command,
            suite: "smoke".into(),
            group: None,
            output_dir: "bench/results/local".into(),
            min_sample_ns: 5_000_000,
            prepared_cap_bytes: 256 << 20,
        };
        while let Some(key) = args.next() {
            if key == "--help" || key == "-h" {
                options.command = "help".into();
                return Ok(options);
            }
            let value = args
                .next()
                .ok_or_else(|| format!("missing value for {key}"))?;
            match key.as_str() {
                "--suite" => options.suite = value,
                "--group" => options.group = Some(value),
                "--out" => options.output_dir = value.into(),
                "--sample-ms" => {
                    options.min_sample_ns = value
                        .parse::<u64>()?
                        .checked_mul(1_000_000)
                        .filter(|&n| n > 0)
                        .ok_or("sample duration must be positive")?
                }
                "--memory-mib" => {
                    options.prepared_cap_bytes = value
                        .parse::<usize>()?
                        .checked_mul(1 << 20)
                        .filter(|&n| n > 0)
                        .ok_or("memory cap must be positive")?
                }
                _ => return Err(format!("unknown option {key}").into()),
            }
        }
        Ok(options)
    }
}
/// Writes new provenance or checks an existing run for compatibility.
fn verify_or_write_json(path: &Path, value: &Value) -> Result<()> {
    if path.exists() {
        let old: Value = serde_json::from_slice(&fs::read(path)?)?;
        if old != *value {
            return Err(format!("{} differs from this run", path.display()).into());
        }
    } else {
        fs::write(path, serde_json::to_vec_pretty(value)?)?;
    }
    Ok(())
}
/// Appends and flushes one complete JSONL record.
fn append_record(file: &mut File, value: &impl serde::Serialize) -> Result<()> {
    serde_json::to_writer(&mut *file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
/// Combines build provenance with the suite and adapter capabilities.
fn create_manifest(options: &Options, cases: &[Case]) -> Value {
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
    provenance["suite"] = json!(options.suite);
    provenance["suite_version"] = json!(1);
    provenance["cases"] = json!(
        cases
            .iter()
            .map(|case| {
                let probe = Case {
                    len: 32,
                    distribution: "random".into(),
                    ..case.clone()
                };
                let input = variant::CaseInput::new(&probe);
                let supported: Vec<_> = variant::list_variants()
                    .iter()
                    .filter(|v| v.prepare(&input, 0).is_some())
                    .map(|v| v.name)
                    .collect();
                json!({"ty":case.ty,"distribution":case.distribution,"len":case.len,"variants":supported})
            })
            .collect::<Vec<_>>()
    );
    provenance["build"] = json!(BUILD);
    provenance["min_sample_ns"] = json!(options.min_sample_ns);
    provenance["prepared_cap_bytes"] = json!(options.prepared_cap_bytes);
    provenance["blocks"] = json!(run::BLOCKS);
    provenance["variants"] = json!(
        variant::list_variants()
            .iter()
            .map(|v| json!({"name":v.name,"stable":v.stable}))
            .collect::<Vec<_>>()
    );
    #[cfg(feature = "compare-native")]
    {
        provenance["dispatch"] = native::read_dispatch_targets();
    }
    provenance
}
/// Checks the host and executable provenance before writing results.
fn prepare_output(options: &Options) -> Result<()> {
    fs::create_dir_all(&options.output_dir)?;
    verify_or_write_json(
        &options.output_dir.join("host.json"),
        &host::read_fingerprint(),
    )?;
    let all = suite::plan_cases(&options.suite, None)?;
    let mut provenance = create_manifest(options, &all);
    if options.command == "memory" {
        provenance.as_object_mut().unwrap().remove("min_sample_ns");
        provenance
            .as_object_mut()
            .unwrap()
            .remove("prepared_cap_bytes");
        verify_or_write_json(
            &options
                .output_dir
                .join(format!("memory-manifest-{BUILD}.json")),
            &provenance,
        )?;
    } else {
        let file = if BUILD == "default" {
            "manifest.json".to_owned()
        } else {
            format!("manifest-{BUILD}.json")
        };
        verify_or_write_json(&options.output_dir.join(file), &provenance)?;
    }
    Ok(())
}
/// Reads completed allocation measurements from a results directory.
fn read_memory_samples(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    BufReader::new(File::open(path)?)
        .lines()
        .map(|l| Ok(serde_json::from_str(&l?)?))
        .collect()
}
/// Reads complete journal lines, leaving an interrupted final write out.
fn read_block_records(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let journal_bytes = fs::read(path)?;
    let complete = journal_bytes
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    journal_bytes[..complete]
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| Ok(serde_json::from_slice(l)?))
        .collect()
}

/// Checks that scratch sizes were measured by the same source and configuration.
fn verify_memory_build(options: &Options) -> Result<()> {
    let path = options
        .output_dir
        .join(format!("memory-manifest-{BUILD}.json"));
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

/// Finds Tessera's measured peak allocation size for a case.
fn find_scratch_bytes(case: &Case, rows: &[Value]) -> Option<usize> {
    rows.iter()
        .find(|r| r["case_id"] == case.id() && r["variant"] == "tessera" && r["build"] == BUILD)
        .and_then(|r| r["peak_extra_bytes"].as_u64())
        .map(|n| n as usize)
}
/// Checks every supported adapter against each case's shared reference.
fn validate_cases(cases: &[Case]) -> Result<()> {
    for case in cases {
        let input = variant::CaseInput::new(case);
        for v in variant::list_variants() {
            if let Some(mut k) = v.prepare(&input, 32_768) {
                run::measure_batch(&mut *k, 1)
                    .map_err(|e| format!("{} {}: {e}", case.id(), v.name))?;
            }
        }
    }
    println!("Validated {} cases ({BUILD})", cases.len());
    Ok(())
}
/// Resumes completed groups and records the missing paired measurements.
fn measure_cases(options: &Options, cases: &[Case]) -> Result<()> {
    prepare_output(options)?;
    let mem_path = options.output_dir.join("memory.jsonl");
    let mem = read_memory_samples(&mem_path)?;
    if cases
        .iter()
        .any(|case| find_scratch_bytes(case, &mem).is_none())
    {
        let exe = std::env::current_exe()?.with_file_name("memory");
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["--suite", &options.suite, "--out"])
            .arg(&options.output_dir);
        if let Some(g) = &options.group {
            cmd.args(["--group", g]);
        }
        if !cmd.status()?.success() {
            return Err("memory measurement failed".into());
        }
    }
    verify_memory_build(options)?;
    let mem = read_memory_samples(&mem_path)?;
    let path = options.output_dir.join("blocks.jsonl");
    let existing = read_block_records(&path)?;
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
            append_record(&mut f, &r)?;
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
        let mut start = host::observe_group(&group, "start");
        start["build"] = json!(BUILD);
        append_record(&mut f, &start)?;
        for case in cases.iter().filter(|case| case.group() == group) {
            let bytes = find_scratch_bytes(case, &mem)
                .ok_or_else(|| format!("missing Tessera memory measurement: {}", case.id()))?;
            run::measure_case(
                case,
                bytes,
                options.min_sample_ns,
                options.prepared_cap_bytes,
                BUILD,
                |sample| append_record(&mut f, &sample),
            )?;
        }
        let mut end = host::observe_group(&group, "end");
        end["build"] = json!(BUILD);
        append_record(&mut f, &end)?;
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
    let options = Options::parse(memory)?;
    if options.command == "help" || options.command == "--help" || options.command == "-h" {
        let command = if memory {
            "memory"
        } else {
            "compare <list|validate|run>"
        };
        println!(
            "{command} [--suite smoke|release] [--group TYPE/SHAPE] [--out DIR] [--sample-ms N] [--memory-mib N]"
        );
        return Ok(());
    }
    let cases = suite::plan_cases(&options.suite, options.group.as_deref())?;
    match options.command.as_str() {
        "list" => {
            for v in variant::list_variants() {
                let supported: Vec<_> = suite::TYPES
                    .into_iter()
                    .filter(|ty| {
                        v.prepare(
                            &variant::CaseInput::new(&Case {
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
        "validate" => validate_cases(&cases),
        "run" => measure_cases(&options, &cases),
        "memory" => measure_memory(&options, &cases),
        _ => Err(format!("unknown command {}", options.command).into()),
    }
}
/// Measures one isolated sort allocation peak per supported case and adapter.
fn measure_memory(options: &Options, cases: &[Case]) -> Result<()> {
    prepare_output(options)?;
    let path = options.output_dir.join("memory.jsonl");
    let mut rows = read_memory_samples(&path)?;
    for case in cases {
        let input = variant::CaseInput::new(case);
        for v in variant::list_variants() {
            if case.len > 1_000_000 && case.ty != "u64" && v.name != "tessera" {
                continue;
            }
            if rows
                .iter()
                .any(|r| r["case_id"] == case.id() && r["variant"] == v.name && r["build"] == BUILD)
            {
                continue;
            }
            if let Some(mut k) = v.prepare(&input, find_scratch_bytes(case, &rows).unwrap_or(0)) {
                k.restore(1);
                super::allocation::begin();
                k.run_batch();
                let peak = super::allocation::end();
                k.check()
                    .map_err(|e| format!("{} {}: {e}", case.id(), v.name))?;
                host::check_threads()?;
                rows.push(json!({"case_id":case.id(),"variant":v.name,"build":BUILD,"peak_extra_bytes":peak,"verified":true}));
            }
        }
    }
    let temporary = path.with_extension("tmp");
    {
        let mut f = File::create(&temporary)?;
        for r in rows {
            append_record(&mut f, &r)?;
        }
    }
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn journal_recovers_truncated_tail() {
        let path =
            std::env::temp_dir().join(format!("tessera-journal-{}.jsonl", std::process::id()));
        std::fs::write(&path, b"{\"kind\":\"observation\"}\n{\"partial\":").unwrap();
        assert_eq!(super::read_block_records(&path).unwrap().len(), 1);
        std::fs::write(&path, b"{broken}\n").unwrap();
        assert!(super::read_block_records(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
