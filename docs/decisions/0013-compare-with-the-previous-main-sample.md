# 0013: Alert on a fixed percentage from the previous main sample

Date: 2026-11-07. Status: accepted. Supersedes the alert rule of [0004](0004-continuous-benchmarks-on-bencher.md).

## Context

`main` was judged with a t-test over its last 32 samples (upper and lower boundary 0.999). The test assumes that its samples come from one unchanging population. Here `main` is optimized continuously, so its history is a series of steps, and the window averages code from before and after each of them.

`u64 · 16 values · 65,536` shows the effect. Fifteen samples near 190 µs (limits ±2–3%) were followed by two near 124 µs, after an optimization landed. The baseline became 187 µs with limits of 125–248 µs. A PR then measured 151 µs and was reported as 19% faster, although it had not touched that path and `main` was already at 124 µs; the same case was reported as 32.5% faster in the other build. A real further improvement would have fallen inside the limits too.

Replaying the 20 commits of `main` history (51 series, one sample per commit) and a synthetic series built from each case's own noise (a step of −35% followed by k clean samples, then a PR that is a further 10% faster):

| Rule | Detected, 1 / 3 / 6 / 10 / 15 samples after the step |
|---|---|
| t-test, last 32 (before) | 13% / 0% / 0% / 0% / 0% |
| t-test, last 8 | 0% / 0% / 0% / 84% / 84% |
| IQR × 3, last 8 | 97% / 1% / 0% / 89% / 89% |
| 5% from the mean of the last 5 | 99% / 100% / 100% / 100% / 100% |
| 5% from the previous sample | 99% / 99% / 99% / 99% / 99% |

The window rules either miss the next improvement for as long as the window holds the old level, or raise false alarms in it (5% from the last five: up to 100% false alarms in the first samples after the −35% step). On the real history, where no step is known, the previous-sample rule alerted on 1 of 397 quiet points; the t-test, 9. The step cases were labelled by a rough rule, the noise of the synthetic series is resampled from 12 samples per case, and runner differences are not modelled.

## Decision

`main` alerts on a value more than 5% above or below the previous `main` sample: a percentage test (`--threshold-test percentage`, boundaries 0.05) with a window of one sample (`--threshold-min-sample-size 1`, `--threshold-max-sample-size 1`). PRs keep cloning these thresholds from their base commit.

## Consequences

The baseline follows each step from the next commit on, so a PR is compared with the code it is based on. The history of earlier code no longer widens a limit.

The limits no longer adapt to a case's noise, and one noisy sample moves the baseline of the next comparison. Today the relative standard deviation of a case's clean `main` samples has a median of 1%, and two points in the history moved by 5% and then returned. A case measured with more noise than 5% needs its own boundary or a different harness setting; the report does not widen it.

A reported change smaller than 5% is never an alert. The comparison chart and tables show it, and the geometric mean counts it. A reviewer who needs an effect below 5% takes it from the A/A measurement of the full protocol, as before.

After merging, one run of **Build benchmarks** on `main` per build replaces the thresholds (`--thresholds-reset`) and provides the first baseline; earlier samples remain in the history.
