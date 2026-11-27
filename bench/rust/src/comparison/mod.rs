// SPDX-License-Identifier: BSD-2-Clause

//! Command orchestration and persistent comparison results.

mod check;
mod go;
mod host;
#[cfg(feature = "compare-native")]
mod native;
mod record;
mod run;
mod variant;

pub use record::CaseInput;
pub use variant::{Kernel, Variant, list_variants};

use crate::plan::{Execution, InputCase, Operation, catalog};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
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
    plan_name: String,
    group: Option<String>,
    plan: crate::plan::Plan,
    plan_path: Option<PathBuf>,
    output_dir: PathBuf,
    sampling: run::Sampling,
    go_worker: Option<PathBuf>,
    go_build: Option<Value>,
    go_routes: BTreeMap<(String, &'static str, Execution), Value>,
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
            plan_name: "smoke".into(),
            group: None,
            plan: crate::plan::Plan::parse(include_str!("../../../plans/smoke.toml"))?,
            plan_path: None,
            output_dir: "bench/results/local".into(),
            go_worker: None,
            go_build: None,
            go_routes: BTreeMap::new(),
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
                "--group" => options.group = Some(value),
                "--plan" if matches!(value.as_str(), "smoke" | "release") => {
                    options.plan_name = value
                }
                "--plan" => options.plan_path = Some(value.into()),
                "--go-worker" => options.go_worker = Some(fs::canonicalize(value)?),
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
            options.plan_name = "custom".into();
            fs::read_to_string(path)?
        } else {
            match options.plan_name.as_str() {
                "smoke" => include_str!("../../../plans/smoke.toml").into(),
                "release" => include_str!("../../../plans/release.toml").into(),
                _ => return Err("expected smoke or release plan".into()),
            }
        };
        options.plan = crate::plan::Plan::parse(&text)?;
        let operations: BTreeSet<_> = options
            .plan
            .cases()?
            .into_iter()
            .map(|(case, _)| case.operation)
            .collect();
        if operations.len() != 1 {
            return Err("an implementation comparison measures one operation per session; split the plan by operation".into());
        }
        let reference =
            crate::plan::operation_reference(operations.first().expect("one operation"));
        if options.command == "run"
            && ["tessera", reference].iter().any(|id| {
                !options
                    .plan
                    .implementations
                    .iter()
                    .any(|selected| selected == id)
            })
        {
            return Err(format!(
                "paired implementation runs require tessera and {reference} controls"
            )
            .into());
        }
        if !options.operation().is_sort() && !cfg!(feature = "operations") {
            return Err("non-sort operations require the operations benchmark feature; the library APIs are always available".into());
        }
        let available = variant::list_variants()
            .into_iter()
            .chain(variant::go_variants().filter(|_| options.go_worker.is_some()))
            .collect::<Vec<_>>();
        for id in &options.plan.implementations {
            if BUILD == "default"
                && options.plan_path.is_some()
                && !available.iter().any(|entry| entry.name == id)
            {
                return Err(format!("implementation {id} is unavailable; enable its compare-native/rayon feature or use tools compare revisions for revision adapters").into());
            }
        }
        Ok(options)
    }

    /// The session's single operation, checked by [`Self::parse`].
    fn operation(&self) -> Operation {
        let (case, _) = &self.plan.cases().expect("validated plan")[0];
        case.operation.clone()
    }

    /// Selects all cases before applying a resumable group restriction.
    fn cases(&self, group: Option<&str>) -> Result<Vec<(InputCase, Vec<Execution>)>> {
        let cases: Vec<_> = self
            .plan
            .cases()?
            .into_iter()
            .filter(|(case, _)| group.is_none_or(|group| case.group() == group))
            .collect();
        if cases.is_empty() {
            return Err("the benchmark plan selects no supported cases".into());
        }
        Ok(cases)
    }

    /// Planned columns in catalog and execution order, before capability checks.
    fn planned_columns(
        &self,
        executions: &[Execution],
    ) -> impl Iterator<Item = (Variant, Execution)> {
        self.variants()
            .into_iter()
            .flat_map(|v| executions.iter().map(move |&e| (v, e)))
    }

    fn supported_columns(
        &self,
        case: &InputCase,
        executions: &[Execution],
    ) -> Vec<(Variant, Execution)> {
        let probe = record::CaseInput::new(&case.probe());
        self.planned_columns(executions)
            .filter(|(v, e)| self.route(case, &probe, *v, *e).0.is_some())
            .collect()
    }

    fn discover_go(&mut self) -> Result<()> {
        let variants: Vec<_> = self
            .variants()
            .into_iter()
            .filter(|v| v.adapter == "go")
            .collect();
        if variants.is_empty() {
            return Ok(());
        }
        use sha2::{Digest, Sha256};
        let binary = self
            .go_worker
            .as_ref()
            .ok_or("missing Go comparison worker")?;
        let mut worker = go::Worker::start(binary, Execution::Serial, None)?;
        self.go_build = Some(json!({
            "binary_sha256": format!("{:x}", Sha256::digest(fs::read(binary)?)),
            "build_info": worker.request(&json!({"action":"info"}))?,
        }));
        for (case, executions) in self.cases(None)? {
            for v in &variants {
                for &execution in &executions {
                    let route = worker.case("list", v.name, &case, execution)?;
                    match (route["route"].as_str(), route["decline"].as_str()) {
                        (Some(value), None) | (None, Some(value)) if !value.is_empty() => {}
                        _ => return Err("invalid Go capability response".into()),
                    }
                    self.go_routes.insert((case.id(), v.name, execution), route);
                }
            }
        }
        Ok(())
    }

    fn route(
        &self,
        case: &InputCase,
        probe: &record::CaseInput,
        variant: Variant,
        execution: Execution,
    ) -> (Option<&str>, &str) {
        if variant.adapter == "go" {
            let capability = &self.go_routes[&(case.id(), variant.name, execution)];
            return (
                capability["route"].as_str(),
                capability["decline"].as_str().unwrap_or(""),
            );
        }
        (
            variant.route(probe, execution),
            "unsupported operation/input/execution combination",
        )
    }

    /// Retains the selected implementations available in this build configuration.
    fn variants(&self) -> Vec<variant::Variant> {
        variant::list_variants()
            .into_iter()
            .chain(variant::go_variants().filter(|_| self.go_worker.is_some()))
            .filter(|entry| self.plan.implementations.iter().any(|id| id == entry.name))
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
/// Combines build provenance with the plan and adapter capabilities.
fn create_manifest(options: &Options, cases: &[(InputCase, Vec<Execution>)]) -> Value {
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
    provenance["plan_name"] = json!(options.plan_name);
    let operation = options.operation();
    if !operation.is_sort() {
        provenance["index_bytes"] = json!(size_of::<usize>());
    }
    provenance["plan"] = json!(options.plan);
    if let Some(build) = &options.go_build {
        provenance["go_build"] = build.clone();
    }
    let mut measured = BTreeSet::new();
    provenance["cases"] = json!(
        cases
            .iter()
            .map(|(case, executions)| {
                let input = record::CaseInput::new(&case.probe());
                let mut columns = Vec::new();
                let mut omitted = Vec::new();
                let mut routes = std::collections::BTreeMap::new();
                for (v, e) in options.planned_columns(executions) {
                    let id = run::column_id(v, e);
                    let (route, reason) = options.route(case, &input, v, e);
                    if let Some(route) = route {
                        measured.insert((v.name, e));
                        columns.push(id.clone());
                        routes.insert(id, route);
                    } else {
                        omitted.push(json!({"column":id,"reason":reason}));
                    }
                }
                let mut entry = json!(case);
                entry["columns"] = json!(columns);
                entry["omitted"] = json!(omitted);
                entry["routes"] = json!(routes);
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
    provenance["columns"] = json!(
        options
            .variants()
            .iter()
            .flat_map(|v| {
                measured
                    .iter()
                    .filter(|(name, _)| *name == v.name)
                    .map(|&(_, e)| {
                        json!({
                            "id": run::column_id(*v, e),
                            "implementation": v.name,
                            "execution": e,
                            "stable": v.route_stable(&operation),
                            "adapter": v.adapter,
                        })
                    })
                    .collect::<Vec<_>>()
            })
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
        if row["column"] != TESSERA_SERIAL || row["build"] != BUILD {
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

/// The serial Tessera control, whose memory sizes `glidesort-sqrt`'s buffer.
const TESSERA_SERIAL: &str = "tessera/serial";

/// Finds serial Tessera's measured peak allocation size for a case.
fn find_scratch_bytes(case: &InputCase, rows: &[Value]) -> Option<usize> {
    rows.iter()
        .find(|r| r["case_id"] == case.id() && r["column"] == TESSERA_SERIAL && r["build"] == BUILD)
        .and_then(|r| r["peak_extra_bytes"].as_u64())
        .map(|n| n as usize)
}
/// Checks every supported adapter against each case's shared reference.
fn validate_cases(options: &Options, cases: &[(InputCase, Vec<Execution>)]) -> Result<()> {
    for (index, (case, executions)) in cases.iter().enumerate() {
        let mut digest = None;
        eprintln!(
            "Validating {}/{}: {} ({BUILD})",
            index + 1,
            cases.len(),
            case.id()
        );
        let input = record::CaseInput::new(case);
        for (v, e) in options.supported_columns(case, executions) {
            if v.adapter == "go" {
                let mut worker = go::Worker::start(options.go_worker.as_ref().unwrap(), e, None)?;
                let result = worker.case("verify", v.name, case, e)?;
                let expected =
                    digest.get_or_insert_with(|| format!("{:016x}", record::input_digest(case)));
                if result["digest"].as_str() != Some(expected) {
                    return Err(format!(
                        "{}: Go input digest differs from Rust for {}",
                        case.id(),
                        v.name
                    )
                    .into());
                }
            } else if let Some(mut k) = v.prepare(&input, e, 32_768) {
                k.restore(1);
                k.run_batch();
                k.check()
                    .map_err(|error| format!("{} {}: {error}", case.id(), run::column_id(v, e)))?;
                host::check_threads(k.worker_threads())?;
            }
        }
    }
    println!("Validated {} cases ({BUILD})", cases.len());
    Ok(())
}
/// Resumes completed groups and records the missing paired measurements.
fn measure_cases(
    options: &Options,
    cases: &[(InputCase, Vec<Execution>)],
    lock: &File,
) -> Result<()> {
    prepare_output(options)?;
    // The memory worker owns completeness for every adapter, not just Tessera.
    // It repairs an interrupted write and skips every row already committed.
    let exe = std::env::current_exe()?.with_file_name("memory");
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--out").arg(&options.output_dir);
    if let Some(worker) = &options.go_worker {
        cmd.arg("--go-worker").arg(worker);
    }
    cmd.arg("--blocks").arg(options.sampling.blocks.to_string());
    if let Some(path) = &options.plan_path {
        cmd.arg("--plan").arg(path);
    } else {
        cmd.args(["--plan", &options.plan_name]);
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
    let groups: BTreeSet<_> = cases.iter().map(|(case, _)| case.group()).collect();
    for group in groups {
        if completed.contains(&group) {
            continue;
        }
        eprintln!("Measuring {group} ({BUILD})");
        let mut start = host::observe_group(&group, "start");
        start["build"] = json!(BUILD);
        append_record(&mut f, &start)?;
        for (case, executions) in cases.iter().filter(|(case, _)| case.group() == group) {
            let bytes = *scratch
                .get(&case.id())
                .ok_or_else(|| format!("missing Tessera memory measurement: {}", case.id()))?;
            run::measure_case(
                case,
                &options.supported_columns(case, executions),
                bytes,
                &options.sampling,
                BUILD,
                options.go_worker.as_deref().map(|worker| (worker, lock)),
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
    let mut options = Options::parse(memory)?;
    if options.command == "help" || options.command == "--help" || options.command == "-h" {
        let command = if memory {
            "memory"
        } else {
            "compare <list|validate|run>"
        };
        println!(
            "{command} [--plan smoke|release|FILE] [--group OPERATION/TYPE/DISTRIBUTION[/PARAMETERS]] [--out DIR] [--sample-ms N] [--memory-mib N] [--blocks 3|9]"
        );
        return Ok(());
    }
    options.discover_go()?;
    let cases = options.cases(options.group.as_deref())?;
    if options.command == "run" {
        // The manifest includes every case, even for a restricted group. Keep
        // required-control admission with native capabilities, not a TS limit.
        run::validate_controls(&options.cases(None)?, &options.variants())?;
    }
    match options.command.as_str() {
        "list" => {
            // One row per adapter, planned execution and distinct route, with
            // the types that take that route.
            let operation = options.operation();
            let probe_operation = operation.probe();
            let executions: BTreeSet<_> =
                cases.iter().flat_map(|(_, e)| e.iter().copied()).collect();
            for v in variant::list_variants() {
                for &execution in &executions {
                    let mut routes: Vec<(&str, Vec<&str>)> = Vec::new();
                    for ty in &catalog().types {
                        let probe = record::CaseInput::new(&InputCase {
                            ty: ty.id.clone(),
                            distribution: "random".into(),
                            len: 32,
                            operation: probe_operation.clone(),
                        });
                        let Some(route) = v.route(&probe, execution) else {
                            continue;
                        };
                        match routes.iter_mut().find(|(r, _)| *r == route) {
                            Some((_, types)) => types.push(&ty.id),
                            None => routes.push((route, vec![&ty.id])),
                        }
                    }
                    for (route, types) in routes {
                        println!(
                            "{}\t{}\t{}\t{}",
                            run::column_id(v, execution),
                            if v.route_stable(&operation) {
                                "stable"
                            } else {
                                "unstable"
                            },
                            types.join(","),
                            route
                        );
                    }
                }
            }
            Ok(())
        }
        "validate" | "run" => {
            let lock = lock_worker(&options.output_dir, "comparison")?;
            if options.command == "validate" {
                validate_cases(&options, &cases)
            } else {
                measure_cases(&options, &cases, &lock)
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

fn measure_memory(options: &Options, cases: &[(InputCase, Vec<Execution>)]) -> Result<()> {
    prepare_output(options)?;
    let path = options.output_dir.join("memory.jsonl");
    let mut rows = read_block_records(&path)?;
    rewrite_journal(&path, &rows)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    for (index, (case, executions)) in cases.iter().enumerate() {
        eprintln!(
            "Measuring memory {}/{}: {} ({BUILD})",
            index + 1,
            cases.len(),
            case.id()
        );
        let input = record::CaseInput::new(case);
        for (v, e) in options.supported_columns(case, executions) {
            if v.adapter == "go" {
                continue;
            }
            if case.operation.is_sort()
                && case.input_bytes() > OPPONENT_MEMORY_INPUT_BYTES
                && v.name != "tessera"
            {
                continue;
            }
            let column = run::column_id(v, e);
            if rows
                .iter()
                .any(|r| r["case_id"] == case.id() && r["column"] == column && r["build"] == BUILD)
            {
                continue;
            }
            if let Some(mut k) = v.prepare(&input, e, find_scratch_bytes(case, &rows).unwrap_or(0))
            {
                k.restore(1);
                super::allocation::begin();
                k.run_batch();
                let peak = super::allocation::end();
                k.check()
                    .map_err(|error| format!("{} {column}: {error}", case.id()))?;
                host::check_threads(k.worker_threads())?;
                let mut row = json!({"case_id":case.id(),"column":column,"build":BUILD,"peak_extra_bytes":peak,"verified":true});
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
        let row = serde_json::json!({"case_id":"sort/u64/random/49/s1", "column":"tessera/serial", "build":super::BUILD, "verified":true, "peak_extra_bytes":123});
        std::fs::write(&journal, format!("{row}\n")).unwrap();
        assert_eq!(
            super::read_scratch(&output_dir).unwrap()["sort/u64/random/49/s1"],
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
