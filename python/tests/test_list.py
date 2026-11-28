# SPDX-License-Identifier: BSD-2-Clause

import math
import multiprocessing
import os
import struct
import sys
import threading
from decimal import Decimal
from fractions import Fraction

import pytest
import tessera_order as ts

NAMES = (
    "sort",
    "sort_inplace",
    "argsort",
    "partition",
    "partition_inplace",
    "argpartition",
)


def assert_list_partition(result, expected, ranks, key=None, descending=False):
    ranks = [ranks] if isinstance(ranks, int) else ranks
    ranks = sorted({rank % len(expected) for rank in ranks})
    assert len(result) == len(expected)
    for rank in ranks:
        assert result[rank] is expected[rank]
    for lo, hi in zip([-1, *ranks], [*ranks, len(expected)]):
        assert sorted(map(id, result[lo + 1 : hi])) == sorted(
            map(id, expected[lo + 1 : hi])
        )
    reordered = sorted(result, key=key, reverse=descending)
    assert all(x is y for x, y in zip(reordered, expected))


def assert_index_partition(indices, order, ranks):
    ranks = [ranks] if isinstance(ranks, int) else ranks
    ranks = sorted({rank % len(order) for rank in ranks})
    assert sorted(indices) == list(range(len(order)))
    for rank in ranks:
        assert indices[rank] == order[rank]
    for lo, hi in zip([-1, *ranks], [*ranks, len(order)]):
        assert sorted(indices[lo + 1 : hi]) == sorted(order[lo + 1 : hi])
    positions = {index: pos for pos, index in enumerate(order)}
    assert sorted(indices, key=positions.__getitem__) == order


def test_list_partition_assertion():
    source = [3, -1, 3, 0, 2]
    expected = sorted(source)
    invalid = expected.copy()
    invalid[1], invalid[3] = invalid[3], invalid[1]
    with pytest.raises(AssertionError):
        assert_list_partition(invalid, expected, [0, 2, 4])
    valid = expected.copy()
    valid[1], valid[2] = valid[2], valid[1]
    assert_list_partition(valid, expected, [0, 3, 4])
    # Duplicate object references must still check exact source indices for argpartition.
    order = [4, 1, 0, 2, 3]
    with pytest.raises(AssertionError):
        assert_index_partition([4, 1, 2, 0, 3], order, [0, 2, 4])
    assert_index_partition([1, 4, 0, 3, 2], order, [2])


