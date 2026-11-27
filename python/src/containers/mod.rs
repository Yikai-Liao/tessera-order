// SPDX-License-Identifier: BSD-2-Clause

//! Container detection and result allocation; buffer.rs owns numeric execution.

mod array;
mod keys;
mod list;
mod numpy;

use crate::args::{Axis, RawKth, normalize_kth};
use crate::pool::Threads;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Output {
    Copy,
    Inplace,
    Indices,
}

#[derive(Clone, Copy)]
pub(crate) struct Request<'a> {
    pub output: Output,
    pub axis: Axis,
    pub descending: bool,
    pub threads: Threads,
    pub kth: Option<&'a RawKth>,
}

pub(crate) fn dispatch(
    a: &Bound<'_, PyAny>,
    request: Request<'_>,
    key: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    if let Ok(list) = a.cast::<PyList>() {
        return list::execute(list, request, key);
    }
    if array::is_array(a)? {
        return array::execute(a, request, key);
    }
    if let Some(np) = numpy::detect(a)? {
        return numpy::execute(a, &np, request, key);
    }
    Err(PyTypeError::new_err(
        "expected list, array.array, or numpy.ndarray",
    ))
}

fn validate_1d(request: Request<'_>, len: usize) -> PyResult<Option<Vec<usize>>> {
    if !matches!(request.axis, Axis::None | Axis::Index(-1 | 0)) {
        return Err(PyValueError::new_err("axis is out of bounds"));
    }
    request.kth.map(|raw| normalize_kth(raw, len)).transpose()
}
