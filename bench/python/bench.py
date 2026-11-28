# SPDX-License-Identifier: BSD-2-Clause
"""End-to-end benchmarks; install NumPy and tessera-order before running.

    python3 bench/python/bench.py --quick --json /tmp/tessera-python.json

Input generation, validation and per-call input resets are untimed. All work
inside each public call, including output allocation and conversion, is timed.
"""

import argparse
import hashlib
import json
import platform
import sys
from array import array
from functools import partial
from pathlib import Path
from statistics import median
from time import perf_counter_ns

import numpy as np

SEED = 20261128
DISTRIBUTIONS = ("random", "few_unique", "nearly_sorted", "descending")
# NumPy < 2.0 spells the same stable contract kind="stable".
try:
    np.sort(np.empty(0), stable=True)
    STABLE = {"stable": True}
except TypeError:
    STABLE = {"kind": "stable"}

try:
    np.sort(np.empty(0), descending=True, **STABLE)
    NUMPY_DESCENDING = True
except TypeError:
    NUMPY_DESCENDING = False


def positive_csv(text):
    try:
        values = [int(value) for value in text.split(",")]
        if not values or any(value < 1 for value in values):
            raise ValueError
    except ValueError as error:
        raise argparse.ArgumentTypeError(
            "expected comma-separated positive integers"
        ) from error
    return list(dict.fromkeys(values))