def check_operations(a, key=None, descending=False, threads=1):
    values = [key(x) if key else x for x in a]
    order = sorted(range(len(a)), key=lambda i: values[i], reverse=descending)
    expected = [a[i] for i in order]
    ranks = sorted({0, len(a) // 2, len(a) - 1}) if a else []
    for name in NAMES:
        original = a.copy()
        calls = []

        def cached_key(x):
            calls.append(x)
            return key(x)

        kwargs = dict(descending=descending, threads=threads)
        if key:
            kwargs["key"] = cached_key
        args = (ranks,) if "partition" in name else ()
        result = getattr(ts, name)(original, *args, **kwargs)
        if key:
            assert len(calls) == len(a)
            assert all(x is y for x, y in zip(calls, a))
        if name.endswith("inplace"):
            assert result is None
            result = original
        else:
            assert all(x is y for x, y in zip(original, a))
        if name.startswith("arg"):
            assert sorted(result) == list(range(len(a)))
        else:
            assert sorted(map(id, result)) == sorted(map(id, a))
        if "partition" in name:
            if name.startswith("arg"):
                assert_index_partition(result, order, ranks)
            else:
                assert_list_partition(result, expected, ranks, key, descending)
        elif name.startswith("arg"):
            assert result == order
        else:
            assert all(x is y for x, y in zip(result, expected))


@pytest.mark.parametrize("threads", [1, None, 2, 4])
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize(
    "values",
    [
        [],
        [7],
        [3, -1, 3, 0, 2],
        [True, False, True],
        [0, 2**63, 1, 2**64 - 1],
        [-1, 2**64, -(2**127), 2**127 - 1],
        [2**127, 0, 2**128 - 1, 2**64],
        [2**128 - 1, 2**127, 0],
    ],
)
def test_numeric_operations(values, descending, threads):
    check_operations(values, descending=descending, threads=threads)


def test_subclasses_and_duplicate_references():
    class Int(int):
        pass

    class Float(float):
        pass

    a, b = Int(2), Int(1)
    check_operations([a, b, a, Int(2), False])
    check_operations([Float(2.0), Float(-1.0), Float(2.0)])
    objects = [{"score": 3}, {"score": 1}, {"score": 3}]
    check_operations([*objects, objects[0]], key=lambda x: x["score"])


@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_float_bits(descending, threads):
    bits = [0x7FF0000000000001, 0xFFF8000000000017, 0x7FF8000000000031]
    nans = [struct.unpack("d", struct.pack("Q", b))[0] for b in bits]
    a = [*nans, 3.0, -0.0, 0.0, -math.inf, -2.0, math.inf, -0.0, *nans]
    values = [ts.sort(a, descending=descending, threads=threads)]
    indices = ts.argsort(a, descending=descending, threads=threads)
    values.append([a[i] for i in indices])
    copy = a.copy()
    assert ts.sort_inplace(copy, descending=descending, threads=threads) is None
    values.append(copy)
    expected_order = sorted(
        range(len(a)),
        key=lambda i: (math.isnan(a[i]), -a[i] if descending else a[i]),
    )
    expected = [a[i] for i in expected_order]
    for result in values:
        assert all(x is y for x, y in zip(result, expected))
        assert b"".join(struct.pack("d", x) for x in result) == b"".join(
            struct.pack("d", x) for x in expected
        )
    for kth in (0, -1, [0, 3, -1]):
        indices = ts.argpartition(a, kth, descending=descending, threads=threads)
        part = ts.partition(a, kth, descending=descending, threads=threads)
        inplace = a.copy()
        ts.partition_inplace(inplace, kth, descending=descending, threads=threads)
        float_key = lambda x: (math.isnan(x), -x if descending else x)
        assert_index_partition(indices, expected_order, kth)
        for result in (part, inplace):
            assert_list_partition(result, expected, kth, float_key, False)


@pytest.mark.parametrize(
    "values,error",
    [
        ([2**200], OverflowError),
        ([-(2**200)], OverflowError),
        ([-1, 2**127], OverflowError),
        ([2**127, -1], OverflowError),
        ([1, 2.0], TypeError),
        ([1.0, 2], TypeError),
        ([Decimal(1)], TypeError),
        ([Fraction(1)], TypeError),
        (["1"], TypeError),
        ([1j], TypeError),
    ],
)
@pytest.mark.parametrize("name", NAMES)
def test_invalid_keys_leave_input_untouched(values, error, name):
    for key in (None, lambda x: x):
        a = values.copy()
        with pytest.raises(error):
            getattr(ts, name)(a, *([0] if "partition" in name else []), key=key)
        assert all(x is y for x, y in zip(a, values))


@pytest.mark.parametrize("name", NAMES)
@pytest.mark.parametrize("length", [0, 1, 7])
def test_exact_key_calls_and_validation(name, length):
    a = list(range(length))
    calls = []
    getattr(ts, name)(
        a, *([[]] if "partition" in name else []), key=lambda x: calls.append(x) or x
    )
    assert calls == list(range(length))
    if length:
        with pytest.raises(TypeError):
            getattr(ts, name)(
                a, *([[]] if "partition" in name else []), key=lambda x: "bad"
            )
    for axis in (-2, 1):
        with pytest.raises(ValueError):
            getattr(ts, name)(
                a,
                *([[]] if "partition" in name else []),
                axis=axis,
                key=lambda _: pytest.fail("key called before axis validation"),
            )
    if "partition" in name:
        for kth in (length, -length - 1):
            with pytest.raises(ValueError):
                getattr(ts, name)(
                    a,
                    kth,
                    key=lambda _: pytest.fail("key called before kth validation"),
                )
    for axis in (-1, 0, None):
        if axis is None and name.endswith("inplace"):
            with pytest.raises(TypeError):
                getattr(ts, name)(a, *([[]] if "partition" in name else []), axis=axis)
        else:
            getattr(ts, name)(a, *([[]] if "partition" in name else []), axis=axis)


@pytest.mark.parametrize("name", NAMES)
@pytest.mark.parametrize("mutation", ["append", "pop", "swap", "replace"])
def test_reentrant_mutation_preserved(name, mutation):
    a = [3, 1, 2]
    edited = []

    def key(x):
        if not edited:
            if mutation == "append":
                a.append(7)
            elif mutation == "pop":
                a.pop()
            elif mutation == "swap":
                a[0], a[1] = a[1], a[0]
            else:
                a[0] = 99
            edited.extend(a)
        return x

    with pytest.raises(ValueError, match="list modified during sort"):
        getattr(ts, name)(a, *([1] if "partition" in name else []), key=key)
    assert a == edited


@pytest.mark.parametrize("name", NAMES)
def test_key_exception_and_partition_ranks(name):
    a = [3, 1, 2, 1]
    before = a.copy()

    def broken(x):
        if x == 2:
            raise RuntimeError("key failed")
        return x

    with pytest.raises(RuntimeError, match="key failed"):
        getattr(ts, name)(a, *([0] if "partition" in name else []), key=broken)
    assert a == before
    if "partition" in name:
        for kth in (0, -1, [0, -1, 2, 2], []):
            original = a.copy()
            result = getattr(ts, name)(original, kth)
            if name.endswith("inplace"):
                result = original
            if not kth and isinstance(kth, list):
                assert result == (list(range(len(a))) if name.startswith("arg") else a)
                continue
            order = sorted(range(len(a)), key=a.__getitem__)
            if name.startswith("arg"):
                assert_index_partition(result, order, kth)
            else:
                assert_list_partition(result, [a[i] for i in order], kth)


@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_large_lists(threads):
    a = [(i * 1777) % 100003 for i in range(20_003)]
    expected = sorted(a)
    assert ts.sort(a, threads=threads) == expected
    assert [a[i] for i in ts.argsort(a, threads=threads)] == expected
    objects = [(x, i) for i, x in enumerate(a)]
    result = ts.sort(objects, key=lambda x: x[0], descending=True, threads=threads)
    assert result == sorted(objects, key=lambda x: x[0], reverse=True)
    expected_order = sorted(range(len(a)), key=a.__getitem__)
    for fn in (ts.partition, ts.partition_inplace, ts.argpartition):
        work = a.copy()
        result = fn(work, [0, 10000, -1], threads=threads)
        if fn is ts.partition_inplace:
            assert result is None
            assert_list_partition(work, expected, [0, 10000, -1])
        elif fn is ts.argpartition:
            assert_index_partition(result, expected_order, [0, 10000, -1])
        else:
            assert_list_partition(result, expected, [0, 10000, -1])


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
@pytest.mark.parametrize("threads", [1, 2])
def test_no_key_concurrent_replacement(name, threads):
    observed_rejection = False
    for attempt in range(5):
        size = 300_007 * (attempt + 1)
        a = [(i * 1777) % size for i in range(size)]
        replacement = 10**12 + 7 + attempt
        ready = threading.Event()
        start = threading.Lock()
        start.acquire()

        def replace():
            ready.set()
            with start:
                a[0] = replacement

        worker = threading.Thread(target=replace)
        worker.start()
        ready.wait(timeout=5)
        interval = sys.getswitchinterval()
        try:
            # Hold the GIL until the Rust call detaches so the waiting thread
            # replaces an element during the detached ordering window.
            sys.setswitchinterval(60)
            start.release()
            try:
                getattr(ts, name)(
                    a, *([len(a) // 2] if "partition" in name else []), threads=threads
                )
            except ValueError as err:
                assert "list modified during sort" in str(err)
                observed_rejection = True
        finally:
            sys.setswitchinterval(interval)
            worker.join(timeout=10)
        assert not worker.is_alive()
        if observed_rejection:
            assert a[0] is replacement
            break
        # Even if the edit landed strictly before or after the call, it must never be lost.
        assert any(item is replacement for item in a)
    assert observed_rejection


def _sort_after_fork(connection):
    try:
        from array import array

        # Exceed the core's 1 MiB parallel threshold so threads=None actually
        # dispatches work to Rayon rather than taking the serial fallback.
        n = 200_000
        a = array("q", ((i * 1777) % 100_003 for i in range(n)))
        expected = array("q", sorted(a))
        for threads in (2, None, 1):
            assert ts.sort(a, threads=threads) == expected
        connection.send(None)
    except BaseException as error:
        connection.send(repr(error))
    finally:
        connection.close()


@pytest.mark.skipif(os.name != "posix", reason="requires POSIX fork")
def test_parallel_pools_after_fork(monkeypatch):
    from array import array

    monkeypatch.setenv("RAYON_NUM_THREADS", "2")
    n = 200_000
    a = array("q", ((i * 1777) % 100_003 for i in range(n)))
    expected = array("q", sorted(a))
    for threads in (2, None):
        assert ts.sort(a, threads=threads) == expected
    context = multiprocessing.get_context("fork")

    def run_fork_check():
        parent, child = context.Pipe(duplex=False)
        worker = context.Process(target=_sort_after_fork, args=(child,))
        worker.start()
        child.close()
        try:
            assert parent.poll(20), "child sort hung after inheriting Rayon pools"
            assert parent.recv() is None
            worker.join(timeout=5)
            assert worker.exitcode == 0
        finally:
            if worker.is_alive():
                worker.terminate()
                worker.join(timeout=5)
            parent.close()

    run_fork_check()

    # Also fork while another parent thread repeatedly replaces the cached pool,
    # ensuring the child never locks an inherited parent mutex.
    stop = threading.Event()
    active = threading.Event()

    def churn_pools():
        small = [3, 1, 2, 0]
        width = 2
        while not stop.is_set():
            ts.sort(small, threads=width)
            active.set()
            width = 3 if width == 2 else 2

    churner = threading.Thread(target=churn_pools)
    churner.start()
    try:
        assert active.wait(timeout=5)
        for _ in range(3):
            run_fork_check()
    finally:
        stop.set()
        churner.join(timeout=10)
    assert not churner.is_alive()
