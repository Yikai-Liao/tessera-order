# Virtual-bucket stage-control checkpoint

The virtual-bucket line is stopped. Further optimization work returns to the
best combined implementation. This checkpoint records the completed negative
results rather than proposing a production merge.

Both experiments use an **old frozen core** and sealed Copy-only records, not
the newly merged SIMD main implementation. They are three-block random-input
diagnostic screens, not acceptance benchmarks or new asymptotic claims.

- [Stage1 report](stage1-report.md): individual stage controls reduced their
  targeted work, but the fastest virtual combination remained 8.30× slower on
  Row16 and 13.87× slower on u64 than the strong combined + std-yield W8 control.
- [Stage2 report](stage2-report.md): fused root and pull restoration have large
  targeted-stage effects, but the fastest virtual variant still took 6.57× /
  11.50× the combined control's Row16/u64 median total. Wide A/A ranges limit
  modest total-time claims. No result establishes a win over SIMD main.
- [Validation limits](validation.md): source and hash checks passed, and both
  summaries recomputed exactly. Only the relocated stage1 timing build and
  driver Clippy completed before work stopped. No new timing run was made.
- [Exact checksums](checksums.json): frozen source/driver/manifest/evidence hashes,
  original timing/heap binary hashes, and the external archive manifests.

The reports are unchanged owner reports. Their original relative paths refer
to the external archive, not to files included in this documentation directory.
The archive layout is research/virtual-stage-controls/{stage1,stage2}; matching
FILE_MANIFEST.json and FILES.sha256 identify its contents. No build targets or
executables are included in that source/evidence archive. Original binary
hashes identify the measured binaries; rebuilding after relocation need not
produce identical hashes because the release profile includes debug paths.

Repository rules exclude raw benchmark data, profiles, and research harnesses.
Accordingly this Git checkpoint contains only reports, hashes, and limitations.
The complete source/driver/raw-evidence packet is retained separately outside
the repository. No algorithm patch, new public API, benchmark harness, raw
JSONL, or profile is added here. Archived source paths and hashes are identifiers,
not active absolute-path dependencies or an assertion that those paths exist
on another machine.
