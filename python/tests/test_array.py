# SPDX-License-Identifier: BSD-2-Clause

import array
import importlib.util
import os
import subprocess
import sys

import pytest
import tessera_order as ts


def assert_array_partition(result, expected, ranks, key=None, descending=False):
    ranks = [ranks] if isinstance(ranks, int) else ranks
    ranks = sorted({rank % len(expected) for rank in ranks})
    assert result.typecode == expected.typecode
    assert len(result) == len(expected)
    for rank in ranks:
        assert result[rank] == expected[rank]
    for lo, hi in zip([-1, *ranks], [*ranks, len(expected)]):
        assert sorted(result[lo + 1 : hi], key=key, reverse=descending) == list(
            expected[lo + 1 : hi]
        )
    assert sorted(result, key=key, reverse=descending) == list(expected)


def assert_index_partition(indices, expected_indices, ranks):
    ranks = [ranks] if isinstance(ranks, int) else ranks
    ranks = sorted({rank % len(expected_indices) for rank in ranks})
    assert sorted(indices) == list(range(len(expected_indices)))
    for rank in ranks:
        assert indices[rank] == expected_indices[rank]
    for lo, hi in zip([-1, *ranks], [*ranks, len(expected_indices)]):
        assert sorted(indices[lo + 1 : hi]) == sorted(expected_indices[lo + 1 : hi])


def test_array_partition_assertion():
    expected = array.array("i", [0, 1, 1, 2, 3, 3])
    bad = array.array("i", [0, 3, 1, 1, 2, 3])
    with pytest.raises(AssertionError):
        assert_array_partition(bad, expected, [0, 2, 5])
    valid = array.array("i", [0, 1, 1, 3, 2, 3])
    assert_array_partition(valid, expected, [0, 2, 5])


@pytest.mark.parametrize("typecode", "bBhHiIlLqQfd")
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("threads", [1, None, 2])
def test_array_operations(typecode, descending, threads):
    a = array.array(typecode, [3, 1, 3, 0, 2, 1])
    before = a.tobytes()
    expected_indices = sorted(range(len(a)), key=a.__getitem__, reverse=descending)
    expected = array.array(typecode, (a[i] for i in expected_indices))
    opts = dict(descending=descending, threads=threads)
    for axis in (-1, 0, None):
        assert ts.sort(a, axis, **opts) == expected
        assert ts.argsort(a, axis, **opts) == expected_indices
        indices = ts.argpartition(a, [0, -1, 2, 2], axis, **opts)
        values = ts.partition(a, [0, -1, 2, 2], axis, **opts)
        assert_index_partition(indices, expected_indices, [0, -1, 2, 2])
        assert_array_partition(values, expected, [0, -1, 2, 2], descending=descending)
    assert a.tobytes() == before
    for axis in (-1, 0):
        copy = a.__copy__()
        assert ts.sort_inplace(copy, axis, **opts) is None
        assert copy == expected
        copy = a.__copy__()
        assert ts.partition_inplace(copy, [0, -1, 2, 2], axis, **opts) is None
        assert_array_partition(copy, expected, [0, -1, 2, 2], descending=descending)
    assert ts.partition(a, [], **opts) == a
    assert ts.argpartition(a, [], **opts) == list(range(len(a)))


def test_empty_and_argument_errors():
    empty = array.array("d")
    assert ts.sort(empty) == empty
    assert ts.argsort(empty) == []
    assert ts.sort_inplace(empty) is None
    for name in ("partition", "partition_inplace", "argpartition"):
        fn = getattr(ts, name)
        with pytest.raises(ValueError):
            fn(empty, 0)
        result = fn(empty, [])
        if name.endswith("inplace"):
            assert result is None
        else:
            assert len(result) == 0
    a = array.array("i", [2, 1])
    for name in (
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ):
        fn = getattr(ts, name)
        args = (a, 0) if "partition" in name else (a,)
        for axis in (-2, 1, 2**200, -(2**200)):
            with pytest.raises(ValueError):
                fn(*args, axis=axis)
        for threads, error in (
            (True, TypeError),
            (1.0, TypeError),
            ("2", TypeError),
            (0, ValueError),
            (-(2**200), ValueError),
            (2**200, OverflowError),
        ):
            with pytest.raises(error):
                fn(*args, threads=threads)
        fn(*args, key=abs)
        if name.endswith("inplace"):
            with pytest.raises(TypeError):
                fn(*args, axis=None)
    assert ts.sort([2, 1]) == [1, 2]
    for unsupported in ((2, 1), memoryview(a), "21", {2, 1}):
        with pytest.raises(TypeError):
            ts.sort(unsupported)
    for code in ("u", "w"):
        if code not in array.typecodes:
            continue
        unsupported = array.array(code, "ba")
        with pytest.raises(TypeError):
            ts.sort(unsupported)


def test_index_threads_and_copy_alias():
    class Index:
        def __index__(self):
            return 2

    assert ts.sort(array.array("i", [2, 1]), threads=Index()).tolist() == [1, 2]

    class Alias(array.array):
        def __copy__(self):
            return self

    with pytest.raises(ValueError, match="aliases"):
        ts.sort(Alias("i", [2, 1]))


