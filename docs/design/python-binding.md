# Python binding design

Status: implemented. The code and tests in `python/` are authoritative for runtime behavior; this page keeps the design rationale and contracts.

The binding wraps the Rust crate and does not wait for the C++ implementation. Its package version synchronizes `X.Y.Z` with the embedded `tessera-order` Rust crate (`python/v0.2.0` embeds Rust `0.2.0`), using PEP 440 post-releases (`python/v0.2.0.post1`) for Python-only adapter or packaging fixes ([releases](../../RELEASING.md)). The module exposes both `__version__` (Python package version) and `__core_version__` (embedded Rust crate version).

## Public API

Support `list`, standard-library `array.array`, and `numpy.ndarray` of any dimension. Every operation uses Tessera's stable radix engine, so no-op or unsupported compatibility flags (`kind`, `stable`, `order`, `out`) are omitted. Copy-returning `sort` and `argsort` follow NumPy's names and `axis`/`descending` semantics; in-place sorting is the dedicated `sort_inplace` function returning `None`.

| Function | Counterpart | Result |
|---|---|---|
| `sort(a, axis=-1, *, descending=None, key=None, threads=1)` | `numpy.sort` / `sorted` | A sorted copy of `a` |
| `sort_inplace(a, axis=-1, *, descending=None, key=None, threads=1)` | `ndarray.sort` / `list.sort` | Mutates `a` in place and returns `None` |
| `argsort(a, axis=-1, *, descending=None, key=None, threads=1)` | `numpy.argsort` | Sorted indices |
| `partition(a, kth, axis=-1, *, descending=None, key=None, threads=1)` | `numpy.partition` | A partitioned copy with `kth` rank(s) in final sorted position |
| `partition_inplace(a, kth, axis=-1, *, descending=None, key=None, threads=1)` | `ndarray.partition` | Mutates `a` in place around `kth` and returns `None` |
| `argpartition(a, kth, axis=-1, *, descending=None, key=None, threads=1)` | `numpy.argpartition` | Indices partitioning `a` around `kth` |

Parameter contracts (shared by one argument parser across all six functions):

- `kth` (for `partition`, `partition_inplace`, `argpartition`): an integer or a sequence of integers (negative ranks index from the end of the lane; `bool` raises `TypeError`; any out-of-bounds rank, including any rank on an empty lane, raises `ValueError`). Duplicate ranks are deduplicated; an empty sequence is a valid no-op. Single `kth` calls `tessera_order::select_nth` / `select_nth_by_key` directly (`argpartition` runs `select_nth_by_key` on `0..len` indices without a full sort). Sequence `kth` sorts and deduplicates normalized ranks and recursively partitions around the median rank on disjoint sub-slices, relying on `select_nth`'s equal-key stability across the whole slice to preserve stable rank semantics without falling back to a full sort.
- `axis`: default `-1`.
  - On `list` and `array.array` (1D): accepts `-1`, `0`, or `None` (for `sort`, `argsort`, `partition`, `argpartition`; `sort_inplace` and `partition_inplace` reject `axis=None` with `TypeError`, matching `ndarray.sort` / `ndarray.partition`). Out-of-bounds axes raise `ValueError`; `bool` raises `TypeError`.
  - On `numpy.ndarray` (any `ndim >= 0`):
    - Integer `axis` in `[-ndim, ndim)` operates on each 1D lane along that axis independently (0D arrays have no valid integer axis and raise `ValueError`, matching NumPy).
    - `axis=None` flattens the array in C-order and returns a 1D result for `sort`, `argsort`, `partition`, and `argpartition` (including 0D arrays and broadcast views). For `sort_inplace` and `partition_inplace`, `axis=None` raises `TypeError` because in-place operations cannot change array shape.
