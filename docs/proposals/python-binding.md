---
type: Proposal
title: Python binding
description: Implement the planned Python adapters with explicit buffer safety.
tags: [python, api, planning, safety]
status: draft
---

# Proposal: Python binding

Status: accepted, not started. Date: 2026-11-06.

The binding wraps the Rust crate and does not wait for the C++ implementation. It has its own package version and `python/vX.Y.Z` tags; its stable and unstable functions share that version.

## First release

Support `list`, standard-library `array.array`, and one-dimensional numeric `numpy.ndarray`. Provide six functions:

| Operation | Stable | Unstable | Result |
|---|---|---|---|
| Modify input | `stable_sort` | `unstable_sort` | `None` |
| Preserve input, return sorted copy | `stable_sorted` | `unstable_sorted` | New container |
| Preserve input, return indices | `stable_argsort` | `unstable_argsort` | Index sequence |

Each function accepts `values, *, key=None, reverse=False`. These return and mutation rules are identical for all supported container families.

| Input | `*_sorted` output | `*_argsort` output |
|---|---|---|
| `list` | `list`, retaining original objects | `list[int]` |
| `array.array` | `array.array` with the same typecode | `list[int]` |
| `numpy.ndarray` | `ndarray` with the same dtype | `ndarray` with dtype `np.intp` |

NumPy is optional. Importing the package and using lists or `array.array` must work without NumPy installed. No comparator fallback is provided. Unsupported keys, dtypes and layouts raise explicit exceptions.

The first Python release may implement `unstable_*` by calling the stable Rust kernel. Its equal-key order remains unspecified by contract, and it makes no speed advantage claim. Document and benchmark the actual kernel. This does not add unstable interfaces to Rust 0.1, whose scope remains stable sorting only.

## Container adapters

### Lists

Numeric lists and arbitrary object lists with a numeric `key=` are supported. Save strong references to the original objects, extract each numeric key once, sort keys and original indices with Tessera, then apply the permutation to the original references. Preserve object identity, numeric subclasses, duplicate references and the order of equal keys under the stable contract. Never replace original Python numbers with newly constructed numbers just to apply a sort.

On a successful call, each element's key is evaluated exactly once. No radix pass calls back into Python. Key extraction and validation finish before the binding writes to the list. A failing key or range check leaves the binding's writeback unstarted; callback side effects are not rolled back. Reentrant mutation of the source by a callback must be detected and given an explicit failure contract before implementation; do not overwrite a callback's edits using stale list positions.

`key=` primarily serves lists in the first release. Array callers should compute numeric key arrays with array operations and use `argsort` to reorder associated columns or objects. Do not add `keys=`, `order=` or field-list APIs in the first release.

### `array.array`

Use the buffer protocol to access integer, `f` and `d` storage directly. Check buffer format, actual `itemsize`, dimensionality, contiguity, alignment, writability and byte order before dispatching once to a Rust numeric entry. Do not create an intermediate Python list or per-element Python numbers. Platform-dependent typecodes such as `l` and `L` must use their actual width. The exact supported typecode/width table is tested per platform; incompatible formats are rejected rather than reinterpreted.

### NumPy

Start with one-dimensional `int8/16/32/64`, `uint8/16/32/64`, and `float32/64`.

| Layout | In-place sort | Sorted copy and argsort |
|---|---|---|
| Writable, contiguous, aligned, native-endian numeric buffer | Direct buffer | Supported |
| Noncontiguous one-dimensional view, including negative stride | Reject | Read in logical view order |
| Read-only numeric array | Reject | Supported |
| Unaligned or non-native-endian numeric array | Reject | Convert into owned aligned/native working storage; preserve output dtype |
| Multidimensional array | Deferred pending an `axis` contract | Deferred |
| Object, string, complex or structured dtype | No direct sort | Extract a supported numeric key array, then argsort it |

Indices for a view are relative to that view, not its backing array. Sorted copies preserve dtype, including byte order; any working-storage conversion and its cost are included in the interface's space and timing measurements. No noncontiguous input is silently treated as a contiguous slice.

## Numeric semantics

Python integers are arbitrary precision. Inspect the whole key range and use one lossless common representation from `i64`, `u64`, `i128` or `u128`. Raise `OverflowError` if none covers the full range, including a mixture of negative keys and unsigned values above `i128::MAX`. Never truncate or silently convert large integers to floating point.

The first release rejects mixed integer/float key sets unless the caller explicitly normalizes them, for example with `key=float`. Strings, `Decimal`, `Fraction` and arbitrary comparison objects are not direct numeric keys.

Python floating-point ordering is an adapter contract, distinct from the Rust API's bitwise `total_cmp` order:

| Case | Python rule |
|---|---|
| Ordinary numeric values | Numeric ascending or descending order |
| NaN | Last in both directions |
| `-0.0` and `+0.0` | Equal sorting keys |
| NaNs, zeros and other equal keys in stable operations | Preserve input order |
| Original values | Preserve bits, including NaN payloads |