def test_numpy_free_import():
    # Fail even an attempted NumPy import, in an interpreter with a fresh cache.
    script = """
import sys
class BlockNumpy:
    def find_spec(self, fullname, *args):
        if fullname.split('.')[0] == 'numpy':
            raise AssertionError('NumPy import attempted')
sys.meta_path.insert(0, BlockNumpy())
import tessera_order as ts
from array import array
assert 'numpy' not in sys.modules
for a in ([3, 1, 2], array('i', [3, 1, 2])):
    for key in (None, lambda x: x):
        for name in ('sort', 'sort_inplace', 'argsort', 'partition', 'partition_inplace', 'argpartition'):
            data = a[:]
            result = getattr(ts, name)(data, *([1] if 'partition' in name else []), key=key)
            assert 'numpy' not in sys.modules
            if name.endswith('inplace'):
                assert result is None
                result = data
            assert list(result) == ([1, 2, 0] if name.startswith('arg') else [1, 2, 3])
try:
    ts.sort((3, 1, 2))
except TypeError:
    pass
else:
    raise AssertionError('unsupported container accepted')
assert 'numpy' not in sys.modules
"""
    env = os.environ.copy()
    spec = importlib.util.find_spec("tessera_order")
    env["PYTHONPATH"] = os.path.dirname(os.path.dirname(spec.origin))
    subprocess.run([sys.executable, "-c", script], env=env, check=True)


@pytest.mark.parametrize(
    "typecode,values,key",
    [
        ("i", [3, -1, -3, 2, 0, -1], abs),
        ("d", [3.5, -1.5, -3.5, 2.5, 0.0], abs),
        ("u", "cabac", ord),
        ("w", "cabac", ord),
    ],
)
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_key_operations(typecode, values, key, descending, threads):
    if typecode not in array.typecodes:
        pytest.skip("typecode unavailable on this Python")
    a = array.array(typecode, values)
    expected_indices = sorted(
        range(len(a)), key=lambda i: key(a[i]), reverse=descending
    )
    expected = array.array(typecode, (a[i] for i in expected_indices))
    before = a.tobytes()
    for name in (
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ):
        for axis in (-1, 0, None):
            if axis is None and name.endswith("inplace"):
                continue
            copy = a[:]
            calls = []

            def counted(x):
                calls.append(x)
                return key(x)

            result = getattr(ts, name)(
                copy,
                *([[0, 2, -1]] if "partition" in name else []),
                axis=axis,
                key=counted,
                descending=descending,
                threads=threads,
            )
            assert calls == list(a)
            if name.endswith("inplace"):
                assert result is None
                result = copy
            else:
                assert copy.tobytes() == before
            if name.startswith("arg"):
                if "partition" in name:
                    assert_index_partition(result, expected_indices, [0, 2, -1])
                else:
                    assert result == expected_indices
                continue
            assert result.typecode == a.typecode
            if "partition" in name:
                assert_array_partition(
                    result, expected, [0, 2, -1], key=key, descending=descending
                )
            else:
                assert result == expected


@pytest.mark.parametrize(
    "name",
    [
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ],
)
@pytest.mark.parametrize("mutation", ["append", "pop", "replace"])
def test_key_mutation(name, mutation):
    a = array.array("i", [3, 1, 2])
    edited = []

    def key(x):
        if not edited:
            if mutation == "append":
                a.append(4)
            elif mutation == "pop":
                a.pop()
            else:
                a[1] = 9
            edited.extend(a)
        return x

    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *([1] if "partition" in name else []), key=key)
    assert list(a) == edited


@pytest.mark.parametrize(
    "name",
    [
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ],
)
def test_key_validation(name):
    for length in (0, 1):
        a = array.array("i", range(length))
        calls = []
        getattr(ts, name)(
            a,
            *([[]] if "partition" in name else []),
            key=lambda x: calls.append(x) or x,
        )
        assert len(calls) == length
    a = array.array("i", [3, 1, 2])
    before = a.tobytes()
    for error, key in ((TypeError, lambda x: "bad"), (OverflowError, lambda x: 2**200)):
        with pytest.raises(error):
            getattr(ts, name)(a, *([0] if "partition" in name else []), key=key)
    calls = []

    def broken(x):
        calls.append(x)
        if len(calls) == 2:
            raise RuntimeError("key failed")
        return x

    with pytest.raises(RuntimeError, match="key failed"):
        getattr(ts, name)(a, *([0] if "partition" in name else []), key=broken)
    assert a.tobytes() == before
    for axis in (1, -2):
        with pytest.raises(ValueError):
            getattr(ts, name)(
                a,
                *([0] if "partition" in name else []),
                axis=axis,
                key=lambda _: pytest.fail("key before validation"),
            )
    if "partition" in name:
        with pytest.raises(ValueError):
            getattr(ts, name)(
                a, len(a), key=lambda _: pytest.fail("key before validation")
            )


def test_copy_validation_precedes_allocation():
    class CopyTrap(array.array):
        def __copy__(self):
            pytest.fail("copy before validation")

    for kwargs in ({"axis": 1}, {}):
        with pytest.raises((TypeError, ValueError)):
            ts.sort(CopyTrap("u", "abc"), **kwargs)
    with pytest.raises(ValueError):
        ts.partition(CopyTrap("i", [3, 1, 2]), 3)


@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_large_key_arrays(threads):
    a = array.array("i", ((i * 1777) % 100003 for i in range(20_003)))
    order = sorted(range(len(a)), key=lambda i: -a[i])
    assert ts.argsort(a, key=lambda x: -x, threads=threads) == order
    assert list(ts.sort(a, key=lambda x: -x, threads=threads)) == [a[i] for i in order]
    b = a[:]
    ts.sort_inplace(b, key=lambda x: -x, threads=threads)
    assert list(b) == [a[i] for i in order]


@pytest.mark.parametrize(
    "name",
    [
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ],
)
def test_key_element_mutation_before_error(name):
    a = array.array("i", [3, 1, 2])

    def key(x):
        a[1] = 9
        raise RuntimeError("callback failed after edit")

    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *([1] if "partition" in name else []), key=key)
    assert list(a) == [3, 9, 2]