- `descending`: `None` (default) or `False` sorts ascending; `True` sorts descending with NaN still last (matching NumPy 2.5). Non-bool values raise `TypeError`. Descending changes key encoding and never reverses an ascending result. In `partition*`/`argpartition`, `descending=True` selects ranks in that descending order (so `kth=0` selects the maximum, and NaNs remain at the highest ranks).
- `key`: optional callable (`None` by default). When `None`, numeric buffers run the zero-callback Rust path and numeric lists extract numbers directly. When provided, `key` runs a unified cached-key path across `list`, `array.array`, and **any `numpy.ndarray` dtype** (including structured, custom, string, and `object` dtypes): each element's key is evaluated once, ordered or partitioned in Rust, and the resulting permutation reorders the elements. Because `key` invokes Python per element, it trades speed for expressiveness rather than matching zero-callback numeric buffer throughput.
- `threads`: `int | None = 1`.
  - `1` (default): serial Rust engine with zero thread-pool involvement, matching single-threaded `np.sort` and avoiding oversubscription inside worker processes.
  - `None`: Rayon global thread pool (respects `RAYON_NUM_THREADS`, defaults to available hardware parallelism).
  - Integer `>= 2`: runs in a single-slot cached Rayon pool of that exact width (`Mutex<Option<(usize, Arc<ThreadPool>)>>`), so repeated calls with the same thread count pay no pool-creation cost and changing the count drops the old pool without accumulating idle threads.
  - `bool` raises `TypeError`; integers `< 1` raise `ValueError`.

| Input | `sort` / `partition` output | `sort_inplace` / `partition_inplace` | `argsort` / `argpartition` output |
|---|---|---|---|
| `list` | `list`, retaining original objects | Mutates `list` in place, returns `None` | `list[int]` |
| `array.array` | `array.array` with the same typecode | Mutates buffer in place, returns `None` | `list[int]` |
| `numpy.ndarray` | `ndarray` with the same dtype (1D when `axis=None`, else same shape) | Mutates `ndarray` in place, returns `None` | `ndarray` with dtype `np.intp` (1D when `axis=None`, else same shape) |

NumPy is optional. Importing the package and using lists or `array.array` must work without NumPy installed. No comparator fallback is provided. Unsupported containers, dtypes, keys, and layouts raise explicit exceptions.

## Container adapters

### Lists

Numeric lists (`key=None`) and arbitrary object lists (`key=callable`) are supported. Snapshot strong references to the original objects, extract each numeric key once, sort keys and original indices with Tessera, then apply the permutation to the original references. Preserve object identity, numeric subclasses (including `bool`, a Python `int` subclass), duplicate references, and equal-key order. Never replace original Python numbers with newly constructed numbers.

On a successful call, each element's key is evaluated exactly once. No radix pass calls back into Python. Key extraction and validation finish before any writeback begins; a failing key or range check leaves writeback unstarted (callback side effects are not rolled back).

Reentrant mutation contract: before calling `key`, snapshot the input list's strong references and raw element pointers. Check the list length immediately before and after each `key` callback. After extraction and again before in-place writeback, verify the length and every element pointer against the snapshot. These full checks take O(n) total work, rather than scanning the list for each callback. A detected mutation raises `ValueError("list modified during sort")` without writing back stale positions. The `array.array` and `ndarray` `key=` paths also detect in-place mutations before writeback.

### `array.array`

For `key=None`, use the PEP 3118 buffer protocol to access signed/unsigned integer, `f`, and `d` storage directly. Check buffer format, actual `itemsize`, dimensionality, contiguity, alignment, writability, and byte order before dispatching once to a Rust numeric entry. Platform-dependent typecodes such as `l` and `L` use their actual width; unsupported formats (such as Unicode `'u'`/`'w'`) raise `TypeError` when `key=None`. With `key=callable`, extract numeric keys once per element, sort indices in Rust, and permute the array's raw `itemsize` elements (or write into a new `array.array` of the same typecode).

### NumPy

Detect `numpy.ndarray` only when `"numpy"` is already present in `sys.modules`, so calling the binding without NumPy installed never triggers an import.

With `key=None`, direct numeric dtypes are `int8/16/32/64`, `uint8/16/32/64`, and `float32/64`:

