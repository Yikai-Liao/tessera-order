# Single-job timing concurrency experiment

This branch-only experiment compares one, two and four concurrently running,
single-threaded Criterion processes on Bencher's intel-v1 spec. It does not
change engine code, production tracking or the Rayon feature.

The 56 production benchmark IDs are divided into four balanced batches. Each
batch has four fixed CPU lanes. API/workspace pairs stay together. A group's
CPU remains unchanged in all conditions. Every remote job runs serial A1,
two/four-worker phases in alternating order across rounds, and serial A2.
Three rounds cover every case three times per condition, with six serial
controls. Warm-up windows, measurement windows and sample counts are unchanged.
Groups are selected before input generation; workers have isolated output
directories.

The image contains Python as the supervisor and one static benchmark executable.
Its SHA-256 and guest CPU topology are recorded. All jobs execute sequentially
under the production Bencher concurrency group, with a 270-second job timeout.
Reports use only an experiment-specific branch and Testbed, not production
baselines. The guest receives no credentials.

Dispatch the existing Track benchmarks workflow on this exact branch. Inputs:
variant (default or portable-simd), rounds (1–5), batches (space-separated 0–3).
Start with one round and batch 0 to verify transport. The small suite excludes\nall inputs of at least one million records and balances the remaining 41 IDs\nover two batches; select batches 0 and 1 for that suite. The benchmark environment
temporarily authorizes only this exact branch; remove that policy after all
remote jobs finish.

The workflow archives the report, authenticated job output and raw samples.
analyze.py uses the mean of bracketing serial point estimates as the denominator.
It reports per-case A/A min/max, concurrent median/range, counts beyond the
existing 5% alert boundary, and phase wall time. Raw benchmark outputs remain in
GitHub artifacts or external task directories, not in this repository.

This is an initial three-block interference experiment, not the nine-block
release-ranking protocol. Small shifts within the controls are inconclusive.
The conclusions apply to the recorded co-runner schedule; other pairings can
have different interference. Guest topology does not prove host CPU mapping.

## Results

All runs below succeeded on Bencher intel-v1. Each condition has three outer
repeats per case. Default full and small-only runs used the identical timing
executable (SHA-256 4658532b0f5d392040a2df246526ff9191e8188739a8a91143d341e135049534).
The SIMD executable hash was
a23a63708b26593e50270cc3d0df0041f24e5662d784f8f0295cce1865e05592.

| Suite | Cases | 2-worker phase time / serial | 2-worker median shifts beyond 5% | 4-worker phase time / serial | 4-worker median shifts beyond 5% |
|---|---:|---:|---:|---:|---:|
| Default, all sizes mixed | 56 | 0.529 | 7/56 | 0.278 | 25/56 |
| Portable SIMD, all sizes mixed | 56 | 0.524 | 8/56 | 0.276 | 27/56 |
| Default, only sizes below 1M | 41 | 0.528 | 0/41 | 0.274 | 8/41 |

Controls are not zero-width: the full Default A/A envelope was approximately
0.9834-1.0166, and the full SIMD envelope approximately 0.9856-1.0144.
In the small-only experiment, 40 cases stayed within approximately +/-0.77%;
u128/random/65536 had an anomalous envelope of approximately +/-4.02%.
For that case, serial A2 was about 8% above A1 in rounds 0 and 2, but not
round 1. Inspection of the raw samples shows a persistent shift within those
serial runs, not just a wide confidence interval. Its cause is unresolved;
the apparent negative concurrent shift is not evidence of an improvement.

For small-only Default, the median across per-case two-worker shifts was
+0.06%. The largest positive per-case median was +3.38% for row_u64/d16/65536
(A/A approximately +/-0.23%; concurrent range +3.17% to +3.80%).
The wide random record's API/workspace median shifts fell to +2.45%/+2.10%
from +6.28%/+6.87% in the mixed-size experiment. Four workers still caused
small-only median shifts up to +21.28%, so removing large inputs is not
sufficient to justify four workers.

These observations support **two workers for small cases, with all inputs of
at least one million records exclusive**, as a candidate continuous-screening
policy. They do not establish zero bias or validate every possible co-runner
assignment. Small-only SIMD and a production end-to-end schedule were not
tested. The small-only control anomaly also remains an investigation item.

The phase timings include preparation, validation and Criterion analysis.
They exclude image setup, remote queue time, and the exclusive large-case
part of a proposed hybrid policy; 0.528 is not an end-to-end CI speed ratio.

Remote evidence and downloadable raw artifacts:

- [Default full, 12 paired trials](https://github.com/Yikai-Liao/tessera-sort/actions/runs/42445494197)
- [Portable SIMD full, 12 paired trials](https://github.com/Yikai-Liao/tessera-sort/actions/runs/42445543990)
- [Default small-only, 6 paired trials](https://github.com/Yikai-Liao/tessera-sort/actions/runs/42447791171)

Artifacts expire after 14 days; a compressed local archive is retained outside
the repository. The temporary experiment environment permission was revoked
after the last job completed; only main remains authorized. No production
workflow or baseline was changed.

Local checks:

    python3 -m unittest discover -s bench/experiments -p 'test_*.py'
    python3 bench/experiments/analyze.py /path/to/downloaded/results
