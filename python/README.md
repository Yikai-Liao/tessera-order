# tessera-order for Python

Stable radix sorting, argsort and selection for Python lists, `array.array`, and optional NumPy arrays. Requires Python 3.9 or newer.

```python
import tessera_order as ts

assert ts.sort([3, 1, 2]) == [1, 2, 3]
assert ts.argsort([3, 1, 2]) == [1, 2, 0]
assert ts.partition([3, 1, 2], 1)[1] == 2
```

`sort`, `argsort`, `partition` and `argpartition` return new containers. `sort_inplace` and `partition_inplace` update the input and return `None`. Use `axis` to select NumPy lanes, `descending=True` for descending order, `key=` for cached real numeric keys, and `threads=` to select serial or parallel execution. NaNs sort last in either direction; equal keys retain input order. Extended-precision float keys require explicit `float()` conversion.

Callbacks that replace elements or change the input layout raise `ValueError`; callback edits remain in the input. Concurrent C-level mutation of the same buffer during a call is undefined, including NumPy operations that release the GIL.

See the [binding contract](https://github.com/Yikai-Liao/tessera-order/blob/main/docs/design/python-binding.md), [documentation](https://yikai-liao.github.io/tessera-order/) and [source repository](https://github.com/Yikai-Liao/tessera-order).

Licensed under BSD-2-Clause. See [LICENSE](LICENSE).
