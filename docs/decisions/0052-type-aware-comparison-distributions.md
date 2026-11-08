<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0052: Compare the non-uniform inputs across algorithms

Date: 2026-11-08. Status: accepted.

## Context

Continuous tracking already covers cardinality, skew, digit structure, long-distance disorder, sorted prefixes with outlier tails and finite signed floats. The algorithm comparison still selects only its original ten distributions. It cannot show whether behavior on the newer inputs is specific to Tessera or shared by competing sorts.

## Decision

Promote all ten newer distributions into the formal comparison plan, reusing the generators and their pinned hashes unchanged. Cover cardinality, skew and disorder across every record representation; select full-width and digit-structure inputs only for types that preserve their meaning. In particular, do not narrow the bit-63 cluster generator to `u32` or interpret integer masks as equivalent float encodings. The [benchmark guide](../development/benchmarking.md#cases-and-suites) lists the selected products.

Measure the added inputs primarily at 65,536 and 1,000,000 records, with all nine non-float-specific additions also measured on 10M `u64` keys. Measure finite signed floats at every release length. The smoke plan validates every release type/distribution pair at 1,000 records. Keep selection in the existing plans rather than adding runner branches or another execution wrapper.

Plot every selected 1M distribution in per-type input panels, admitting stable algorithms only for records. Take the 10M memory maximum across every selected `u64` distribution. Retain the original ten distributions as an explicitly labelled common set for ranking and cross-type aggregate panels; those aggregates do not describe the whole expanded suite.

## Consequences

The release plan grows from 260 to 381 inputs and smoke validation from 70 to 125 pairs. Continuous case IDs, generator hashes, sorting implementations and measurement protocols remain unchanged. Reports show the new inputs individually without changing the cross-type aggregate population or presenting semantically different key geometries as the same experiment. Raw measurements and executables stay outside the repository.
