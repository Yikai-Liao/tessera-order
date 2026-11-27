// SPDX-License-Identifier: BSD-2-Clause

//! Snapshot list identities before callbacks; write back only after validation.

use super::{Output, Request, keys, validate_1d};
use crate::encoding::NumericKeys;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;

pub(super) fn execute(
    a: &Bound<'_, PyList>,
    request: Request<'_>,
    key: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let ranks = validate_1d(request, a.len())?;
    let py = a.py();
    let items: Vec<Py<PyAny>> = a.iter().map(Bound::unbind).collect();
    let ptrs: Vec<_> = if key.is_some() {
        items.iter().map(Py::as_ptr).collect()
    } else {
        Vec::new()
    };
    let unchanged = || {
        if a.len() != ptrs.len() || a.iter().zip(&ptrs).any(|(item, &ptr)| item.as_ptr() != ptr) {
            Err(PyValueError::new_err("list modified during sort"))
        } else {
            Ok(())
        }
    };
    let scalars = keys::ScalarTypes::new(py)?;
    let mut numeric = NumericKeys::Empty;
    for item in &items {
        if let Some(key) = key {
            if a.len() != items.len() {
                return Err(PyValueError::new_err("list modified during sort"));
            }
            let value = key.call1((item,))?;
            if a.len() != items.len() {
                return Err(PyValueError::new_err("list modified during sort"));
            }
            scalars.push(&mut numeric, &value)?;
        } else {
            scalars.push(&mut numeric, item.bind(py))?;
        }
    }
    if key.is_some() {
        unchanged()?;
    }
    let indices = keys::order_lanes(py, &[numeric], ranks.as_deref(), request)?
        .pop()
        .unwrap();
    match request.output {
        Output::Indices => Ok(PyList::new(py, indices)?.into_any().unbind()),
        Output::Copy => Ok(PyList::new(py, indices.iter().map(|&i| &items[i]))?
            .into_any()
            .unbind()),
        Output::Inplace => {
            if key.is_some() {
                unchanged()?;
            }
            for (i, &source) in indices.iter().enumerate() {
                a.set_item(i, &items[source])?;
            }
            Ok(py.None())
        }
    }
}
