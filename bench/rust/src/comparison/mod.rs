// SPDX-License-Identifier: BSD-2-Clause

//! Command orchestration and persistent comparison results.

mod check;
mod host;
#[cfg(feature = "compare-native")]
mod native;
mod record;
mod run;
mod variant;

pub use record::CaseInput;
pub use variant::{Kernel, Variant, list_variants};

use crate::plan::{InputCase, catalog};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Fallible comparison commands and result persistence.
type Result<T> = std::result::Result<T, Box<dyn Error>>;
/// The single current build; manifests retain actual features for provenance.
const BUILD: &str = "default";

/// Command selection, output location and sampling limits.
struct Options {
    command: String,
    suite: String,
    group: Option<String>,
    plan: crate::plan::Plan,
    plan_path: Option<PathBuf>,
    output_dir: PathBuf,
    sampling: run::Sampling,
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
            plan: crate::plan::Plan::parse(include_str!("../../../plans/smoke.toml"))?,
            plan_path: None,
            output_dir: "bench/results/local".into(),
            sampling: run::Sampling {
                min_sample_ns: 5_000_000,
                prepared_cap_bytes: 256 << 20,
                blocks: catalog().paired_blocks.extended.count,
            },
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
                "--plan" => options.plan_path = Some(value.into()),
                "--out" => options.output_dir = value.into(),
                "--blocks" => {
                    let count = value.parse::<usize>()?;
                    let budgets = &catalog().paired_blocks;
                    if count != budgets.short.count && count != budgets.extended.count {
                        return Err(format!("unsupported paired-block budget: {count}").into());
                    }
                    options.sampling.blocks = count;
                }
                "--sample-ms" => {
                    options.sampling.min_sample_ns = value
                        .parse::<u64>()?
                        .checked_mul(1_000_000)
                        .filter(|&n| n > 0)
                        .ok_or("sample duration must be positive")?
                }
                "--memory-mib" => {
                    options.sampling.prepared_cap_bytes = value
                        .parse::<usize>()?
                        .checked_mul(1 << 20)
                        .filter(|&n| n > 0)
                        .ok_or("memory cap must be positive")?
                }
                _ => return Err(format!("unknown option {key}").into()),
            }
        }
        let text = if let Some(path) = &options.plan_path {
            options.suite = "custom".into();
            fs::read_to_string(path)?
        } else {
            match options.suite.as_str() {
                "smoke" => include_str!("../../../plans/smoke.toml").into(),
                "release" => include_str!("../../../plans/release.toml").into(),
                _ => return Err("expected smoke or release suite".into()),
            }
        };
        options.plan = crate::plan::Plan::parse(&text)?;
        if options.command == "run"
            && ["tessera", "std-stable"].iter().any(|id| {
                !options
                    .plan
                    .algorithms
                    .iter()
                    .any(|selected| selected == id)
            })
        {
            return Err("paired algorithm runs require tessera and std-stable controls".into());
        }
        if options.plan.has_rule_operations() {
            return Err("algorithm comparisons take one plan-level operation; rule operations are for revision suites".into());
        }
        if !options.plan.operation().is_sort() && !cfg!(feature = "operations") {
            return Err("non-sort workloads require the operations benchmark feature; the library APIs are always available".into());
        }
        let available = variant::list_variants();
        for id in &options.plan.algorithms {
            if BUILD == "default"
                && options.plan_path.is_some()
                && !available.iter().any(|entry| entry.name == id)
            {
                return Err(format!("algorithm {id} is unavailable; enable its compare-native/rayon feature or use tools compare revisions for revision adapters").into());
            }
        }
        Ok(options)
    }

    /// Selects all supported cases before applying a resumable group restriction.
    fn cases(&self, group: Option<&str>) -> Result<Vec<InputCase>> {
        // Ratios require a common cohort, including separately selected parallel inputs.
        let cases: Vec<_> = self
            .plan
            .inputs()?
            .into_iter()
            .chain(self.plan.parallel_inputs()?)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|case| group.is_none_or(|group| case.group() == group))
            .collect();
        if cases.is_empty() {
            return Err("the benchmark plan selects no supported cases".into());
        }
        Ok(cases)
    }

    /// Retains the selected algorithms available in this build configuration.
    fn variants(&self) -> Vec<variant::Variant> {
        variant::list_variants()
            .into_iter()
            .filter(|entry| self.plan.algorithms.iter().any(|id| id == entry.name))
            .collect()
    }
}
/// Holds a kernel lock until this worker exits, including after its Node parent dies.
///
/// The comparison and memory workers lock their own roles. Before touching timing
/// journals, the comparison worker always waits for its resumable memory child.
/// An orphaned memory child therefore blocks recovery before timing can resume.
fn lock_worker(directory: &Path, role: &str) -> Result<File> {
    fs::create_dir_all(directory)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(format!(".{role}-lock")))?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
        format!(
            "{role} worker still active in {}: {error}",
            directory.display()
        )
    })?;
    Ok(file)
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
fn create_manifest(options: &Options, cases: &[InputCase]) -> Value {
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
    provenance["suite_version"] = json!(if options.plan.operation().is_sort() {
        1
    } else {
        2
    });
    if !options.plan.operation().is_sort() {
        provenance["index_bytes"] = json!(size_of::<usize>());
    }
    provenance["plan"] = json!(options.plan);
    provenance["cases"] = json!(
        cases
            .iter()
            .map(|case| {
                let probe = case.probe();
                let input = record::CaseInput::new(&probe);
                let supported: Vec<_> = options.variants()
                    .iter()
                    .filter(|v| v.supports(&input))
                    .map(|v| v.name)
                    .collect();
                let mut entry = json!(case);
                entry["variants"] = json!(supported);
                if !case.operation.is_sort() {
                    entry["implementations"] = json!(options.variants().iter()
                        .filter_map(|v| v.prepare(&input, 0).map(|k| (v.name, k.description())))
                        .collect::<std::collections::BTreeMap<_, _>>());
                    entry["omitted"] = json!(options.variants().iter()
                        .filter(|v| !supported.contains(&v.name))
                        .map(|v| json!({"variant":v.name,"reason":"unsupported operation/input combination"}))
                        .collect::<Vec<_>>());
                }
                #[cfg(tessera_bench_revisions)]
                let entry = {
                    let mut entry = entry;
                    entry["revision_entry_offsets"] = json!(input.revision_entries());
                    entry
                };
                entry
            })
            .collect::<Vec<_>>()
    );
    provenance["build"] = json!(BUILD);
    provenance["min_sample_ns"] = json!(options.sampling.min_sample_ns);
    provenance["prepared_cap_bytes"] = json!(options.sampling.prepared_cap_bytes);
    provenance["blocks"] = json!(options.sampling.blocks);
    provenance["variants"] = json!(
        options
            .variants()
            .iter()
            .map(
                |v| json!({"name":v.name,"stable":v.stable,"adapter":v.adapter,"threads":v.threads})
            )
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
    let all = options.cases(None)?;
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

/// Removes incomplete writes before resuming an append-only journal.
fn rewrite_journal(path: &Path, records: &[Value]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    {
        let mut file = File::create(&temporary)?;
        for record in records {
            append_record(&mut file, record)?;
        }
    }
    fs::rename(temporary, path)?;
    Ok(())
}

/// Checks that scratch sizes were measured by the same source and configuration.
fn verify_memory_build(directory: &Path) -> Result<()> {
    let path = directory.join(format!("memory-manifest-{BUILD}.json"));
    let memory: Value = serde_json::from_slice(&fs::read(path)?)?;
    let timing: Value =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/provenance.json")))?;
    for field in [
        "schema",
        "source_sha256",
        "rustc",
        "target",
        "profile",
        "opt_level",
        "debug",
        "features",
        "cargo_encoded_rustflags",
        "cc",
        "cxx",
        "native_build",
        "native_sources",
        "revisions",
    ] {
        if memory[field] != timing[field] {
            return Err(
                format!("memory executable differs from timing executable: {field}").into(),
            );
        }
    }
    Ok(())
}

/// Loads exact-case Tessera scratch budgets from the normal memory worker.
/// Rejects incompatible builds, unverified rows and duplicate case measurements.
///
/// # Errors
/// Returns an error for invalid evidence, incompatible provenance or failed I/O.
pub fn read_scratch(directory: &Path) -> Result<std::collections::BTreeMap<String, usize>> {
    verify_memory_build(directory)?;
    let mut scratch = std::collections::BTreeMap::new();
    for row in read_block_records(&directory.join("memory.jsonl"))? {
        if row["variant"] != "tessera" || row["build"] != BUILD {
            continue;
        }
        let id = row["case_id"]
            .as_str()
            .ok_or("missing memory case identity")?;
        let bytes: usize = row["peak_extra_bytes"]
            .as_u64()
            .ok_or("invalid scratch bytes")?
            .try_into()?;
        if row["verified"] != true || scratch.insert(id.to_owned(), bytes).is_some() {
            return Err("unverified or duplicate Tessera memory measurement".into());
        }
    }
    Ok(scratch)
}

/// Finds Tessera's measured peak allocation size for a case.
fn find_scratch_bytes(case: &InputCase, rows: &[Value]) -> Option<usize> {
    rows.iter()
        .find(|r| r["case_id"] == case.id() && r["variant"] == "tessera" && r["build"] == BUILD)
        .and_then(|r| r["peak_extra_bytes"].as_u64())
        .map(|n| n as usize)
}
/// Checks every supported adapter against each case's shared reference.
fn validate_cases(options: &Options, cases: &[InputCase]) -> Result<()> {
    for (index, case) in cases.iter().enumerate() {
        eprintln!(
            "Validating {}/{}: {} ({BUILD})",
            index + 1,
            cases.len(),
            case.id()
        );
        let input = record::CaseInput::new(case);
        for v in options.variants() {
            if let Some(mut k) = v.prepare(&input, 32_768) {
                k.measure_batch(1)
                    .map_err(|e| format!("{} {}: {e}", case.id(), v.name))?;
            }
        }
    }
    println!("Validated {} cases ({BUILD})", cases.len());
    Ok(())
}
/// Resumes completed groups and records the missing paired measurements.
fn measure_cases(options: &Options, cases: &[InputCase]) -> Result<()> {
    prepare_output(options)?;
    // The memory worker owns completeness for every adapter, not just Tessera.
    // It repairs an interrupted write and skips every row already committed.
    let exe = std::env::current_exe()?.with_file_name("memory");
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--out").arg(&options.output_dir);
    cmd.arg("--blocks").arg(options.sampling.blocks.to_string());
    if let Some(path) = &options.plan_path {
        cmd.arg("--plan").arg(path);
    } else {
        cmd.args(["--suite", &options.suite]);
    }
    if let Some(g) = &options.group {
        cmd.args(["--group", g]);
    }
    if !cmd.status()?.success() {
        return Err("memory measurement failed".into());
    }
    // Keep direct memory invocations out while timing and journals are active.
    let _memory_lock = lock_worker(&options.output_dir, "memory")?;
    let scratch = read_scratch(&options.output_dir)?;
    let path = options.output_dir.join("blocks.jsonl");
    let existing = read_block_records(&path)?;
    let completed: BTreeSet<_> = existing
        .iter()
        .filter(|r| r["kind"] == "observation" && r["phase"] == "end" && r["build"] == BUILD)
        .filter_map(|r| r["group"].as_str().map(str::to_owned))
        .collect();
    // Replace an interrupted group as a unit; retained groups have their full declared budget.
    let retained: Vec<_> = existing
        .into_iter()
        .filter(|r| {
            r["build"] != BUILD || r["group"].as_str().is_some_and(|g| completed.contains(g))
        })
        .collect();
    rewrite_journal(&path, &retained)?;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    let groups: BTreeSet<_> = cases.iter().map(InputCase::group).collect();
    for group in groups {
        if completed.contains(&group) {
            continue;
        }
        eprintln!("Measuring {group} ({BUILD})");
        let mut start = host::observe_group(&group, "start");
        start["build"] = json!(BUILD);
        append_record(&mut f, &start)?;
        for case in cases.iter().filter(|case| case.group() == group) {
            let bytes = *scratch
                .get(&case.id())
                .ok_or_else(|| format!("missing Tessera memory measurement: {}", case.id()))?;
            run::measure_case(
                case,
                &options.variants(),
                bytes,
                &options.sampling,
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
            "{command} [--suite smoke|release | --plan FILE] [--group TYPE/SHAPE] [--out DIR] [--sample-ms N] [--memory-mib N] [--blocks 3|9]"
        );
        return Ok(());
    }
    let cases = options.cases(options.group.as_deref())?;
    if options.command == "run" {
        // The manifest includes every case, even for a restricted group. Keep
        // required-control admission with native capabilities, not a TS limit.
        run::validate_controls(&options.cases(None)?, &options.variants())?;
    }
    match options.command.as_str() {
        "list" => {
            for v in variant::list_variants() {
                let supported: Vec<_> = catalog()
                    .types
                    .iter()
                    .map(|ty| ty.id.as_str())
                    .filter(|ty| {
                        v.supports(&record::CaseInput::new(&InputCase {
                            ty: (*ty).into(),
                            distribution: "random".into(),
                            len: 32,
                            operation: options.plan.operation().probe(),
                        }))
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
        "validate" | "run" => {
            let _lock = lock_worker(&options.output_dir, "comparison")?;
            if options.command == "validate" {
                validate_cases(&options, &cases)
            } else {
                measure_cases(&options, &cases)
            }
        }
        "memory" => {
            let _lock = lock_worker(&options.output_dir, "memory")?;
            measure_memory(&options, &cases)
        }
        _ => Err(format!("unknown command {}", options.command).into()),
    }
}
/// Measures one isolated sort allocation peak per supported case and adapter.
/// Largest input whose opponents' memory is measured. Linear-memory opponents need
/// as much again, so the 10M-record 32- and 128-byte release inputs are measured for
/// Tessera only, which sizes glidesort-sqrt; 10M u64 keys and 16M 8-byte rows fit.
const OPPONENT_MEMORY_INPUT_BYTES: usize = 128 << 20;

fn measure_memory(options: &Options, cases: &[InputCase]) -> Result<()> {
    prepare_output(options)?;
    let path = options.output_dir.join("memory.jsonl");
    let mut rows = read_block_records(&path)?;
    rewrite_journal(&path, &rows)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    for (index, case) in cases.iter().enumerate() {
        eprintln!(
            "Measuring memory {}/{}: {} ({BUILD})",
            index + 1,
            cases.len(),
            case.id()
        );
        let input = record::CaseInput::new(case);
        for v in options.variants() {
            if case.operation.is_sort()
                && case.input_bytes() > OPPONENT_MEMORY_INPUT_BYTES
                && v.name != "tessera"
                && v.adapter != "tessera-par"
            {
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
                host::check_threads(k.worker_threads())?;
                let mut row = json!({"case_id":case.id(),"variant":v.name,"build":BUILD,"peak_extra_bytes":peak,"verified":true});
                if !case.operation.is_sort() {
                    row["output_bytes"] = json!(k.output_bytes());
                }
                append_record(&mut file, &row)?;
                rows.push(row);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    /// Runs the test binary as either an orchestrator or its independent worker.
    fn lock_fixture_command(directory: &std::path::Path, role: &str, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "comparison::tests::worker_lock_fixture",
                "--ignored",
            ])
            .env("TESSERA_LOCK_DIRECTORY", directory)
            .env("TESSERA_LOCK_ROLE", role)
            .env("TESSERA_LOCK_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    #[test]
    #[ignore = "subprocess fixture for worker lock ownership"]
    fn worker_lock_fixture() {
        let directory =
            std::path::PathBuf::from(std::env::var_os("TESSERA_LOCK_DIRECTORY").unwrap());
        let role = std::env::var("TESSERA_LOCK_ROLE").unwrap();
        if std::env::var("TESSERA_LOCK_MODE").unwrap() == "orchestrator" {
            let mut child = lock_fixture_command(&directory, &role, "worker")
                .spawn()
                .unwrap();
            // A deadline keeps a failed parent test from leaking the fixture.
            thread::sleep(Duration::from_secs(20));
            let _ = child.kill();
            let _ = child.wait();
        } else {
            let _lock = super::lock_worker(&directory, &role).unwrap();
            std::fs::write(directory.join("ready"), b"ready").unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while !directory.join("release").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn orphaned_workers_exclude_resumes() {
        for role in ["comparison", "memory"] {
            let directory =
                std::env::temp_dir().join(format!("tessera-orphan-{}-{role}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            let mut orchestrator = lock_fixture_command(&directory, role, "orchestrator")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !directory.join("ready").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            let ready = directory.join("ready").exists();
            orchestrator.kill().unwrap();
            orchestrator.wait().unwrap();
            assert!(ready, "worker did not acquire the lock");
            assert!(
                super::lock_worker(&directory, role).is_err(),
                "orphaned {role} worker lost its lock"
            );
            std::fs::write(directory.join("release"), b"release").unwrap();
            while super::lock_worker(&directory, role).is_err() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(super::lock_worker(&directory, role).is_ok());
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn memory_evidence_checks_configuration_and_rows() {
        let output_dir = std::env::temp_dir().join(format!(
            "tessera-memory-build-{}-{}",
            std::process::id(),
            super::BUILD
        ));
        std::fs::create_dir_all(&output_dir).unwrap();
        let path = output_dir.join(format!("memory-manifest-{}.json", super::BUILD));
        let timing: serde_json::Value =
            serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/provenance.json")))
                .unwrap();
        std::fs::write(&path, serde_json::to_vec(&timing).unwrap()).unwrap();
        super::verify_memory_build(&output_dir).unwrap();
        for field in ["opt_level", "debug", "cc", "cxx"] {
            let mut memory = timing.clone();
            memory[field] = serde_json::json!("different");
            std::fs::write(&path, serde_json::to_vec(&memory).unwrap()).unwrap();
            assert_eq!(
                super::verify_memory_build(&output_dir)
                    .unwrap_err()
                    .to_string(),
                format!("memory executable differs from timing executable: {field}")
            );
        }
        std::fs::write(&path, serde_json::to_vec(&timing).unwrap()).unwrap();
        let journal = output_dir.join("memory.jsonl");
        let row = serde_json::json!({"case_id":"u64/random/49/s1", "variant":"tessera", "build":super::BUILD, "verified":true, "peak_extra_bytes":123});
        std::fs::write(&journal, format!("{row}\n")).unwrap();
        assert_eq!(
            super::read_scratch(&output_dir).unwrap()["u64/random/49/s1"],
            123
        );
        std::fs::write(&journal, format!("{row}\n{row}\n")).unwrap();
        assert!(super::read_scratch(&output_dir).is_err());
        let mut invalid = row;
        invalid["verified"] = serde_json::json!(false);
        std::fs::write(&journal, format!("{invalid}\n")).unwrap();
        assert!(super::read_scratch(&output_dir).is_err());
        std::fs::remove_dir_all(&output_dir).unwrap();
    }

    #[test]
    fn journal_recovers_truncated_tail() {
        let path =
            std::env::temp_dir().join(format!("tessera-journal-{}.jsonl", std::process::id()));
        std::fs::write(&path, b"{\"kind\":\"observation\"}\n{\"partial\":").unwrap();
        let records = super::read_block_records(&path).unwrap();
        assert_eq!(records.len(), 1);
        super::rewrite_journal(&path, &records).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        super::append_record(&mut file, &serde_json::json!({"kind": "memory"})).unwrap();
        assert_eq!(super::read_block_records(&path).unwrap().len(), 2);
        std::fs::write(&path, b"{broken}\n").unwrap();
        assert!(super::read_block_records(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