def make_input(size, dtype, distribution, special=True):
    rng = np.random.default_rng(SEED)
    dtype = np.dtype(dtype)
    if distribution == "few_unique":
        values = rng.integers(0, 16, size=size).astype(dtype)
    elif dtype.kind == "f":
        values = rng.standard_normal(size).astype(dtype)
    else:
        bounds = np.iinfo(dtype)
        values = rng.integers(
            bounds.min, bounds.max, size=size, dtype=dtype, endpoint=True
        )
    if distribution in ("nearly_sorted", "descending"):
        values.sort(**STABLE)
        if distribution == "descending":
            values = values[::-1].copy()
        else:
            positions = rng.integers(0, size, size=(2, max(1, size // 100)))
            values[positions[0]], values[positions[1]] = (
                values[positions[1]].copy(),
                values[positions[0]].copy(),
            )
    if dtype.kind == "f":
        # Signed zeros also exercise stable ties in Python's finite-float cases.
        edges = np.array([-0.0, 0.0, -0.0, 0.0], dtype=dtype)
        if special:
            uint = np.dtype(f"u{dtype.itemsize}")
            exponent = 0x7FF0000000000000 if dtype.itemsize == 8 else 0x7F800000
            sign = 1 << (dtype.itemsize * 8 - 1)
            # Distinct signed NaN payloads must survive unchanged.
            payloads = np.array(
                [exponent | 1, exponent | 31, sign | exponent | 17], dtype=uint
            )
            edges = np.concatenate(
                (edges, np.array([-np.inf, np.inf], dtype=dtype), payloads.view(dtype))
            )
        count = min(size, len(edges))
        values[np.linspace(0, size - 1, count, dtype=int)] = edges[:count]
    return values


def assert_bits(actual, expected):
    actual, expected = np.asarray(actual), np.asarray(expected)
    assert actual.shape == expected.shape and actual.dtype == expected.dtype
    np.testing.assert_array_equal(actual, expected)
    assert actual.tobytes() == expected.tobytes(), "value bits differ"


def order_keys(values):
    """Numeric order with signed zeros equal and all NaNs last."""
    if values.dtype.kind != "f":
        return values
    uint = np.dtype(f"u{values.itemsize}")
    bits = values.view(uint).copy()
    bits[values == 0] = 0
    sign = np.array(1 << (values.itemsize * 8 - 1), dtype=uint)
    keys = np.where(bits & sign, ~bits, bits ^ sign)
    keys[np.isnan(values)] = np.iinfo(uint).max
    return keys


def validate(
    source, result, tessera, *, operation, descending=False, key=None, kth=None
):
    if isinstance(source, np.ndarray):
        assert isinstance(result, np.ndarray)
        assert result.shape == source.shape
        if operation in ("argsort", "argpartition"):
            assert result.dtype == np.dtype(np.intp)
        else:
            assert result.dtype == source.dtype
        if "partition" in operation:
            if operation == "argpartition":
                np.testing.assert_array_equal(np.sort(result), np.arange(len(source)))
                values = source[result]
            else:
                values = result
            # NumPy partitions are unstable: layouts and equal-key pivot bits can
            # differ. Require an exact bit multiset and the same ordered rank.
            uint = np.dtype(f"u{source.itemsize}")
            assert np.array_equal(
                np.sort(values.view(uint)), np.sort(source.view(uint))
            ), "partition did not preserve the exact input bit multiset"
            keys = order_keys(values)
            expected_keys = np.sort(order_keys(source))
            assert keys[kth] == expected_keys[kth], "incorrect selected rank"
            assert np.all(keys[:kth] <= keys[kth]), "left partition exceeds pivot"
            assert np.all(keys[kth + 1 :] >= keys[kth]), (
                "right partition precedes pivot"
            )
            if tessera:
                order = np.argsort(source, **STABLE)
                assert_bits(values[kth : kth + 1], source[order[kth : kth + 1]])
                assert_bits(np.sort(values, **STABLE), source[order])
                if operation == "argpartition":
                    assert result[kth] == order[kth], "unstable selected index"
                    np.testing.assert_array_equal(
                        result[np.argsort(keys, **STABLE)], order
                    )
            return
        options = dict(STABLE)
        if descending:
            options["descending"] = True
        order = np.argsort(source, **options)
        assert_bits(result, order if operation == "argsort" else source[order])
        return
    expected = sorted(source, key=key, reverse=descending)
    assert type(result) is type(source)
    if isinstance(source, array):
        assert result.typecode == source.typecode
        assert_bits(result, array(source.typecode, expected))
    else:
        # List calls must retain the original objects and equal-key order.
        assert len(result) == len(expected)
        assert all(a is b for a, b in zip(result, expected)), (
            "object identity/order differs"
        )
        if key is None:
            assert_bits(result, expected)


def benchmark(name, source, baseline, candidate, check, rounds, inplace=False):
    def fresh_input():
        # ndarray slicing creates a view; each in-place call needs owned storage.
        return source.copy() if isinstance(source, np.ndarray) else source[:]

    # Validate both complete calls before warmup, including return contracts and
    # preservation of the source for copy-returning operations.
    for is_tessera, call in ((False, baseline), (True, candidate)):
        work = fresh_input()
        try:
            output = call(work)
            if inplace:
                assert output is None, "in-place call must return None"
                output = work
            else:
                if isinstance(source, list):
                    assert all(a is b for a, b in zip(work, source))
                    assert len(work) == len(source)
                else:
                    assert_bits(work, source)
            check(source, output, is_tessera)
        except (AssertionError, TypeError, NotImplementedError) as error:
            if is_tessera:
                raise
            reason = f"baseline {'validation failed' if isinstance(error, AssertionError) else 'unsupported'}: {error}"
            print(f"| {name} | unsupported | — | — |", flush=True)
            print(f"{name}: {reason}", file=sys.stderr)
            return {"case": name, "status": "unsupported", "reason": reason}
        del output, work
    # Warm each implementation on fresh input; Rayon pool creation is untimed.
    for call in (baseline, candidate):
        call(fresh_input())
    samples = [[], []]
    for round_index in range(rounds):
        # Adjacent AB/BA rounds reduce systematic order/drift bias.
        for index in (0, 1) if round_index % 2 == 0 else (1, 0):
            work = fresh_input()
            call = (baseline, candidate)[index]
            start = perf_counter_ns()
            output = call(work)
            elapsed_ms = (perf_counter_ns() - start) / 1_000_000
            samples[index].append(elapsed_ms)
            del output, work  # Output destruction is outside the public call.
    summaries = []
    for times in samples:
        q1, q3 = np.percentile(times, [25, 75])
        summaries.append(
            {"median_ms": median(times), "iqr_ms": float(q3 - q1), "samples_ms": times}
        )
    base, test = summaries
    speedup = base["median_ms"] / test["median_ms"]
    print(
        f"| {name} | {base['median_ms']:.3f} ({base['iqr_ms']:.3f}) "
        f"| {test['median_ms']:.3f} ({test['iqr_ms']:.3f}) | {speedup:.2f}x |",
        flush=True,
    )
    return {
        "case": name,
        "status": "ok",
        "aa_control": baseline is candidate,
        "baseline": base,
        "tessera": test,
        "speedup": speedup,
    }


def cases(size, thread_counts, ts):
    for dtype in ("int64", "uint64", "float64", "float32"):
        for distribution in DISTRIBUTIONS:
            source = make_input(size, dtype, distribution)
            label = f"ndarray/{dtype}/{distribution}/n={size}"
            control = partial(np.sort, **STABLE)
            yield (
                f"A/A {label}/np.sort vs np.sort",
                source,
                control,
                control,
                partial(validate, operation="sort"),
                False,
            )
            for threads in thread_counts:
                for operation in (
                    "sort",
                    "sort_inplace",
                    "argsort",
                    "partition",
                    "partition_inplace",
                    "argpartition",
                ):
                    for descending in (
                        (False, True)
                        if operation == "sort" and NUMPY_DESCENDING
                        else (False,)
                    ):
                        options = (
                            dict(STABLE)
                            if "partition" not in operation
                            else {"kth": size // 2}
                        )
                        if descending:
                            options["descending"] = True
                        inplace = operation.endswith("_inplace")
                        numpy_name = operation.removesuffix("_inplace")
                        if inplace:
                            baseline = lambda a, name=numpy_name, opts=options: getattr(
                                a, name
                            )(**opts)
                        else:
                            baseline = partial(getattr(np, numpy_name), **options)
                        kwargs = {"threads": threads, "descending": descending}
                        if "partition" in operation:
                            kwargs["kth"] = size // 2
                        candidate = partial(getattr(ts, operation), **kwargs)
                        check = partial(
                            validate,
                            operation=operation,
                            descending=descending,
                            kth=size // 2,
                        )
                        direction = "/desc" if descending else ""
                        yield (
                            f"{label}/{operation}{direction}/t={threads}",
                            source,
                            baseline,
                            candidate,
                            check,
                            inplace,
                        )
    # Python sorted/list.sort do not share NumPy's NaN ordering contract. Use
    # finite floats (including both zero signs) for these equivalent comparisons.
    for distribution in DISTRIBUTIONS:
        for kind, dtype in (("int", "int64"), ("float", "float64")):
            values = make_input(size, dtype, distribution, special=False).tolist()
            containers = [(f"list/{kind}", values)]
            typecode = "q" if kind == "int" else "d"
            containers.append((f"array/{typecode}", array(typecode, values)))
            for container, source in containers:
                for threads in thread_counts:
                    for operation in ("sort", "sort_inplace"):
                        inplace = operation == "sort_inplace"
                        if isinstance(source, array):
                            if inplace:

                                def baseline(a):
                                    a[:] = array(a.typecode, sorted(a))
                            else:
                                baseline = lambda a: array(a.typecode, sorted(a))
                        else:
                            baseline = list.sort if inplace else sorted
                        candidate = partial(getattr(ts, operation), threads=threads)
                        name = f"{container}/{distribution}/n={size}/{operation}/t={threads}"
                        yield (
                            name,
                            source,
                            baseline,
                            candidate,
                            partial(validate, operation=operation),
                            inplace,
                        )
        records = [
            {"key": value, "index": i}
            for i, value in enumerate(
                make_input(size, "int64", distribution, special=False).tolist()
            )
        ]
        key = lambda record: record["key"]
        for threads in thread_counts:
            name = f"list/object/{distribution}/n={size}/sort_inplace(key)/t={threads}"
            yield (
                name,
                records,
                lambda a, key=key: a.sort(key=key),
                partial(ts.sort_inplace, key=key, threads=threads),
                partial(validate, operation="sort_inplace", key=key),
                True,
            )


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="use sizes 10000,100000 unless --sizes is set",
    )
    parser.add_argument(
        "--sizes",
        type=positive_csv,
        help="comma-separated sizes (default: 10000,100000,1000000)",
    )
    parser.add_argument(
        "--threads",
        type=positive_csv,
        default=[1, 4],
        help="Tessera thread counts (default: 1,4)",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=5,
        help="timed rounds per implementation (default: 5; minimum: 2)",
    )
    parser.add_argument(
        "--json",
        type=Path,
        help="raw samples and metadata; path must be outside the repository",
    )
    args = parser.parse_args()
    if args.rounds < 2:
        parser.error("--rounds must be at least 2")
    sizes = args.sizes or (
        [10_000, 100_000] if args.quick else [10_000, 100_000, 1_000_000]
    )
    if args.json and args.json.resolve().is_relative_to(
        Path(__file__).resolve().parents[2]
    ):
        parser.error(
            "--json must be outside the repository, for example /tmp/tessera-python.json"
        )
    try:
        import tessera_order as ts
    except ImportError as error:
        parser.exit(
            2,
            f"Install tessera-order in this interpreter before benchmarking: {error}\n",
        )
    build_info = ts._core._build_info()
    artifact_path = Path(ts._core.__file__).resolve()
    verified = build_info["commit"] != "unknown" and build_info["dirty"] == "false"
    metadata = {
        **build_info,
        "artifact_path": str(artifact_path),
        "artifact_sha256": hashlib.sha256(artifact_path.read_bytes()).hexdigest(),
        "numpy_path": str(Path(np.__file__).resolve()),
        "python_executable": sys.executable,
        "comparison_scope": "installed-artifact-vs-installed-baselines",
        "provenance_verified": verified,
        "python": sys.version,
        "platform": platform.platform(),
        "numpy": np.__version__,
        "tessera": ts.__version__,
        "core": ts.__core_version__,
        "seed": SEED,
        "sizes": sizes,
        "threads": args.threads,
        "rounds": args.rounds,
        "warmup_rounds": 1,
    }
    if not verified:
        metadata["provenance_note"] = (
            f"Artifact source is unverified: commit={build_info['commit']}, "
            f"dirty={build_info['dirty']}. The benchmark checkout does not identify the wheel source."
        )
    print(
        f"Python {platform.python_version()}, NumPy {np.__version__}, Tessera {ts.__version__}; seed={SEED}."
    )
    print(
        f"Sizes: {sizes}; Tessera threads: {args.threads}; rounds: {args.rounds}; warmup: 1."
    )
    print(
        "NumPy is single-threaded. Times are median (IQR) ms; speedup = baseline / Tessera; >1x is faster."
    )
    print(
        "A/A rows time np.sort twice; the Tessera column holds the second NumPy measurement."
    )
    print(
        "Input resets are untimed; public-call allocation/conversion/writeback are timed; pools are warm."
    )
    print(
        "array baselines include sorted-to-array conversion and, for sort_inplace, slice writeback."
    )
    print(
        "Partitions use kth=n//2; NumPy ties are unstable. Python float cases omit NaNs.\n"
    )
    if not NUMPY_DESCENDING:
        print(
            "Descending sort comparisons skipped: this NumPy lacks descending= support.\n"
        )
    print("| Case | Baseline median (IQR) ms | Tessera median (IQR) ms | Speedup |")
    print("| :--- | ---: | ---: | ---: |", flush=True)
    results = []
    for size in sizes:
        for name, source, baseline, candidate, check, inplace in cases(
            size, args.threads, ts
        ):
            try:
                results.append(
                    benchmark(
                        name, source, baseline, candidate, check, args.rounds, inplace
                    )
                )
            except Exception as error:
                raise RuntimeError(f"Benchmark failed: {name}: {error}") from error
    if args.json:
        args.json.write_text(
            json.dumps({"metadata": metadata, "results": results}, indent=2) + "\n"
        )


if __name__ == "__main__":
    main()
