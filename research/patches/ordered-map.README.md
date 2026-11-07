<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Ordered-map interpretation control

This is a **research-only, unadopted** single-variable follow-on to historical
`b1d36ce4e2b547ba72a0026dfb5850389e0d45e7` plus
[`combined-main-5b13.patch`](combined-main-5b13.patch). Apply that prerequisite
first, then `ordered-map.patch`. Do not stack the fixed-root, sleep50, deferred,
virtual, halo, rank, or indirect controls. This is not a drop-in patch for the
public `0e50926` implementation.

The candidate replaces redundant map interpretation only after the existing
whole-block permutation returns normally. It retains every copy, publication
point, allocation/capacity, root geometry, child kernel, and wait strategy. The
source delta changes two runtime files and adds one test file; no Cargo file
changes. See [the proof and space boundaries](ordered-map.SPACE-AND-PROOF.md),
[validation scope](ordered-map.VALIDATION.md), and
[machine-readable lineage](ordered-map.SOURCE-LINEAGE.json).

The historical base retains the ordinary `tessera-sort` package name and
implicit `tessera_sort` library name. All21 resulting runtime/test source files
match the independently validated private candidate byte for byte. Independent
apply, full-tree comparison, and both reverse-apply checks passed. No staging
build or shared production-tree modification was performed.

Candidate source identity:
`331452a13f64dd430ddf79c60ed7056856ab979649a644712c303debfb01620a`.
The identity's exact file set and serialization are defined by the external
frozen manifest; its file SHA-256 is
`5cc8565181593afd3ce8291dfe7d9ceb9bf94e1b1de38588a53247a915966471`.

Follow-on patch SHA-256:
`cf039ba22f98c875cbbb5e8558c16031a6e9bc4576cd205f14ca73fdf5c9c336`.

No speedup, slowdown, or acceptance conclusion is recorded here. Performance
measurement is a separate stage. Its attribution plan includes a refactor-only
control retaining the wrappers while running generic map interpretation.
Source review and correctness are not performance evidence. Raw logs, timing
data, harnesses, generated binaries, and external source archives are excluded
from this repository packet.
