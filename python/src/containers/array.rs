// SPDX-License-Identifier: BSD-2-Clause

use super::{Output, Request, keys, validate_1d};
use crate::buffer;
use crate::encoding::NumericKeys;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyList, PyType};

static ARRAY: PyOnceLock<Py<PyType>> = PyOnceLock::new();

pub(super) fn is_array(a: &Bound<'_, PyAny>) -> PyResult<bool> {
    let ty = ARRAY.get_or_try_init(a.py(), || {
        Ok::<_, PyErr>(
            a.py()
                .import("array")?
                .getattr("array")?
                .cast_into::<PyType>()?
                .unbind(),
        )
    })?;
    a.is_instance(ty.bind(a.py()))
}

pub(super) fn execute(
    a: &Bound<'_, PyAny>,
    request: Request<'_>,
    key: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    if let Some(key) = key {
        let ranks = validate_1d(request, a.len()?)?;
        return execute_key(a, request, key, ranks.as_deref());
    }
    let py = a.py();
    match request.output {
        Output::Copy => {
            buffer::validate_output(a, request)?;
            let copy = a.call_method0("__copy__")?;
            if !copy.getattr("typecode")?.eq(a.getattr("typecode")?)? {
                return Err(PyValueError::new_err("copy has a different typecode"));
            }
            buffer::check_copy(a, &copy, request.axis)?;
            buffer::execute(
                &copy,
                None,
                Request {
                    output: Output::Inplace,
                    ..request
                },
            )?;
            Ok(copy.unbind())
        }
        Output::Inplace => {
            buffer::execute(a, None, request)?;
            Ok(py.None())
        }
        Output::Indices => {
            let indices = buffer::execute(a, None, request)?;
            Ok(PyList::new(py, indices)?.into_any().unbind())
        }
    }
}

fn execute_key(
    a: &Bound<'_, PyAny>,
    request: Request<'_>,
    key: &Bound<'_, PyAny>,
    ranks: Option<&[usize]>,
) -> PyResult<Py<PyAny>> {
    let py = a.py();
    let len = a.len()?;
    let snapshot = buffer::RawSnapshot::get(a, request.axis)?;
    let unchanged = || {
        if a.len()? != len || !snapshot.matches(a, request.axis)? {
            Err(PyValueError::new_err("array modified during sort"))
        } else {
            Ok(())
        }
    };
    let scalars = keys::ScalarTypes::new(py)?;
    let mut numeric = NumericKeys::Empty;
    for i in 0..len {
        if a.len()? != len {
            return Err(PyValueError::new_err("array modified during sort"));
        }
        let result = key.call1((a.get_item(i)?,));
        if result.is_err() {
            unchanged()?;
        }
        let value = result?;
        if let Err(err) = scalars.push(&mut numeric, &value) {
            unchanged()?;
            return Err(err);
        }
    }
    let indices = keys::order_lanes(py, &[numeric], ranks, request)?;
    unchanged()?;
    if request.output == Output::Indices {
        return Ok(PyList::new(py, &indices[0])?.into_any().unbind());
    }
    let output = if request.output == Output::Copy {
        let copy = a.call_method0("__copy__")?;
        if !copy.getattr("typecode")?.eq(a.getattr("typecode")?)? {
            return Err(PyValueError::new_err("copy has a different typecode"));
        }
        buffer::check_copy(a, &copy, request.axis)?;
        copy
    } else {
        a.clone()
    };
    unchanged()?;
    snapshot.write(&output, request.axis, &indices)?;
    Ok(if request.output == Output::Inplace {
        py.None()
    } else {
        output.unbind()
    })
}
