<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Owned-record sorting diagnostic

The earlier sustained slowdown was not reproduced in this fixed run. Native calls had medians of 1.89–1.92ms, with isolated long calls rather than a sustained 4–5.8ms regime.

The experiment used 262,144 owned records (64-bit key plus String payload), eight workers, the original 256-way root and AVX512 dispatch. All 216 predeclared calls passed full stable payload checks. All 27 traces were retained; there were no selected reruns or tuning changes.

Instrumentation mattered: enabled traces were 8.7% slower than the same diagnostic copy with recording disabled at the paired median. The disabled copy was 2.0% above the native A/A center. Individual ratios varied substantially, so traced phases are not an unperturbed reproduction.

Observed child CPU was relatively steady. Some longer traces instead had larger preparation, scatter or permutation spans. Queue-empty CPU was a separate material cost during the later consumer period. These observations do not establish why the earlier slowdown happened and do not identify a CPU quota, cache, spinning or hardware cause.

Uninstrumented long calls have outer elapsed times and broad worker CPU envelopes, but no phase traces or aligned broadcast wall timestamps. They cannot be matched to another call’s phase trace.

Conclusion: preserve the original algorithm. This is a diagnostic result, not evidence for a performance win or adoption decision.
