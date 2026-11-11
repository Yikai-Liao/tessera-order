## Summary

<!-- The title follows Conventional Commits, for example `fix(block): keep records when a key panics`; see CONTRIBUTING.md. -->

<!-- What this changes and why. Link the issue it addresses. -->

## Validation

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets` and `cargo fmt --all --check`
- [ ] Engine changes: Miri on the affected tests
- [ ] Performance claims: results following `docs/development/benchmarking.md`, including the A/A range
- [ ] Semantics changes: `conformance/semantics.md` updated
- [ ] Documentation: edits are confined to the owners in `docs/README.md` ("Who owns what") of the changed facts, with no parallel record and no numbers restated outside their owner; conclusions later work depends on are recorded there, not only in this description

## AI assistance

<!-- Check one. If you used AI tools, name them and say what they did: code, tests, documentation or review. -->

- [ ] No AI tools were used.
- [ ] AI tools were used:

## Author checklist

- [ ] I have read and understand every change in this pull request and can answer review questions about it.
- [ ] Every commit is signed off by me (`git commit -s`), certifying the [DCO](https://developercertificate.org/).
