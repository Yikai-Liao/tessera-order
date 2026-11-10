// SPDX-License-Identifier: BSD-2-Clause

//! Recovery contracts exercised through the retained executable pair.

#![cfg(feature = "compare")]

use std::{fs, process::Command};

#[test]
fn memory_resumes_missing_adapters() {
    let directory =
        std::env::temp_dir().join(format!("tessera-memory-resume-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let plan = directory.join("plan.json");
    let mut algorithms = vec!["tessera", "std-stable"];
    if cfg!(tessera_bench_revisions) {
        algorithms.extend(["tessera-baseline", "tessera-baseline-copy", "tessera-copy"]);
    }
    if cfg!(feature = "rayon") {
        algorithms.extend(["tessera-par-8", "rayon-stable-8"]);
    }
    fs::write(
        &plan,
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "harness": "comparison", "algorithms": algorithms,
            "include": [{"types": ["u64"], "distributions": ["random"], "lengths": [49]}]
        }))
        .unwrap(),
    )
    .unwrap();
    let measure = || {
        let output = Command::new(env!("CARGO_BIN_EXE_compare"))
            .arg("run")
            .arg("--plan")
            .arg(&plan)
            .arg("--out")
            .arg(&directory)
            .args(["--sample-ms", "1", "--memory-mib", "1"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    measure();
    let journal = directory.join("memory.jsonl");
    let original = fs::read_to_string(&journal).unwrap();
    let rows: Vec<_> = original
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect();
    assert_eq!(rows.len(), algorithms.len());
    let tessera = rows.iter().find(|row| row["variant"] == "tessera").unwrap();
    fs::write(&journal, format!("{tessera}\n{{\"partial\":")).unwrap();
    let records = |path| {
        fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>()
    };
    let blocks = records(directory.join("blocks.jsonl"));
    measure();
    let resumed = records(journal);
    assert_eq!(resumed.len(), rows.len());
    assert_eq!(resumed.first(), Some(tessera));
    // Missing adapters are measured again; parallel scheduling can change peaks.
    for (mut recovered, mut expected) in resumed.into_iter().zip(rows) {
        assert!(recovered["peak_extra_bytes"].as_u64().is_some());
        recovered
            .as_object_mut()
            .unwrap()
            .remove("peak_extra_bytes");
        expected.as_object_mut().unwrap().remove("peak_extra_bytes");
        assert_eq!(recovered, expected);
    }
    assert_eq!(records(directory.join("blocks.jsonl")), blocks);
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(feature = "rayon")]
#[test]
fn large_parallel_memory_is_measured() {
    let directory =
        std::env::temp_dir().join(format!("tessera-parallel-memory-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let plan = directory.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "harness": "comparison",
            "algorithms": ["tessera-par-8"],
            "include": [{
                "types": ["u32", "row_u32"],
                "distributions": ["equal"], "lengths": [1_000_001]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_memory"))
        .arg("--plan")
        .arg(&plan)
        .arg("--out")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let journal = fs::read_to_string(directory.join("memory.jsonl")).unwrap();
    let rows: Vec<serde_json::Value> = journal
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for ty in ["u32", "row_u32"] {
        assert!(rows.iter().any(|row| {
            row["case_id"] == format!("{ty}/equal/1000001/s1")
                && row["variant"] == "tessera-par-8"
                && row["verified"] == true
        }));
    }
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(feature = "rayon")]
#[test]
fn parallel_adapters_validate_records() {
    let directory =
        std::env::temp_dir().join(format!("tessera-parallel-validate-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let plan = directory.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "harness": "comparison",
            "algorithms": ["tessera-par-8", "rayon-stable-8"],
            "include": [{
                "types": ["f64", "row_u64", "record128", "owned"],
                "distributions": ["random", "d16"], "lengths": [131_073]
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_compare"))
        .arg("validate")
        .arg("--plan")
        .arg(&plan)
        .arg("--out")
        .arg(&directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(directory).unwrap();
}
