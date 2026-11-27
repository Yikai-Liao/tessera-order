# SPDX-License-Identifier: BSD-2-Clause

import math
import struct
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
            positions = [order.index(i) for i in result]
        else:
            assert sorted(map(id, result)) == sorted(map(id, a))
        if "partition" in name:
            for rank in ranks:
                if name.startswith("arg"):
                    assert positions[rank] == rank
                    assert all(x < rank for x in positions[:rank])
                    assert all(x > rank for x in positions[rank + 1 :])
                else:
                    assert result[rank] is expected[rank]
            # A stable sort of the partition must retain each equal-key group.
            if not name.startswith("arg"):
                reordered = sorted(result, key=key, reverse=descending)
                assert all(x is y for x, y in zip(reordered, expected))
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
    expected = sorted(a, key=lambda x: (math.isnan(x), -x if descending else x))
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
        for rank in kth if isinstance(kth, list) else [kth]:
            assert a[indices[rank]] is expected[rank]
            assert part[rank] is expected[rank]
            assert inplace[rank] is expected[rank]


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
            for rank in kth if isinstance(kth, list) else [kth]:
                assert result[rank] == (
                    order[rank] if name.startswith("arg") else a[order[rank]]
                )


@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_large_lists(threads):
    a = [(i * 1777) % 100003 for i in range(20_003)]
    expected = sorted(a)
    assert ts.sort(a, threads=threads) == expected
    assert [a[i] for i in ts.argsort(a, threads=threads)] == expected
    objects = [(x, i) for i, x in enumerate(a)]
    result = ts.sort(objects, key=lambda x: x[0], descending=True, threads=threads)
    assert result == sorted(objects, key=lambda x: x[0], reverse=True)
    for fn in (ts.partition, ts.argpartition):
        result = fn(a, [0, 10000, -1], threads=threads)
        if fn is ts.argpartition:
            result = [a[i] for i in result]
        assert [result[i] for i in (0, 10000, -1)] == [
            expected[i] for i in (0, 10000, -1)
        ]