| Layout | `sort_inplace` | `sort` (copy) and `argsort` |
|---|---|---|
| Writable, aligned, native-endian lane (`strides[ax] == itemsize`, no self-overlap) | Direct in-place sort per lane (zero gather/scatter) | Supported (contiguous copy + in-place sort on C-contiguous `axis=-1`) |
| Writable strided, negative-stride, unaligned, or non-native-endian lane (no self-overlap) | Gather each lane to 1D native aligned scratch, sort, scatter back (preserving dtype/byte order) | Read each lane in logical view order, sort/argsort, write output preserving dtype |
| Read-only numeric array | Reject (`ValueError`) | Supported |
| Broadcast or self-overlapping writable view (e.g. `stride == 0` with `shape > 1` or overlapping coordinates) | Reject (`ValueError`) | Supported (read-only traversal) |
| Unsupported dtype (`bool`, `complex`, string, structured, `object`) with `key=None` | Reject (`TypeError`) | Reject (`TypeError`) |

With `key=callable`, **any `numpy.ndarray` dtype** (including structured, custom, string, `bool`, `complex`, and `object`) is supported along any valid `axis` (and `axis=None` for `sort`/`argsort`):

- For non-object dtypes (`!dtype.hasobject`), elements are plain `itemsize`-byte values: extract keys once per lane element, sort indices in Rust, and permute the raw element bytes into the output (or back into `a` for `sort_inplace`, rejecting read-only / self-overlapping views).
- For object-containing dtypes (`dtype.hasobject`), never raw-`memcpy` `PyObject*` bytes; perform refcount-safe permutation through Python/NumPy element operations after sorting indices in Rust.

Indices for a view or axis lane are relative to that lane (or flattened C-order for `axis=None`), not the backing array. Sorted copies preserve dtype including byte order by allocating output with `numpy.empty(shape, dtype=a.dtype)`.

## Numeric semantics

Python integers are arbitrary precision. Inspect the whole key range in one pass and widen losslessly across `i64`, `u64`, `i128`, or `u128`. Raise `OverflowError` if none covers the full range, including a mixture of negative keys and unsigned values above `i128::MAX`. Never truncate or silently convert large integers to floating point.

Mixed integer/float key sets raise `TypeError` unless the caller explicitly normalizes them (for example with `key=float`). Strings, `Decimal`, `Fraction`, and arbitrary comparison objects are not direct numeric keys (`TypeError`).

Python floating-point ordering is an adapter contract, distinct from the Rust API's bitwise `total_cmp` order:

| Case | Python rule |
|---|---|
| Ordinary numeric values | Numeric ascending or descending order |
| NaN | Last in both directions |
| `-0.0` and `+0.0` | Equal sorting keys |
| NaNs, zeros, and other equal keys | Preserve input order |
| Original values | Preserve bits, including NaN payloads |

Normalize only sorting keys; move original values or object references. Exact unsigned encoding for `f64` (and identically for `f32`):

- Canonicalize `-0.0` (`0x8000_0000_0000_0000`) to `+0.0` (`0x0000_0000_0000_0000`).
- If `x.is_nan()`, map to `u64::MAX` (`0xFFFF_FFFF_FFFF_FFFF`) in **both** ascending and descending order.
- Otherwise, ascending encoding flips all bits when negative and flips only the sign bit when non-negative, mapping `[-inf, +inf]` into `[0x000F_FFFF_FFFF_FFFF, 0xFFF0_0000_0000_0000]`; descending encoding bitwise-inverts that ascending key, keeping `[-inf, +inf]` strictly below `u64::MAX`.

Because `u64::MAX` never collides with any finite or infinite value in either direction, float buffers with NaNs or mixed `±0.0` call Rust's `sort_by_key` / `argsort_by_key` / `select_nth_by_key` / `par_*_by_key` directly with a pure-Rust key encoder: no Python callback enters the radix loop and no extra key copy is allocated. When a float slice has no NaNs and does not mix `-0.0` and `+0.0`, `total_cmp` (ascending) and `Reverse<f32>` / `Reverse<f64>` (descending) match NumPy order bit for bit, so the binding automatically takes the direct-key SIMD fast path; descending integer slices likewise use transparent `Reverse<T>` views to retain direct SIMD.