Normalize only sorting keys; move the original values or object references. A float buffer can use the Rust by-key API with a library-owned unsigned key encoder, so no Python callback enters the radix loop and no full key copy is required just to normalize float order. `reverse=True` changes key encoding; it must not reverse the sorted result. The NaN sentinel remains last in both directions. This policy is Tessera's Python contract, not a claim that every NumPy descending idiom has identical behavior.

## Binding and buffer safety

Use PyO3, rust-numpy and maturin. Keep three adapters: list extraction/reference writeback, standard buffer validation/access, and NumPy dtype/layout/borrow and output handling. They all call the same Rust Tessera crate. Container/type checks happen at entry, outside the sort loops.

Buffer access needs a separate safety design. A Python buffer export does not by itself grant an exclusive Rust mutable reference. Audit the actual pinned PyO3/rust-numpy versions and their current APIs before implementing access; do not copy legacy `PyBuffer::as_slice`/`as_mut_slice` examples with known aliasing problems. Concentrate unsafe pointer-to-slice conversion in one small adapter with explicit layout, lifetime and aliasing invariants.

- Keep interpreter access while extracting Python keys and manipulating object references.
- Rust-owned key/index buffers or copies may sort while detached from Python.
- Direct external-buffer sorting initially keeps the GIL and requires no concurrent access through any alias for the call's duration.
- The GIL cannot stop NumPy or C threads that already released it, and rust-numpy's borrow tracking cannot exclude every foreign alias. Validate what the binding can enforce and document the remaining caller obligation.
- The first release requires a GIL-enabled access model. Use the selected PyO3 version's explicit GIL requirement (currently spelled `gil_used = true`) and verify behavior on free-threaded interpreters; do not advertise a free-threaded wheel until that access design has been validated separately.

## Space and benchmarks

Do not advertise O(√n) auxiliary memory for the whole Python package.

| Path | Space accounting |
|---|---|
| Contiguous numeric in-place sorting | Core O(√n) auxiliary space plus bounded adapter/core storage |
| Sorted copy | O(n) output, plus sorting scratch |
| Argsort | O(n) index output; record any additional key storage |
| List sorting | O(n) key/index/reference adaptation storage |

Measure the entire public call, including extraction, allocation, conversion, sort and writeback. Compare numeric lists to `list.sort()`, keyed object lists to `list.sort(key=...)`, NumPy in-place sorting to `ndarray.sort(kind="stable")`, sorted copies to `np.sort(..., kind="stable")`, and indices to `np.argsort(..., kind="stable")`. Include A/A controls, validate equivalent semantics and report unsupported cases. No speed claim follows merely from using Rust or radix sort; prioritize continuous numeric buffers, then measure the net benefit for lists. Array benchmarks also include an explicit matching standard-library or NumPy conversion baseline, with conversion cost visible.

## Repository and release integration

```text
python/pyproject.toml             package metadata and maturin build settings
python/Cargo.toml                 unpublished PyO3 extension crate
python/src/                      Rust binding and adapters
python/python/tessera_sort/       exports, type stubs and py.typed
python/tests/                    API, identity, buffer and ordering tests
bench/python/                    end-to-end benchmarks
```

The CI router reserves Python independently of Rust and C++. Enable Python checks only when its project manifests exist. Changes to the Rust core also select Python validation once the binding exists; they do not automatically release Python. Shared contract changes test each implemented language with its applicable adapter semantics. Python tags validate and build Python only, including the embedded Rust core needed by the binding.

Use a maturin wheel matrix for mainstream Linux, macOS and Windows, with NumPy present and absent in API tests. Pin the build/toolchain policy at implementation time. Wheel build records identify Python package version, actual Rust core package version and source commit, binding commit, ABI, Python version, toolchain and build flags. Prefer versioned published core dependencies for releases while testing the local core in development. The release process must verify which dependency source the wheels actually used.

### Delivery phases

1. Specify and test numeric encodings, return types and exception behavior; audit buffer exclusivity with the selected binding versions.
2. Implement continuous `array.array` and NumPy numeric paths and argsort.
3. Implement list key extraction, identity-preserving writeback and the six symmetric functions, documenting stable-kernel aliases for unstable APIs.
4. Validate identity and repeated references, exact key-call counts, extraction failure and callback mutation, integer bounds, mixed key rejection, float bits and stable reverse order, read-only/noncontiguous views, array widths, dtype errors and NumPy-free imports.
5. Measure end-to-end costs, build/install/test platform wheels, and publish Python independently after its own release checks.

Multidimensional `axis`, direct structured-dtype/multicolumn sorting and a Python workspace remain deferred until real use cases justify them. Internal tile, digit and insertion parameters are not Python API.
