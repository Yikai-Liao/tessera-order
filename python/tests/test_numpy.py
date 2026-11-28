# SPDX-License-Identifier: BSD-2-Clause

from array import array
import sys

import pytest
import tessera_order as ts

np = pytest.importorskip("numpy")


def bits(a):
    return a.tobytes(order="C")


def reference_argsort(a, axis=-1, descending=False):
    """Stable numeric order with NaNs last in either direction, on NumPy 1.x+."""
    data = a.ravel(order="C") if axis is None else a
    ax = -1 if axis is None else axis
    lanes = np.moveaxis(data, ax, -1)
    result = np.empty(data.shape, dtype=np.intp)
    outputs = np.moveaxis(result, ax, -1)
    for position in np.ndindex(lanes.shape[:-1]):
        lane = lanes[position]

        def key(i):
            value = lane[i].item()
            nan = isinstance(value, float) and value != value
            return (nan, 0 if nan else -value if descending else value)

        outputs[position] = sorted(range(len(lane)), key=key)
    return result


def assert_numpy_partition(part, expected, kth, axis, descending):
    length = expected.shape[axis]
    part_order = reference_argsort(part, axis=axis, descending=descending)
    assert bits(np.take_along_axis(part, part_order, axis=axis)) == bits(expected)
    part_positions = np.argsort(part_order, axis=axis)
    for rank in kth:
        rank %= length
        assert bits(np.take(part, rank, axis=axis)) == bits(
            np.take(expected, rank, axis=axis)
        )
        assert np.all(np.take(part_positions, range(rank), axis=axis) < rank)
        assert np.all(
            np.take(part_positions, range(rank + 1, length), axis=axis) > rank
        )


def assert_numpy_argpartition(
    argpart, lane, expected_indices, kth, axis, descending, key=None
):
    length = expected_indices.shape[axis]
    identity = np.moveaxis(
        np.broadcast_to(np.arange(length), np.moveaxis(lane, axis, -1).shape), -1, axis
    )
    np.testing.assert_array_equal(np.sort(argpart, axis=axis), identity)
    positions = np.take_along_axis(
        np.argsort(expected_indices, axis=axis), argpart, axis=axis
    )
    for rank in kth:
        rank %= length
        np.testing.assert_array_equal(
            np.take(argpart, rank, axis=axis),
            np.take(expected_indices, rank, axis=axis),
        )
        assert np.all(np.take(positions, range(rank), axis=axis) < rank)
        assert np.all(np.take(positions, range(rank + 1, length), axis=axis) > rank)
    if key is None:
        gathered = np.take_along_axis(lane, argpart, axis=axis)
        tie_order = reference_argsort(gathered, axis=axis, descending=descending)
        np.testing.assert_array_equal(
            np.take_along_axis(argpart, tie_order, axis=axis), expected_indices
        )
    else:
        src_lanes = np.moveaxis(lane, axis, -1)
        res_lanes = np.moveaxis(argpart, axis, -1)
        exp_lanes = np.moveaxis(expected_indices, axis, -1)
        for pos in np.ndindex(src_lanes.shape[:-1]):
            lane_keys = [key(x) for x in src_lanes[pos]]
            reordered = sorted(
                res_lanes[pos].tolist(), key=lane_keys.__getitem__, reverse=descending
            )
            assert reordered == exp_lanes[pos].tolist()


def test_numpy_partition_assertion():
    expected = np.array([0, 1, 2, 3, 4], dtype="i4")
    bad = np.array([0, 3, 2, 1, 4], dtype="i4")
    with pytest.raises(AssertionError):
        assert_numpy_partition(bad, expected, [0, 2, 4], -1, False)
    valid = np.array([0, 2, 1, 3, 4], dtype="i4")
    assert_numpy_partition(valid, expected, [0, 3, 4], -1, False)
    # Keyed and numeric argpartition must reject equal-key indices reversed inside one region.
    scores = np.array([3, 1, 3, 0, 2, 1], dtype="i4")
    order = np.array([3, 1, 5, 4, 0, 2], dtype=np.intp)
    swapped_ties = np.array([3, 5, 1, 4, 0, 2], dtype=np.intp)
    with pytest.raises(AssertionError):
        assert_numpy_argpartition(swapped_ties, scores, order, [0, 3, 5], -1, False)
    with pytest.raises(AssertionError):
        assert_numpy_argpartition(
            swapped_ties, scores, order, [0, 3, 5], -1, False, key=int
        )
    swapped_distinct = np.array([3, 4, 1, 5, 0, 2], dtype=np.intp)
    assert_numpy_argpartition(swapped_distinct, scores, order, [0, 5], -1, False)
    assert_numpy_argpartition(
        swapped_distinct, scores, order, [0, 5], -1, False, key=int
    )


