# Working on the Go implementation

The Go module in `tessera/` ports the Rust engine's stages and passes the same [semantics](../conformance/semantics.md) and golden vectors in [`conformance/vectors/`](../conformance/vectors/), but it is written as Go code. Go aims at the engineering sweet spot: good performance for little code, standard abstractions, and one codebase for every platform. Rust techniques that buy performance ceiling with code volume do not carry over. The repository [instructions](../AGENTS.md) apply except where this page replaces a Rust rule.

## Rules

- **Toolchain and dependencies.** The Go version in `tessera/go.mod` is the minimum; raise it only with the operator. The library imports only the standard library; the benchmark harness may depend on pinned opponents.
- **Generics.** Use plain generics and bind the key codec once at entry. Accept the cost of Go's dictionary-based generics. Do not add code generation, assembly, per-type kernel copies or `unsafe` pointer reinterpretation for local speedups; `unsafe.Sizeof` is the only `unsafe` use. A specialization needs a hosted benchmark showing a large loss on a complete operation, and stays small. Each Go stage stays comparable in size to the Rust stage it ports.
- **Garbage collection.** Do not emulate Rust ownership. Scratch is ordinary typed slices and may hold duplicate copies of records; clear it before returning only when the record type holds pointers, so it does not keep user objects alive. A key-function panic propagates with the slice contents unspecified ([semantics](../conformance/semantics.md#go)), so the engine has no recovery paths and no panic-restoration tests. Normal returns keep every record, including under inconsistent keys.
- **SIMD.** Vector code uses Go's portable `simd` package (`GOEXPERIMENT=simd`) behind internal functions with a plain Go counterpart for builds without the experiment. Do not write per-ISA `simd/archsimd` kernels or expose a backend choice; users never choose a kernel.
- **Parallelism.** Parallel entry points use goroutines sized by `GOMAXPROCS`; key functions may run concurrently.
- **Shared rules.** A change to a shared rule, such as routing, a threshold or digit planning, updates both implementations in the same PR, or records the remaining divergence in the [roadmap](../ROADMAP.md).
- **Documentation.** Exported items have doc comments stating their contract, including key-function calls, panics and memory, and runnable `Example` functions with `// Output:`.

## Validation

Tests cover ordering, stability, inconsistent keys, pointer-bearing records and the golden vectors. Run from `tessera/`:

```sh
gofmt -l . && go vet ./... && go test ./...
GOEXPERIMENT=simd go vet ./... && GOEXPERIMENT=simd go test ./...
```

Parallel changes also run `go test -race ./...`; engine changes run each fuzz target briefly, for example `go test . -run '^$' -fuzz '^FuzzOrdering$' -fuzztime=30s`. A change to `conformance/vectors/` also runs `cargo test -p tessera-order --test golden_vectors`.
