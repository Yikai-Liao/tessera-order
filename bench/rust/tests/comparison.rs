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
    fs::write(&plan, r#"{"schema":1,"harness":"comparison","algorithms":["tessera","std-stable"],"include":[{"types":["u64"],"distributions":["random"],"lengths":[49]}]}"#).unwrap();
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
    assert_eq!(rows.len(), 2);
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
    assert_eq!(fs::read_to_string(&journal).unwrap(), original);
    assert_eq!(records(directory.join("blocks.jsonl")), blocks);
    fs::remove_dir_all(directory).unwrap();
}
