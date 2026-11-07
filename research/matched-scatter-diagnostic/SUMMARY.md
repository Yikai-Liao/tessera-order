<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Matched native versus aligned scatter diagnostic

A single fixed run compared the current native scheduler with the aligned stable root design on identical seed-one 10-million-record inputs: u64 and 16-byte key/index records, eight workers, 256 root buckets, with actual native defaults verified. It used one warmup plus three randomized measured blocks. All 64 calls passed complete stable-reference and fingerprint checks: 16 warmups and 48 measured calls. There were 16 diagnostic-enabled traces, of which 12 were measured.

Across the six measured same-block comparisons, the aligned record loop used more summed thread CPU:

- u64: 1.55×, 2.35× and 1.79× native. Native summed loop CPU was 20.58–27.51 ms; aligned was 42.65–48.34 ms. The corresponding loop wall envelopes were 2.93–4.15 ms and 6.62–12.46 ms.
- 16-byte rows: 1.57×, 1.23× and 2.33× native. Native summed loop CPU was 31.84–39.62 ms; aligned was 45.71–92.34 ms. Loop wall envelopes were 6.50–8.78 ms and 9.14–14.03 ms.

Summed CPU is work charged across threads, not elapsed wall time. Native job setup/finish outside the loop contributed only 0.30–0.66 ms summed, so the unequal setup boundaries do not account for this observed loop CPU gap. Aligned histogram/rank preparation was outside its scatter job and was measured separately.

The controls were noisy: one native diagnostic-disabled call was 2.01× its ordinary block mean, and the widest row A/A range was 0.744–1.256. All tails were retained. These are descriptive measurements of the frozen diagnostic copies, not a production speed acceptance, an isolated clock-overhead estimate, or proof of a specific instruction, cache or bandwidth cause. No production safety claim follows from this narrow Copy-record diagnostic.

The next useful target is reducing work in the aligned record loop, followed by a separate controlled performance test, rather than changing the allocator or restoration mover on the basis of unmatched historical scatter timings.

## Completed exact-binary assembly audit

The measured binary retains the ordinary structural difference: aligned scatter uses a 40-byte cursor, checked bucket-vector lookup and one record per loop; native uses 16-byte writers and four u64 or two Row16 records per loop. Both retain a full-block conditional branch per record. These observations do not isolate any one instruction's cost.

Importantly, boundary instrumentation introduced one context-pointer stack reload per record in both inspected common paths. Boundary-only source hooks therefore did not preserve hot machine code. The native/aligned qualitative difference persists, but neither the entire CPU gap nor the ordinary cost can be attributed to bounds checking. These are static decoded paths, not retired instruction, cache, bandwidth or cycle measurements.

The aligned diagnostic-on/off paired Row16 ratio was 1.254, reinforcing the perturbation qualification. Eight diagnostic smoke calls, three negative oracle fixtures and sixteen negative parser fixtures passed; independent review covered 38 source files and 45 dependencies, with source identity unchanged before and after the run. This is not a full production safety pass.

A separate unchecked-cursor indexing control is only in source preparation. It is not validated, accepted or included in this archive. No algorithm or runtime change is made here.

## Provenance

This publication preserves the aggregate observations and editorially incorporates the completed assembly audit. Original evidence remains unchanged; raw timings, traces, harnesses, private wrappers and binaries are not published.

- Original sanitized summary SHA256: 66cf56082cdc89f50c433ddb7e647a6e242990f4ffad7bb61c8a61d2c75e5dbb.
- Derived numeric summary SHA256: 88f03cd468f4124624ab623e116b6245c6b61e619af6553dc5031f68c8191543.
- Final evidence inventory SHA256: f9dca571d68648568a37ef1fd5971d461f311c520749f52d075f5b95bc6a4347.
- Frozen source plan SHA256: b2edad11ad4e6d3c83cf0eb990a7e2a3b770dd3be8a4c310d762500d67d02d08.
- Measured binary SHA256: a6989c69dfa542f70d57d75daaf3f7be3595c1511f24d369251cb35becc1f274.
- Completed assembly audit SHA256: 69df67b84fde915926adabc42dc6d356eca31089abf08a6b08da8a26d191d15f.
