## Summary

<!-- What this changes and why. Link the issue it addresses. -->

## Validation

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets` and `cargo fmt --all --check`
- [ ] Engine changes: Miri on the affected tests
- [ ] Performance claims: results following `docs/benchmarking.md`, including the A/A range
- [ ] Semantics changes: `conformance/semantics.md` updated

## AI assistance

<!-- Check one. If you used AI tools, name them and say what they did: code, tests, documentation or review. -->

- [ ] No AI tools were used.
- [ ] AI tools were used:

## Author checklist

- [ ] I have read and understand every change in this pull request and can answer review questions about it.
- [ ] Every commit is signed off by me (`git commit -s`), certifying the [DCO](https://developercertificate.org/).
