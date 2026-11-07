
## Separate non-timed actual-work audit

After the timing experiment, one full-reference-checked call per type/config ran from the separately prepared safe-TLS diagnostic copy. The timed sources/binaries were unchanged. No duration samples were emitted or used. Applied root widths8/default and4/R16, P8, expected B, full child coverage and zero successful parallel-count entries were explicitly checked.

| Type/config | child block calls | child block input records | tile MSD calls | tile MSD input records | LSD calls | deeper block/tile levels |
|---|---:|---:|---:|---:|---:|---|
| u64/default | 256 | 10000000 | 4096 | 10000000 | 0 | none |
| u64/r16 | 16 | 10000000 | 4096 | 10000000 | 0 | none |
| row16/default | 256 | 10000000 | 8192 | 10000000 | 0 | none |
| row16/r16 | 16 | 10000000 | 4096 | 10000000 | 0 | none |

The first serial child block partition is depth0 in these counters, separate from the native root. All four cases have exactly that one child block level and one tile-MSD counting-pass level, each processing10M input records. There are no extra recursive block/tile levels, no LSD passes and no histogram overflow. Therefore the R16 child-CPU/tail increase is not evidence of an extra full partition/pass level. Chunk sizes and fanouts change; cache/bandwidth/instruction explanations remain unmeasured. Record-work sums overlap across stages and are not physical traffic.

Diagnostic provenance identity: 522d3300856fe3c69d1e220bd21f568e7848e856a72ddb7a5697456378e6e7ac. Compiler rustc1.99.1 commit58cf8f9c35b355199727627d07ae34a4e2846d67, x86_64-unknown-linux-gnu. Exact cached dependency rlibs are hashed; features match the existing Cargo fingerprint: default, std, rayon, portable-simd. Direct rustc emitted only a diagnostic rlib/executable, with release optimization, unwind, debug assertions/overflow checks off. This is not a performance-binary equivalence claim. The shared audit ended18:15:17Z, with167MiB filesystem reserve and stage cumulative growth below26MiB. No further candidate or allocator study ran.