## Binding, buffer safety, and Rayon

Use **PyO3 0.30** and **maturin** directly, without `rust-numpy`. One unified PEP 3118 buffer layer (`buffer.rs`) serves both `array.array` and `numpy.ndarray`, avoiding duplicate buffer/endianness logic, avoiding `ndarray`/`rust-numpy` dependency weight, and supporting NumPy 1.x and 2.x through the stable buffer protocol.

All unsafe raw-pointer-to-slice and strided-element access is isolated inside `buffer.rs` with explicit layout, lifetime, alignment, writability, and non-overlap invariants (using raw `Py_buffer`, never PyO3's deprecated `as_slice`/`as_mut_slice`):

- Keep interpreter access while extracting Python keys, checking container metadata, and reading/writing Python object references.
- Rust-owned scratch and key/index buffers (list ordering, `key=` ordering, gathered strided/non-native lanes) sort or partition while detached from the GIL (`py.detach(...)`), both serially and with Rayon.
- Direct external-buffer operations keep the GIL held on the calling thread for the call's duration. These include contiguous native `sort` / `partition` copies that write directly into pinned output buffers, `sort_inplace` / `partition_inplace`, and zero-copy `argsort` / `argpartition`. When `threads != 1`, Rayon worker threads execute only pure-Rust numeric kernels and encoders and never acquire the GIL, so holding the GIL on the caller thread prevents concurrent Python bytecode mutation of the buffer without deadlock risk.
- For multi-lane N-D arrays when `threads != 1`: single-lane inputs call `par_sort` / `par_argsort`; multi-lane inputs parallelize across independent disjoint lanes inside the selected Rayon pool (and large lanes can nest `par_sort`/`par_argsort` cleanly through Rayon work-stealing).
- The GIL cannot stop foreign C or NumPy threads that already released it; document that concurrent C-level mutation of the same buffer during a call is undefined by the caller.
- Require a GIL-enabled interpreter with `#[pymodule(gil_used = true)]` (PyO3 0.28+ defaults to `gil_used = false`), and do not advertise free-threaded wheels until a separate no-GIL buffer access design is validated.

## Space and benchmarks

Do not advertise O(√n) auxiliary memory for the whole package: contiguous native `sort_inplace` uses the core's O(√n) (or O(P√n) when `threads != 1`) plus bounded adapter storage; strided/non-native `sort_inplace` / `partition_inplace` adds O(lane_len) gather/scatter scratch when serial and O(min(P, num_lanes) * lane_len) when `threads != 1`, gathering at most one lane per worker (P is the selected pool's worker count); `sort` (copy) adds O(n) output plus sorting scratch; `argsort` adds O(n) index output; and list or `key=` sorting adds O(n) key/index/reference storage.

Measure the entire public call (extraction, allocation, conversion, sort, writeback) against `list.sort()`, `list.sort(key=...)`, `ndarray.sort(kind="stable")`, `np.sort(..., kind="stable")`, and `np.argsort(..., kind="stable")`, with A/A controls, validated equivalent semantics, reported unsupported cases, and conversion costs visible.

## Repository and release

The binding lives in `python/` (excluded from the root Cargo workspace, with its own `Cargo.toml`, `Cargo.lock`, `pyproject.toml`, `tessera_order` Python package with `.pyi` stubs and `py.typed`, and `tests/`) with benchmarks in `bench/python/`.

- **Python support & wheels**: `requires-python = ">=3.9"`. Build per-version wheels for `cp39` and `cp310`, and a single `cp311-abi3` wheel covering Python 3.11+ (`Py_buffer` entered the Limited API in 3.11; the Rust source is identical across both builds).
- **CI & release**: The CI router treats Python independently of Rust and C++; Rust core and shared Cargo configuration changes select Python validation without releasing it. Python tags (`python/vX.Y.Z` or `python/vX.Y.Z.postN`) verify version alignment with `rust/tessera-order`, test each release wheel with NumPy and each `cp311-abi3` wheel without NumPy, and test installation from the sdist outside the checkout before publication ([release workflows](../../RELEASING.md#workflows)).
