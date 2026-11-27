# SPDX-License-Identifier: BSD-2-Clause

"""Stable ordering for lists, array.array, and optional NumPy arrays.

Concurrent C-level mutation of the same buffer during a call is undefined,
including NumPy operations that have released the GIL.
"""

from ._core import (
    __core_version__,
    __version__,
    argpartition,
    argsort,
    partition,
    partition_inplace,
    sort,
    sort_inplace,
)

__all__ = [
    "sort",
    "sort_inplace",
    "argsort",
    "partition",
    "partition_inplace",
    "argpartition",
    "__version__",
    "__core_version__",
]
