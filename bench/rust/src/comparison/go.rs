// SPDX-License-Identifier: BSD-2-Clause

//! The Go worker owns concrete calls, verification and its clock. The Rust
//! coordinator owns case identities, block order and journal publication.

use super::Result;
use crate::plan::{Execution, InputCase, record_bytes};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Duration,
};

pub(super) struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Worker {
    pub(super) fn start(binary: &Path, execution: Execution, lock: Option<&File>) -> Result<Self> {
        let mut command = Command::new(binary);
        if let Some(lock) = lock {
            // The inherited descriptor keeps the scheduler lease while an
            // orphaned worker completes its current batch and sees stdin close.
            command.stderr(lock.try_clone()?);
        }
        Self::spawn(command.env("GOMAXPROCS", execution.threads().unwrap_or(1).to_string()))
    }

    fn spawn(command: &mut Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Self {
            input: child.stdin.take().expect("piped stdin"),
            output: BufReader::new(child.stdout.take().expect("piped stdout")),
            child,
        })
    }

    pub(super) fn request(&mut self, request: &Value) -> Result<Value> {
        serde_json::to_writer(&mut self.input, request)?;
        self.input.write_all(b"\n")?;
        let mut line = String::new();
        if self.output.read_line(&mut line)? == 0 {
            return Err(format!("Go worker exited while handling {}", request["action"]).into());
        }
        let response: Value = serde_json::from_str(&line)?;
        if let Some(message) = response["error"].as_str() {
            return Err(message.to_owned().into());
        }
        Ok(response)
    }

    pub(super) fn case(
        &mut self,
        action: &str,
        name: &str,
        case: &InputCase,
        execution: Execution,
    ) -> Result<Value> {
        let id = case.worker_id("go", execution);
        let result = self.request(&json!({
            "action": action, "id": id, "case": case, "implementation": name,
            "record_bytes": record_bytes(&case.ty), "threads": execution.threads().unwrap_or(1),
            "parallel": execution.threads().is_some(),
        }))?;
        if result["id"] != id {
            return Err(format!("Go worker returned the wrong timed ID for {id}").into());
        }
        Ok(result)
    }

    pub(super) fn batch(&mut self, count: usize) -> Result<Duration> {
        let result = self.request(&json!({"action":"batch", "batch":count}))?;
        let elapsed = result["elapsed_ns"].as_u64().filter(|&ns| ns > 0);
        let Some(ns) = elapsed.filter(|_| result["sorts"] == count) else {
            return Err("invalid Go batch response".into());
        };
        Ok(Duration::from_nanos(ns))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Once the final response has been read, no worker activity should
        // carry into the next slot. Also reap a failed worker.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_verified_batch_shape() {
        for (reply, message) in [
            (r#"{"elapsed_ns":11,"sorts":2}"#, None),
            (
                r#"{"elapsed_ns":11,"sorts":1}"#,
                Some("invalid Go batch response"),
            ),
            (
                r#"{"elapsed_ns":0,"sorts":2}"#,
                Some("invalid Go batch response"),
            ),
            (r#"{"error":"incorrect output"}"#, Some("incorrect output")),
            ("", Some("Go worker exited")),
        ] {
            let mut child = Command::new("sh");
            child.args([
                "-c",
                r#"read -r request; if [ -n "$1" ]; then printf '%s\n' "$1"; fi"#,
                "fixture",
                reply,
            ]);
            let mut worker = Worker::spawn(&mut child).unwrap();
            let result = worker.batch(2);
            if let Some(message) = message {
                assert!(result.unwrap_err().to_string().contains(message));
            } else {
                assert_eq!(result.unwrap(), Duration::from_nanos(11));
            }
        }
    }
}