def check_operations(a, axis, descending, threads):
    opts = dict(axis=axis, descending=descending, threads=threads)
    lane = a.ravel(order="C") if axis is None else a
    ax = -1 if axis is None else axis
    expected_indices = reference_argsort(a, axis=axis, descending=descending)
    expected = np.take_along_axis(lane, expected_indices, axis=ax)
    original = bits(a)
    sorted_copy = ts.sort(a, **opts)
    indices = ts.argsort(a, **opts)
    assert sorted_copy.dtype == a.dtype
    assert sorted_copy.shape == expected.shape
    assert bits(sorted_copy) == bits(expected)
    assert indices.dtype == np.dtype(np.intp)
    np.testing.assert_array_equal(indices, expected_indices)
    length = lane.shape[ax]
    ranks = sorted({0, length // 2, length - 1}) if length else []
    for kth in ([r - length for r in ranks], ranks[:1], []):
        part = ts.partition(a, kth, **opts)
        argpart = ts.argpartition(a, kth, **opts)
        assert part.dtype == a.dtype
        assert argpart.dtype == np.dtype(np.intp)
        assert_numpy_argpartition(argpart, lane, expected_indices, kth, ax, descending)
        # Check stable ties, partition regions, and original bits without fixing region order.
        assert_numpy_partition(part, expected, kth, ax, descending)
        if kth and not descending:
            # NumPy 2.5 has descending sort/argsort, but no descending partition.
            reference = np.partition(a, kth, axis=axis)
            reference_indices = np.argpartition(a, kth, axis=axis)
            reference_values = np.take_along_axis(lane, reference_indices, axis=ax)
            for rank in kth:
                np.testing.assert_array_equal(
                    np.take(part, rank, axis=ax),
                    np.take(reference_values, rank, axis=ax),
                )
                np.testing.assert_array_equal(
                    np.take(part, rank, axis=ax), np.take(reference, rank, axis=ax)
                )
        elif not kth:
            assert bits(part) == bits(lane)
    assert bits(a) == original
    if axis is not None:
        for name in ("sort_inplace", "partition_inplace"):
            # Keep the layout, alignment, byte order and underlying writeability.
            if not a.flags.writeable:
                with pytest.raises(ValueError, match="read-only"):
                    getattr(ts, name)(
                        a, *([ranks] if "partition" in name else []), **opts
                    )
                continue
            backup = a.copy()
            args = [ranks] if "partition" in name else []
            assert getattr(ts, name)(a, *args, **opts) is None
            if args:
                assert_numpy_partition(a, expected, ranks, ax, descending)
            else:
                assert bits(a) == bits(expected)
            a[...] = backup


@pytest.mark.parametrize(
    "dtype", ["i1", "i2", "i4", "i8", "u1", "u2", "u4", "u8", "f4", "f8"]
)
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("threads", [1, None, 2])
def test_nd_layouts(dtype, descending, threads):
    base = (((np.arange(120) * 17) % 29) - 14).astype(dtype).reshape(4, 5, 6)
    if np.dtype(dtype).kind in "iu":
        base.flat[0] = np.iinfo(dtype).min
        base.flat[1] = np.iinfo(dtype).max
    unaligned = np.ndarray(
        base.shape, dtype=dtype, buffer=bytearray(base.nbytes + 1), offset=1
    )
    unaligned[...] = base
    readonly = base.copy()
    readonly.flags.writeable = False
    cases = [
        base.copy(),
        base.transpose(2, 0, 1),
        base[::-1, ::2, ::-1],
        np.asfortranarray(base),
        base.astype(np.dtype(dtype).newbyteorder("S")),
        unaligned,
        readonly,
        np.broadcast_to(base[:1, :1, :], (3, 2, 6)),
    ]
    for a in cases:
        for axis in (None, 0, 1, -1):
            check_operations(a, axis, descending, threads)


@pytest.mark.parametrize("dtype,uint", [("f4", "u4"), ("f8", "u8")])
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("threads", [1, None, 2])
def test_float_bits(dtype, uint, descending, threads):
    floating = np.dtype(dtype)
    exponent = np.array([np.inf], dtype=dtype).view(uint)[0].item()
    sign = 1 << (floating.itemsize * 8 - 1)
    # Include signed signaling/quiet NaN payloads, stable ties, both zero signs.
    nan_bits = [exponent | 1, exponent | 31, sign | exponent | 17]
    special = np.array(nan_bits, dtype=uint).view(dtype)
    for zeros in ([0.0, 0.0], [-0.0, -0.0], [-0.0, 0.0]):
        values = np.array([3.0, -np.inf, *zeros, 2.0, np.inf, -1.0, 2.0], dtype=dtype)
        for a in (values, np.concatenate([special, values, special[::-1]])):
            for view in (a.copy(), a[::-1], a.astype(floating.newbyteorder("S"))):
                check_operations(view, -1, descending, threads)


def test_empty_scalar_and_validation():
    for shape in ((0,), (0, 3), (2, 0, 4), (2, 3, 0)):
        a = np.empty(shape)
        for axis in (None, *range(a.ndim)):
            check_operations(a, axis, False, 2)
            for name in ("partition", "partition_inplace", "argpartition"):
                if axis is None and name.endswith("inplace"):
                    continue
                if axis is None or a.shape[axis] == 0:
                    with pytest.raises(ValueError):
                        getattr(ts, name)(a, 0, axis=axis)
                else:
                    getattr(ts, name)(a, 0, axis=axis)
    scalar = np.array(7.0)
    check_operations(scalar, None, False, 2)
    for name in (
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ):
        args = [0] if "partition" in name else []
        for axis in (-1, 0, 1, -(2**200)):
            with pytest.raises(ValueError):
                getattr(ts, name)(scalar, *args, axis=axis)
        if not name.endswith("inplace"):
            getattr(ts, name)(scalar, *args, axis=None, key=abs)
        a = np.empty((0, 3))
        a.flags.writeable = False
        if name.endswith("inplace"):
            with pytest.raises(ValueError, match="read-only"):
                getattr(ts, name)(a, *([[]] if args else []))
    for dtype in (
        "?",
        "f2",
        "c8",
        "c16",
        "U2",
        "S2",
        "O",
        "M8[ns]",
        "m8[ns]",
        [("x", "i4")],
        np.longdouble,
    ):
        a = np.empty(0, dtype=dtype)
        for name in (
            "sort",
            "sort_inplace",
            "argsort",
            "partition",
            "partition_inplace",
            "argpartition",
        ):
            with pytest.raises(TypeError):
                getattr(ts, name)(a, *([[]] if "partition" in name else []))
    assert ts.sort(np.array([2, 1]), threads=np.int64(2)).tolist() == [1, 2]


def test_overlap_and_output_alias(monkeypatch):
    base = np.arange(16, dtype="i4")[::-1].copy()
    views = [
        np.lib.stride_tricks.as_strided(base, shape=(4, 4), strides=(4, 4)),
        np.lib.stride_tricks.as_strided(base, shape=(3, 2), strides=(0, 4)),
        np.ndarray((3,), dtype="i4", buffer=bytearray(8), strides=(2,)),
    ]
    for view in views:
        for name in ("sort_inplace", "partition_inplace"):
            with pytest.raises(ValueError, match="overlapping"):
                getattr(ts, name)(view, *([0] if "partition" in name else []))
        for axis in (None, -1):
            check_operations(view, axis, False, 1) if not view.flags.writeable else None
            np.testing.assert_array_equal(
                ts.sort(view, axis=axis), np.sort(view, axis=axis, kind="stable")
            )
    a = np.arange(4, dtype=np.intp)
    monkeypatch.setattr(np, "empty", lambda *args, **kwargs: a)
    with pytest.raises(ValueError, match="aliases"):
        ts.argsort(a)


@pytest.mark.parametrize(
    "shape,axis", [((1, 20_000), 1), ((2, 20_000), 1), ((24, 800), 1), ((40_000,), -1)]
)
def test_parallel_large_lanes(shape, axis):
    rng = np.random.default_rng(7)
    a = rng.integers(-1000, 1000, size=shape, dtype=np.int64)
    check_operations(a, axis, True, 4)
    check_operations(a[..., ::-1], axis, False, 4)


def check_key_operations(a, key, axis, descending=False, threads=1):
    source = a.ravel(order="C") if axis is None else a
    ax = -1 if axis is None else axis
    lanes = np.moveaxis(source, ax, -1)
    expected_indices = np.empty(source.shape, dtype=np.intp)
    expected = np.empty(source.shape, dtype=a.dtype)
    index_lanes = np.moveaxis(expected_indices, ax, -1)
    output_lanes = np.moveaxis(expected, ax, -1)
    length = lanes.shape[-1]
    for position in np.ndindex(lanes.shape[:-1]):
        lane = lanes[position]
        order = sorted(range(length), key=lambda i: key(lane[i]), reverse=descending)
        index_lanes[position] = order
        output_lanes[position] = lane[order]
    ranks = sorted({0, length // 2, length - 1}) if length else []
    for name in (
        "sort",
        "sort_inplace",
        "argsort",
        "partition",
        "partition_inplace",
        "argpartition",
    ):
        if axis is None and name.endswith("inplace"):
            continue
        data = a.copy()
        calls = []

        def counted(x):
            calls.append(1)
            return key(x)

        result = getattr(ts, name)(
            data,
            *([ranks] if "partition" in name else []),
            axis=axis,
            key=counted,
            descending=descending,
            threads=threads,
        )
        assert len(calls) == a.size
        if name.endswith("inplace"):
            assert result is None
            result = data
        else:
            np.testing.assert_array_equal(data, a)
        assert result.shape == source.shape
        assert result.dtype == (
            np.dtype(np.intp) if name.startswith("arg") else a.dtype
        )
        if "partition" in name:
            if name.startswith("arg"):
                assert_numpy_argpartition(
                    result, source, expected_indices, ranks, ax, descending, key=key
                )
            else:
                res_lanes = np.moveaxis(result, ax, -1)
                exp_lanes = np.moveaxis(expected, ax, -1)
                for pos in np.ndindex(res_lanes.shape[:-1]):
                    res_lane = res_lanes[pos]
                    exp_lane = exp_lanes[pos]
                    lane_order = sorted(
                        range(length),
                        key=lambda i: key(res_lane[i]),
                        reverse=descending,
                    )
                    np.testing.assert_array_equal(res_lane[lane_order], exp_lane)
                    inv = [0] * length
                    for final_pos, src_pos in enumerate(lane_order):
                        inv[src_pos] = final_pos
                    for rank in ranks:
                        np.testing.assert_array_equal(res_lane[rank], exp_lane[rank])
                        assert all(inv[i] < rank for i in range(rank))
                        assert all(inv[i] > rank for i in range(rank + 1, length))
            if not ranks:
                if name.startswith("arg"):
                    assert result.size == a.size
                else:
                    np.testing.assert_array_equal(result, source)
        else:
            np.testing.assert_array_equal(
                result, expected_indices if name.startswith("arg") else expected
            )


@pytest.mark.parametrize("threads", [1, None, 2, 4])
@pytest.mark.parametrize("descending", [False, True])
@pytest.mark.parametrize("shape", [(6,), (2, 3), (2, 1, 3)])
def test_key_any_dtype(shape, descending, threads):
    values = np.array([3, 1, 3, 0, 2, 1]).reshape(shape)
    structured = np.empty(
        shape, dtype=[("score", ">i8"), ("label", "U4"), ("tie", "i4")]
    )
    structured["score"] = values
    structured["label"] = values.astype("U4")
    structured["tie"] = np.arange(6).reshape(shape)
    objects = np.empty(shape, dtype=object)
    objects.flat[:] = [{"score": int(x)} for x in values.flat]
    cases = [
        (structured, lambda r: r["score"]),
        (objects, lambda r: r["score"]),
        (values.astype("U4"), len),
        (values.astype("S4"), lambda s: int(s)),
        (values.astype(bool), lambda b: b),
        ((values + 1j * values).astype("c16"), abs),
        (values.astype("datetime64[D]"), lambda d: d.astype("i8")),
        (values.astype("timedelta64[D]"), lambda d: d.astype("i8")),
        (values.astype(np.longdouble), float),
    ]
    if hasattr(getattr(np, "dtypes", None), "StringDType"):
        cases.append((values.astype(np.dtypes.StringDType()), lambda s: int(s)))
    for a, key in cases:
        for axis in (None, *range(a.ndim), -1):
            check_key_operations(a, key, axis, descending, threads)


@pytest.mark.parametrize("shape", [(), (1,), (0,), (2, 0, 3), (0, 3)])
@pytest.mark.parametrize(
    "dtype,key",
    [
        ("datetime64[ns]", lambda x: x.astype("i8")),
        ("O", int),
        ("U2", len),
        ("?", lambda x: x),
        ("c16", abs),
        ([("score", "i8")], lambda r: r["score"]),
    ],
)
def test_key_scalar_and_empty(shape, dtype, key):
    a = np.zeros(shape, dtype=dtype)
    for axis in (None, *range(a.ndim)):
        check_key_operations(a, key, axis, threads=2)
    if not shape:
        for name in (
            "sort",
            "sort_inplace",
            "argsort",
            "partition",
            "partition_inplace",
            "argpartition",
        ):
            with pytest.raises(ValueError):
                getattr(ts, name)(
                    a,
                    *([0] if "partition" in name else []),
                    key=lambda _: pytest.fail("key before axis validation"),
                )
    else:
        for axis, length in enumerate(shape):
            if length == 0:
                for name in ("partition", "partition_inplace", "argpartition"):
                    with pytest.raises(ValueError):
                        getattr(ts, name)(
                            a,
                            0,
                            axis=axis,
                            key=lambda _: pytest.fail("key before kth validation"),
                        )


@pytest.mark.parametrize("threads", [1, None, 2, 4])
def test_key_strided_layouts(threads):
    values = ((np.arange(120) * 17) % 29).reshape(4, 5, 6)
    structured = np.empty(values.shape, dtype=[("score", "i4"), ("tag", "i4")])
    structured["score"] = values
    structured["tag"] = np.arange(120).reshape(values.shape)
    for a in (
        structured.transpose(2, 0, 1),
        structured[::-1, ::2, ::-1],
        np.asfortranarray(structured),
    ):
        for axis in (None, 0, 1, -1):
            check_key_operations(a, lambda r: r["score"], axis, threads=threads)
        inplace = a.copy()[..., ::-1]
        ts.sort_inplace(inplace, key=lambda r: r["score"], threads=threads)
        np.testing.assert_array_equal(
            inplace["score"], np.sort(a[..., ::-1]["score"], axis=-1, kind="stable")
        )
    unaligned = np.ndarray(
        values.shape,
        dtype=structured.dtype,
        buffer=bytearray(structured.nbytes + 1),
        offset=1,
    )
    unaligned[...] = structured
    check_key_operations(unaligned, lambda r: r["score"], -1, threads=threads)
    readonly = structured.copy()
    readonly.flags.writeable = False
    for axis in (None, -1):
        result = ts.sort(readonly, axis=axis, key=lambda r: r["score"], threads=threads)
        expected = ts.sort(
            structured, axis=axis, key=lambda r: r["score"], threads=threads
        )
        np.testing.assert_array_equal(result, expected)


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
def test_key_writeability_overlap_before_callbacks(name):
    base = np.arange(12)
    readonly = base.copy()
    readonly.flags.writeable = False
    broadcast = np.broadcast_to(base[:1], (6,))
    overlapping = np.lib.stride_tricks.as_strided(base, shape=(3, 3), strides=(8, 8))
    zero_stride = np.lib.stride_tricks.as_strided(base, shape=(4,), strides=(0,))
    partial = np.ndarray((3,), dtype="i4", buffer=bytearray(8), strides=(2,))
    for a in (readonly, broadcast, overlapping, zero_stride, partial):
        with pytest.raises(ValueError, match="read-only|overlapping"):
            getattr(ts, name)(
                a,
                *([0] if "partition" in name else []),
                key=lambda _: pytest.fail("key before layout validation"),
            )
        np.testing.assert_array_equal(
            ts.sort(a, key=int), np.sort(a, axis=-1, kind="stable")
        )
        np.testing.assert_array_equal(
            ts.sort(a, axis=None, key=int), np.sort(a, axis=None, kind="stable")
        )


def test_object_identity_and_refcounts():
    import gc
    import sys

    class Record:
        def __init__(self, score):
            self.score = score

    first, second, third = Record(3), Record(1), Record(3)
    a = np.array([first, second, first, third], dtype=object)
    expected = [second, first, first, third]
    gc.collect()
    before = [sys.getrefcount(x) for x in (first, second, third)]
    for _ in range(5):
        for threads in (1, None, 2, 4):
            result = ts.sort(a, key=lambda x: x.score, threads=threads)
            assert all(result[i] is expected[i] for i in range(4))
            copy = a.copy()
            assert ts.sort_inplace(copy, key=lambda x: x.score, threads=threads) is None
            assert all(copy[i] is expected[i] for i in range(4))
            indices = ts.argsort(a, key=lambda x: x.score, threads=threads)
            assert indices.tolist() == [1, 0, 2, 3]
            del result, copy, indices
            check_key_operations(a, lambda x: x.score, -1, threads=threads)
    gc.collect()
    assert [sys.getrefcount(x) for x in (first, second, third)] == before
    # Structured scalars are views: retaining a scalar alone would alias writes.
    structured = np.empty((2, 4), dtype=[("obj", object), ("score", "i8")])
    structured["obj"] = a
    structured["score"] = [3, 1, 3, 3]
    check_key_operations(structured, lambda r: r["score"], -1, threads=4)
    assert structured["obj"][0, 0] is first


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
def test_key_failure_and_reentrant_metadata_mutation(name):
    args = [1] if "partition" in name else []
    a = np.array([3, 1, 2])
    calls = []

    def broken(x):
        calls.append(x)
        if len(calls) == 2:
            raise RuntimeError("key failed")
        return x

    with pytest.raises(RuntimeError, match="key failed"):
        getattr(ts, name)(a, *args, key=broken)
    assert a.tolist() == [3, 1, 2]
    for mutation in ("shape", "dtype", "strides"):
        a = np.arange(6, dtype="i8")
        edited = []

        def key(x):
            if not edited:
                edited.append(True)
                if mutation == "shape":
                    a.shape = (2, 3)
                elif mutation == "dtype":
                    a.dtype = "f8"
                else:
                    with pytest.warns(DeprecationWarning):
                        a.strides = (0,)
            return int(x)

        with pytest.raises(ValueError, match="array modified during sort"):
            getattr(ts, name)(a, *args, key=key)
        if mutation == "shape":
            assert a.shape == (2, 3)
        elif mutation == "dtype":
            assert a.dtype == np.dtype("f8")
        else:
            assert a.strides == (0,)
    if name.endswith("inplace"):
        a = np.array([3, 1, 2])

        def readonly_key(x):
            a.flags.writeable = False
            return x

        with pytest.raises(ValueError, match="read-only"):
            getattr(ts, name)(a, *args, key=readonly_key)
        assert a.tolist() == [3, 1, 2]


@pytest.mark.parametrize("threads", [1, None, 2, 4])
@pytest.mark.parametrize("shape", [(20_003,), (2, 20_003), (24, 1000)])
def test_large_key_lanes(shape, threads):
    values = ((np.arange(np.prod(shape)) * 1777) % 100003).reshape(shape)
    records = np.empty(shape, dtype=[("score", "i8"), ("index", "i8")])
    records["score"] = values
    records["index"] = np.arange(values.size).reshape(shape)
    order = reference_argsort(values, axis=-1, descending=True)
    expected = np.take_along_axis(records, order, axis=-1)
    np.testing.assert_array_equal(
        ts.sort(records, key=lambda r: r["score"], descending=True, threads=threads),
        expected,
    )
    np.testing.assert_array_equal(
        ts.argsort(records, key=lambda r: r["score"], descending=True, threads=threads),
        order,
    )
    part = ts.partition(
        records,
        [0, shape[-1] // 2, -1],
        key=lambda r: r["score"],
        descending=True,
        threads=threads,
    )
    for rank in (0, shape[-1] // 2, -1):
        np.testing.assert_array_equal(part[..., rank], expected[..., rank])


@pytest.mark.parametrize(
    "scalar",
    [
        np.int8(-1),
        np.uint64(2**64 - 1),
        np.float16(1.5),
        np.float32(1.5),
        np.float64(1.5),
        np.bool_(True),
    ],
)
def test_numpy_scalar_numeric_keys(scalar):
    assert ts.sort([scalar])[0] is scalar
    assert ts.argsort([object()], key=lambda _: scalar) == [0]
    assert list(ts.sort(array("i", [1]), key=lambda _: scalar)) == [1]


@pytest.mark.parametrize(
    "scalar",
    [
        np.complex64(1j),
        np.complex128(1j),
        np.datetime64("2020-01-01"),
        np.str_("1"),
        np.bytes_(b"1"),
    ],
)
def test_numpy_scalar_rejected_keys(scalar):
    with pytest.raises(TypeError):
        ts.sort([scalar])
    with pytest.raises(TypeError):
        ts.sort(np.array([1]), key=lambda _: scalar)


def test_key_output_alias(monkeypatch):
    a = np.array([3, 1, 2], dtype=np.intp)
    monkeypatch.setattr(np, "empty", lambda *args, **kwargs: a)
    for fn in (ts.sort, ts.argsort):
        with pytest.raises(ValueError, match="aliases"):
            fn(a, key=int)
    assert a.tolist() == [3, 1, 2]


@pytest.mark.skipif(
    sys.version_info < (3, 12), reason="PEP 688 __buffer__ requires Python 3.12+"
)
def test_key_buffer_fallback():
    class NoBuffer(np.ndarray):
        def __buffer__(self, flags):
            raise BufferError("buffer export disabled")

    a = np.array(
        [(3, "c"), (1, "a"), (2, "b")], dtype=[("score", "i8"), ("label", "U2")]
    ).view(NoBuffer)
    with pytest.raises(BufferError, match="disabled"):
        memoryview(a)
    check_key_operations(a, lambda r: r["score"], -1, threads=2)
    check_key_operations(
        a[::-1], lambda r: r["score"], None, descending=True, threads=2
    )


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
def test_key_pod_padding_and_bits(name):
    dtype = np.dtype(
        {"names": ["score"], "formats": ["i4"], "offsets": [4], "itemsize": 16}
    )
    data = np.arange(64, dtype="u1")
    a = data.view(dtype)
    a["score"] = [3, 1, 2, 1]
    order = [1, 3, 2, 0]
    saved = data.copy()
    expected = b"".join(a[i : i + 1].tobytes() for i in order)
    args = ([0, 1, 2, 3],) if "partition" in name else ()
    result = getattr(ts, name)(a, *args, key=lambda r: r["score"])
    if name.endswith("inplace"):
        assert result is None
        result = a
    if name.startswith("arg"):
        assert result.tolist() == order
        assert b"".join(a[i : i + 1].tobytes() for i in result) == expected
    else:
        assert result.tobytes() == expected

    data[:] = saved
    edited = False

    def key(record):
        nonlocal edited
        if not edited:
            data[0] ^= np.uint8(0xFF)
            edited = True
        return record["score"]

    saved[0] ^= np.uint8(0xFF)
    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *args, key=key)
    assert edited
    assert data.tobytes() == saved.tobytes()


@pytest.mark.skipif(
    tuple(int(x) for x in np.__version__.split(".")[:2]) < (2, 5),
    reason="descending=True requires NumPy 2.5+",
)
@pytest.mark.parametrize("axis", [None, 0, -1])
@pytest.mark.parametrize("dtype", ["i8", "u8", "f4", "f8"])
def test_descending_numpy_reference(axis, dtype):
    a = np.array([[3, 1, 3, 0], [0, 2, 1, 2]], dtype=dtype)
    if a.dtype.kind == "f":
        a[0, :2] = [np.nan, -0.0]
        a[1, :2] = [np.nan, 0.0]
    expected_indices = np.argsort(a, axis=axis, stable=True, descending=True)
    expected = np.sort(a, axis=axis, stable=True, descending=True)
    np.testing.assert_array_equal(reference_argsort(a, axis, True), expected_indices)
    np.testing.assert_array_equal(
        ts.argsort(a, axis=axis, descending=True), expected_indices
    )
    assert bits(ts.sort(a, axis=axis, descending=True)) == bits(expected)


@pytest.mark.skipif(
    np.dtype(np.longdouble).itemsize <= 8, reason="longdouble has no extra precision"
)
def test_extended_precision_keys_require_explicit_conversion():
    scalar = np.longdouble(1) + np.finfo(np.longdouble).eps
    message = r"extended-precision float keys are unsupported; convert explicitly with float\(\)"
    for a in ([scalar], array("d", [1]), np.array([scalar])):
        with pytest.raises(TypeError, match=message):
            ts.sort(a, key=lambda _: scalar)
        assert len(ts.sort(a, key=float)) == 1
    with pytest.raises(TypeError, match=message):
        ts.sort([scalar])


def test_interleaved_nonoverlapping_layout():
    a = np.ndarray((3, 2), dtype="i4", buffer=bytearray(32), strides=(8, 12))
    a[...] = [[6, 1], [4, 3], [2, 5]]
    for axis in (0, 1):
        check_operations(a, axis, False, 2)
        check_key_operations(a, int, axis, threads=2)


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
@pytest.mark.parametrize("raises", [False, True])
@pytest.mark.parametrize(
    "kind", ["scalar", "object", "struct", "nested_object", "datetime", "string"]
)
def test_key_element_mutation(name, raises, kind):
    if kind == "scalar":
        a = np.array([3.0, 1.0, 2.0])
    elif kind == "object":

        class Item:
            def __init__(self, score):
                self.score = score

            def __eq__(self, other):
                pytest.fail("object equality must not run")

        a = np.array([Item(3), Item(1), Item(2)], dtype=object)
    elif kind == "struct":
        a = np.zeros(
            3,
            dtype={
                "names": ["score"],
                "formats": ["i4"],
                "offsets": [4],
                "itemsize": 16,
            },
        )
        a["score"] = [3, 1, 2]
    elif kind == "nested_object":
        a = np.zeros(3, dtype=[("inner", [("score", "i4"), ("obj", object)])])
        a["inner"]["score"] = [3, 1, 2]
    elif kind == "datetime":
        a = np.array([3, 1, 2], dtype="datetime64[D]")
    else:
        if not hasattr(getattr(np, "dtypes", None), "StringDType"):
            pytest.skip("StringDType requires NumPy 2.0+")
        a = np.array(["3", "1", "2"], dtype=np.dtypes.StringDType())
    calls = []
    replacement = Item(9) if kind == "object" else None

    def key(x):
        if not calls:
            calls.append(True)
            if kind == "object":
                a[1] = replacement
            elif kind == "struct":
                a["score"][1] = 9
            elif kind == "nested_object":
                a["inner"]["obj"][1] = object()
            else:
                a[1] = 9
            if raises:
                raise RuntimeError("callback failed after edit")
        return 0

    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *([1] if "partition" in name else []), key=key)
    if kind == "object":
        assert a[1] is replacement
    elif kind == "struct":
        assert a["score"][1] == 9
    elif kind == "nested_object":
        assert type(a["inner"]["obj"][1]) is object
    else:
        assert int(a[1].astype("i8") if kind == "datetime" else a[1]) == 9


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
def test_object_subclass_lying_dtype_refcounts(name):
    import gc

    class LyingDType(np.ndarray):
        @property
        def dtype(self):
            return np.dtype("int64")

    first, second, replacement = object(), object(), object()
    a = np.array([first, second, first], dtype=object).view(LyingDType)
    before = [sys.getrefcount(x) for x in (first, second, replacement)]
    calls = []

    def key(x):
        if not calls:
            calls.append(True)
            a[1] = replacement
        return 0

    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *([1] if "partition" in name else []), key=key)
    gc.collect()
    assert a[0] is first and a[1] is replacement and a[2] is first
    assert [sys.getrefcount(x) for x in (first, second, replacement)] == [
        before[0],
        before[1] - 1,
        before[2] + 1,
    ]
    result = ts.sort(a, key=lambda x: int(x is first))
    assert result.dtype == np.dtype(object)
    assert result[0] is replacement


def test_kth_validation_precedes_numpy_allocation(monkeypatch):
    a = np.array([3, 1, 2])
    monkeypatch.setattr(
        np, "empty", lambda *args, **kwargs: pytest.fail("allocation before validation")
    )
    for fn in (ts.partition, ts.argpartition):
        with pytest.raises(ValueError, match="kth is out of bounds"):
            fn(a, 3)


def test_key_rejects_object_destination_for_raw_snapshot(monkeypatch):
    class LyingDType(np.ndarray):
        @property
        def dtype(self):
            return np.dtype("int64")

    first, second = object(), object()
    output = np.array([first, second], dtype=object).view(LyingDType)
    before = (sys.getrefcount(first), sys.getrefcount(second))
    monkeypatch.setattr(np, "empty", lambda *args, **kwargs: output)
    with pytest.raises(TypeError, match="output array does not match request|not POD"):
        ts.sort(np.array([2, 1], dtype="i8"), key=int)
    assert output[0] is first and output[1] is second
    assert (sys.getrefcount(first), sys.getrefcount(second)) == before


@pytest.mark.skipif(
    not hasattr(getattr(np, "dtypes", None), "StringDType"),
    reason="StringDType requires NumPy 2.0+",
)
@pytest.mark.parametrize("missing", [np.nan, None, object()])
def test_key_string_missing_values(missing):
    a = np.array(["bbb", missing, "a"], dtype=np.dtypes.StringDType(na_object=missing))
    result = ts.sort(a, key=lambda x: 0 if x is missing else len(x))
    assert result[0] is missing
    assert result[1:].tolist() == ["a", "bbb"]


@pytest.mark.skipif(
    np.dtype(np.clongdouble).itemsize <= 16,
    reason="clongdouble has no extended precision",
)
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
def test_key_extended_complex_nan_mutation(name):
    a = np.array(
        [np.clongdouble(complex(float("nan"), 1.0)), np.clongdouble(1 + 2j)],
        dtype=np.clongdouble,
    )
    calls = []

    def key(_):
        if not calls:
            calls.append(True)
            a[0] = np.clongdouble(complex(float("nan"), 9.0))
        return 0

    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *([0] if "partition" in name else []), key=key)
    assert float(a[0].imag) == 9.0


@pytest.mark.skipif(
    np.dtype(np.longdouble).itemsize <= 8,
    reason="longdouble has no extended precision",
)
def test_key_extended_float_signed_zero_mutation():
    a = np.array([np.longdouble(0.0), np.longdouble(1.0)], dtype=np.longdouble)
    calls = []

    def key(_):
        if not calls:
            calls.append(True)
            a[0] = np.longdouble(-0.0)
        return 0

    with pytest.raises(ValueError, match="array modified during sort"):
        ts.sort_inplace(a, key=key)
    assert bool(np.signbit(a[0])) is True


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
@pytest.mark.parametrize("key", [None, int])
@pytest.mark.parametrize("negative", [False, True])
def test_large_interleaved_layout(name, key, negative):
    a = np.ndarray((2, 65_536), dtype="i4", buffer=bytearray(524_296), strides=(12, 8))
    a[...] = (np.arange(a.size).reshape(a.shape) * 1777) % 100_003
    if negative:
        a = a[::-1, ::-1]
    expected = np.sort(a, axis=-1, kind="stable")
    ranks = [0, a.shape[-1] // 2, -1]
    args = (ranks,) if "partition" in name else ()
    assert getattr(ts, name)(a, *args, key=key) is None
    if "partition" in name:
        np.testing.assert_array_equal(a[:, ranks], expected[:, ranks])
        for rank in (0, a.shape[-1] // 2, a.shape[-1] - 1):
            assert np.all(a[:, :rank] <= a[:, rank, None])
            assert np.all(a[:, rank + 1 :] >= a[:, rank, None])
        np.testing.assert_array_equal(np.sort(a, axis=-1), expected)
    else:
        np.testing.assert_array_equal(a, expected)


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
@pytest.mark.parametrize("key", [None, int])
@pytest.mark.parametrize(
    "shape,strides", [((3, 65_536), (12, 8)), ((2, 65_536), (10, 8))]
)
def test_large_interleaved_overlap_rejected(name, key, shape, strides):
    a = np.ndarray(shape, dtype="i4", buffer=bytearray(524_308), strides=strides)
    before = a.tobytes()
    with pytest.raises(ValueError, match="overlapping"):
        getattr(ts, name)(a, *([0] if "partition" in name else []), key=key)
    assert a.tobytes() == before


def test_large_3d_interleaved_layout_beyond_65536():
    # (257, 257, 257) with strides (1028, 1032, 527356) has rest=66049 > 65536
    # yet every 4-byte element occupies a disjoint slot.
    backing = bytearray(135_532_000)
    a = np.ndarray(
        (257, 257, 257),
        dtype="i4",
        buffer=backing,
        strides=(1028, 1032, 527_356),
    )
    a[0, 0, :] = np.arange(257, 0, -1, dtype="i4")
    assert ts.partition_inplace(a, 0, axis=2) is None
    assert a[0, 0, 0] == 1
    bad = np.ndarray(
        (257, 257, 257),
        dtype="i4",
        buffer=backing,
        strides=(1028, 1032, 527_358),
    )
    with pytest.raises(ValueError, match="overlapping"):
        ts.partition_inplace(bad, 0, axis=2)


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
def test_key_zero_byte_record_layout(name):
    a = np.ndarray((2, 2, 2), dtype=np.dtype([]), buffer=bytearray(16), strides=(1, 1, 1))
    args = ([0],) if "partition" in name else ()
    assert getattr(ts, name)(a, *args, key=lambda _: 0) is None
    assert a.shape == (2, 2, 2)
    assert a.dtype == np.dtype([])


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
def test_key_object_subclass_overridden_setitem(name):
    class IgnoredSetter(np.ndarray):
        def __setitem__(self, index, value):
            return None

    class Record:
        def __init__(self, score):
            self.score = score

    r3, r1, r2 = Record(3), Record(1), Record(2)
    a = np.array([r3, r1, r2], dtype=object).view(IgnoredSetter)
    args = ([1],) if "partition" in name else ()
    assert getattr(ts, name)(a, *args, key=lambda r: r.score) is None
    if "partition" in name:
        assert a[1] is r2
        assert a[0] is r1 and a[2] is r3
    else:
        assert [a[0], a[1], a[2]] == [r1, r2, r3]


@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
def test_key_subclass_lying_zero_itemsize_overlapping_rejected(name):
    class ZeroItemsize(np.ndarray):
        @property
        def itemsize(self):
            return 0

    class Record:
        def __init__(self, score):
            self.score = score

    r3, r1, r2 = Record(3), Record(1), Record(2)
    backing = np.array([r3, r1, r2], dtype=object)
    overlap = np.lib.stride_tricks.as_strided(
        backing, shape=(2, 2), strides=(backing.strides[0], backing.strides[0])
    ).view(ZeroItemsize)
    args = ([0],) if "partition" in name else ()
    with pytest.raises(ValueError, match="overlapping"):
        getattr(ts, name)(overlap, *args, key=lambda r: r.score)
    assert [backing[0], backing[1], backing[2]] == [r3, r1, r2]


@pytest.mark.skipif(
    np.dtype(np.longdouble).itemsize <= 8, reason="longdouble has no extra precision"
)
@pytest.mark.parametrize("name", ["sort_inplace", "partition_inplace"])
def test_key_extended_float_nan_payload_mutation(name):
    a = np.array([np.longdouble("nan"), np.longdouble(1.0)], dtype=np.longdouble)
    raw = a.view(np.uint8).reshape(2, a.dtype.itemsize)
    big_endian = a.dtype.byteorder == ">" or (
        a.dtype.byteorder == "=" and sys.byteorder == "big"
    )
    idx = a.dtype.itemsize - 1 if big_endian else 0
    edited = False

    def key(value):
        nonlocal edited
        if not edited:
            # Flip a low mantissa bit of the NaN payload (never the x87 padding bytes).
            raw[0, idx] ^= np.uint8(1)
            edited = True
        return 0.0 if np.isnan(value) else float(value)

    expected_byte = int(raw[0, idx]) ^ 1
    args = ([0],) if "partition" in name else ()
    with pytest.raises(ValueError, match="array modified during sort"):
        getattr(ts, name)(a, *args, key=key)
    assert edited
    assert int(raw[0, idx]) == expected_byte


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
def test_key_many_empty_lanes(name):
    a = np.empty((1_000_000, 0), dtype="i8")
    args = ([],) if "partition" in name else ()

    def key(_):
        pytest.fail("key called for an empty array")

    result = getattr(ts, name)(a, *args, key=key)
    if name.endswith("inplace"):
        assert result is None
    else:
        assert result.shape == a.shape
        assert result.dtype == (
            np.dtype(np.intp) if name.startswith("arg") else a.dtype
        )
    if "partition" in name:
        with pytest.raises(ValueError):
            getattr(ts, name)(a, [0], key=key)
    a.flags.writeable = False
    if name.endswith("inplace"):
        with pytest.raises(ValueError, match="read-only"):
            getattr(ts, name)(a, *args, key=key)
