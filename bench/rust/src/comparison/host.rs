// SPDX-License-Identifier: BSD-2-Clause

use serde_json::{Value, json};
use std::{fs, process::Command};

fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}
fn command(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|p| p.status.success())
        .map(|p| String::from_utf8_lossy(&p.stdout).trim().to_owned())
}
fn field(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|s| {
        s.strip_prefix(name)
            .map(|s| s.trim().trim_start_matches(':').trim().to_owned())
    })
}
fn affinity() -> Option<String> {
    field(&read("/proc/self/status")?, "Cpus_allowed_list")
}
fn cpufreq(name: &str) -> Option<String> {
    let affinity = affinity()?;
    let cpu = affinity.split([',', '-']).next()?;
    read(&format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/{name}"))
}
pub(super) fn fingerprint() -> Value {
    let cpu = read("/proc/cpuinfo")
        .and_then(|s| field(&s, "model name"))
        .or_else(|| command("sysctl", &["-n", "machdep.cpu.brand_string"]));
    let turbo = read("/sys/devices/system/cpu/intel_pstate/no_turbo")
        .map(|s| s == "0")
        .or_else(|| read("/sys/devices/system/cpu/cpufreq/boost").map(|s| s == "1"))
        .map(|on| if on { "on" } else { "off" });
    json!({"machine":read("/etc/machine-id"),"cpu":cpu,"topology":command("lscpu", &["-p=CPU,CORE,SOCKET,NODE"]),"kernel":command("uname", &["-srvm"]),"governor":cpufreq("scaling_governor"),"turbo":turbo,"smt":read("/sys/devices/system/cpu/smt/active"),"affinity":affinity()})
}
pub(super) fn observation(group: &str, phase: &str) -> Value {
    let temperatures: Vec<_> = fs::read_dir("/sys/class/thermal")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path().join("temp")).ok())
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();
    json!({"kind":"observation","group":group,"phase":phase,"unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),"load":read("/proc/loadavg"),"frequency_khz":cpufreq("scaling_cur_freq"),"memory_pressure":read("/proc/pressure/memory"),"temperatures_millidegrees":temperatures})
}
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
