<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0042: Reuse the dense order probe for confirmation

Date: 2026-11-08. Status: accepted.

## Context

The order gate checks adjacent pairs at a stride of max(1, floor(n / 4,096)). Below 8,192 records, that stride is one: the gate already visits every adjacent pair, but then `confirm_order` repeats a full scan for an ordered input. This second scan is redundant for ascending and descending inputs.

The result must still distinguish strict descent from descending keys with ties so the fast reversal in [decision 0041](0041-strict-descending-fast-path.md) preserves stability.

## Decision

When the stride is one, classify order and strict descent in the full adjacent-pair scan and return its result directly. Keep `confirm_order` for larger inputs, where the strided gate only samples the input. The dense scan stays in a non-inlined helper so its loop does not enlarge the sampled path.

## Consequences

Dense ordered inputs avoid one full confirmation scan; unsorted dense inputs still stop as soon as both directions are disproven. Strict descent remains available to the existing direct-reversal path, while ties keep the stable reversal. The threshold follows the existing stride calculation; there is no new size constant or allocation.

The key function is called fewer times on dense ordered inputs. Its exact call count and order are not part of the API contract. The paired measurements, including the code-placement controls, are recorded in the [port record](../design/port.md#dense-order-probes).
