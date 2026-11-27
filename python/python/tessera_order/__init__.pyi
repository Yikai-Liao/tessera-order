# SPDX-License-Identifier: BSD-2-Clause

from array import array
from typing import (
    TYPE_CHECKING,
    Any,
    Callable,
    Optional,
    Sequence,
    SupportsIndex,
    TypeVar,
    Union,
    overload,
)

# NumPy is an optional dependency; unresolved annotations stay Any without it.
if TYPE_CHECKING:
    import numpy as np  # type: ignore[import-not-found, unused-ignore]

__version__: str
__core_version__: str

_T = TypeVar("_T")
_Value = TypeVar("_Value", int, float, str)
_DType = TypeVar("_DType", bound="np.dtype[Any]")
_Axis = Optional[SupportsIndex]
_Kth = Union[SupportsIndex, Sequence[SupportsIndex], "np.ndarray[Any, Any]"]

@overload
def sort(
    a: list[_T],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[_T]: ...
@overload
def sort(
    a: array[_Value],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> array[_Value]: ...
@overload
def sort(
    a: np.ndarray[Any, _DType],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> np.ndarray[Any, _DType]: ...
@overload
def sort_inplace(
    a: list[_T],
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def sort_inplace(
    a: array[_Value],
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def sort_inplace(
    a: np.ndarray[Any, _DType],
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def argsort(
    a: list[_T],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[int]: ...
@overload
def argsort(
    a: array[_Value],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[int]: ...
@overload
def argsort(
    a: np.ndarray[Any, _DType],
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> np.ndarray[Any, np.dtype[np.intp]]: ...
@overload
def partition(
    a: list[_T],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[_T]: ...
@overload
def partition(
    a: array[_Value],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> array[_Value]: ...
@overload
def partition(
    a: np.ndarray[Any, _DType],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> np.ndarray[Any, _DType]: ...
@overload
def partition_inplace(
    a: list[_T],
    kth: _Kth,
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def partition_inplace(
    a: array[_Value],
    kth: _Kth,
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def partition_inplace(
    a: np.ndarray[Any, _DType],
    kth: _Kth,
    axis: SupportsIndex = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> None: ...
@overload
def argpartition(
    a: list[_T],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_T], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[int]: ...
@overload
def argpartition(
    a: array[_Value],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[_Value], Any]] = ...,
    threads: Optional[int] = ...,
) -> list[int]: ...
@overload
def argpartition(
    a: np.ndarray[Any, _DType],
    kth: _Kth,
    axis: _Axis = ...,
    *,
    descending: Optional[bool] = ...,
    key: Optional[Callable[[Any], Any]] = ...,
    threads: Optional[int] = ...,
) -> np.ndarray[Any, np.dtype[np.intp]]: ...
