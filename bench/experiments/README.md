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

Local checks:

    python3 -m unittest discover -s bench/experiments -p 'test_*.py'
    python3 bench/experiments/analyze.py /path/to/downloaded/results
