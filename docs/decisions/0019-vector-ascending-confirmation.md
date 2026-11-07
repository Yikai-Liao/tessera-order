# 0019: Reuse the vector scan for ascending-order confirmation

Date: 2026-11-07. Status: accepted.

## Context

The order probe confirms a sampled direction with a full scalar pass. The optional SIMD backend already locates descending adjacent pairs during insertion, but an ascending input returns before that insertion pass runs. Its full confirmation therefore does not use the vector kernel.

Confirmation also calls user key functions. Re-reading the first unequal record, or changing the order of those calls, would alter their observable behavior. Backend selection must preserve that scalar evaluation contract.

## Decision

Give extractors an ascending confirmation operation that receives the unread suffix and the last encoded key. The default operation folds over that suffix, evaluating each key once. Direct primitive extraction with `portable-simd` checks the predecessor boundary and then uses the existing inversion scan. Dispatch occurs once per eligible confirmation; suffixes shorter than the existing scan minimum stay scalar.

The direction probe, descending confirmation, insertion algorithm, digit planner and routing thresholds retain their current decisions. Reverse keys and user key functions retain scalar confirmation. No allocation or new unsafe operation is needed.

## Consequences

This targets ascending primitive inputs. It does not establish a gain for random input, near-sorted input, records sorted by a closure or keys without a vector kernel. The extra entry point changes code placement, so acceptance requires source-copy controls alongside duplicated baseline timings under the [benchmark protocol](../development/benchmarking.md), including the default build and the static SIMD build.

Passing the already encoded predecessor to the operation preserves the evaluation count for user key functions. Re-starting a generic inversion scan at that predecessor would evaluate it again.
