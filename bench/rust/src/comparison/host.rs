// SPDX-License-Identifier: BSD-2-Clause

//! Read-only host fingerprints and group observations.

use serde_json::{Value, json};
use std::{fs, process::Command};

/// Reads an available host property as trimmed text.
fn read_optional(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}
/// Reads a host query when its platform command is available.
fn read_command_output(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|p| p.status.success())
        .map(|p| String::from_utf8_lossy(&p.stdout).trim().to_owned())
}
/// Extracts a named property from a colon-separated system report.
fn find_field(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|s| {
        s.strip_prefix(name)
            .map(|s| s.trim().trim_start_matches(':').trim().to_owned())
    })
}
/// Reads the logical CPUs allowed for the current process.
fn read_affinity() -> Option<String> {
    find_field(&read_optional("/proc/self/status")?, "Cpus_allowed_list")
}
/// Reads a frequency-policy property for the first allowed CPU.
fn read_cpufreq_setting(name: &str) -> Option<String> {
    let affinity = read_affinity()?;
    let cpu = affinity.split([',', '-']).next()?;
    read_optional(&format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/{name}"))
}
/// Records the machine and host settings that must agree across groups.
pub(super) fn read_fingerprint() -> Value {
    let cpu = read_optional("/proc/cpuinfo")
        .and_then(|s| find_field(&s, "model name"))
        .or_else(|| read_command_output("sysctl", &["-n", "machdep.cpu.brand_string"]));
    let turbo = read_optional("/sys/devices/system/cpu/intel_pstate/no_turbo")
        .map(|s| s == "0")
        .or_else(|| read_optional("/sys/devices/system/cpu/cpufreq/boost").map(|s| s == "1"))
        .map(|on| if on { "on" } else { "off" });
    json!({
        "machine": read_optional("/etc/machine-id"),
        "cpu": cpu,
        "topology": read_command_output("lscpu", &["-p=CPU,CORE,SOCKET,NODE"]),
        "kernel": read_command_output("uname", &["-srvm"]),
        "governor": read_cpufreq_setting("scaling_governor"),
        "turbo": turbo,
        "smt": read_optional("/sys/devices/system/cpu/smt/active"),
        "affinity": read_affinity(),
    })
}
/// Records changing host conditions at a group boundary.
pub(super) fn observe_group(group: &str, phase: &str) -> Value {
    let temperatures: Vec<_> = fs::read_dir("/sys/class/thermal")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path().join("temp")).ok())
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();
    json!({
        "kind": "observation",
        "group": group,
        "phase": phase,
        "unix_seconds": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
        "load": read_optional("/proc/loadavg"),
        "frequency_khz": read_cpufreq_setting("scaling_cur_freq"),
        "memory_pressure": read_optional("/proc/pressure/memory"),
        "temperatures_millidegrees": temperatures,
    })
}
/// Rejects a Linux measurement that started additional process threads.
pub(super) fn check_threads() -> Result<(), String> {
    if cfg!(target_os = "linux")
        && fs::read_dir("/proc/self/task")
            .map_err(|e| e.to_string())?
            .count()
            != 1
    {
        return Err("variant started additional threads".into());
    }
    Ok(())
}
