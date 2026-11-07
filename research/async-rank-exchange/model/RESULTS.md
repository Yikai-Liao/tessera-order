<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Granted safe-model result

All eight authorized commands completed with exit status 0. No assertion,
timeout, MemoryError, resource-limit failure, source modification, retry, or
parameter increase occurred. The shared CPU lock was actually acquired and
released through FD9, device 27/inode 1578019, before this analysis.

This supersedes the execution status in the frozen source-preparation documents;
their proof and plan were deliberately left unchanged for hash verification.
It is a token-model result, not production timing or a Rust correctness result.

## Exact counts

| Command | Completed schedules | Exact exhaustive fixtures | States | Transitions | Exit |
|---|---:|---:|---:|---:|---:|
| s1-directed | 1 | n/a | n/a | n/a | 0 |
| s1-matrix | 192 | n/a | n/a | n/a | 0 |
| s1-random | 64 | n/a | n/a | n/a | 0 |
| s1-stale | 64 | n/a | n/a | n/a | 0 |
| s2-injection-f1 | n/a | 4 | 105 | 134 | 0 |
| s2-injection-f2 | n/a | 12 | 2,528 | 4,476 | 0 |
| s2-tail-only | n/a | 2 | 69 | 89 | 0 |
| s2-shared-tiny | n/a | 2 | 99 | 129 | 0 |
| Total | 321 | 20 | 2,801 | 4,828 | all 0 |

All 20 exhaustive fixtures report `exact: true`. Zero were truncated. These
counts describe the approved subset, not the unexecuted default full suite.

The matrix covers six named shapes, P=1/2/3/8, four map-store orders, and two
schedule modes. The ordinary random and bounded historical-load commands each
cover 32 seeded fixtures with fair and random scheduling. Random runs are tests,
not exhaustive coverage or a fair-execution proof.

## Mandatory witnesses

The directed 3-cycle test performed 72 microsteps and asserted all of:

- One revalidation failure and retry.
- One metadata lock on a retired, child-mutated physical block.
- Two child starts before all root blocks were retired.

It completed with two swaps, one identity retirement, three publications and
12 model record copies. No root payload access after retirement occurred.
The identity target was incidentally fixed by another transaction and was still
credited by its own static owner.

Additional observed totals across completed schedules:

| Suite | Revalidation retries | Retired metadata locks | Early child starts |
|---|---:|---:|---:|
| Directed | 1 | 1 | 2 |
| Matrix | 96 | 32 | 440 |
| Random | 67 | 23 | 462 |
| Bounded historical loads | 113 | 78 | 452 |

These are coverage observations, not contention or performance predictions.

## Resources and identity

- Every command used a 60-second timeout, 768 MiB address-space limit and 1 MiB
  output-file limit. No process peak RSS was measured or inferred from that cap.
- Evidence remained below the 4 MiB budget. Before this report it occupied
  103,726 bytes; the largest output log was 58,715 bytes.
- Initial filesystem availability was 208,896,000 bytes, above 163 MiB.
  Availability after the window was 208,744,448 bytes, above the 150 MiB reserve.
- Model source SHA256:
  `fd0e1b7d9f0cf895dca44482cc4269f54c48ef07c23ce3d9f321cd1cb240a117`.
- Source freeze, exact commands, exits and logs are in
  `evidence-granted-first/{source.sha256,commands.tsv,status.tsv,*.log}`.
  `source-check.txt` confirms model, proof, plan and runner matched their freeze.

## Remaining gates

There was no counterexample in the executed scope. This does not establish Rust
aliasing/provenance, compiled atomic ordering, OS fairness, panic recovery,
production allocation peaks, or speed. The model's release-chain bookkeeping
describes the required happens-before argument but is not a weak-memory model
checker. No frozen real directory dump was available, so real-directory replay
remains untested. No Rust/raw-T implementation, build, test, benchmark, or new
execution authorization follows automatically from these results.
