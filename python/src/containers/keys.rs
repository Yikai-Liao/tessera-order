// SPDX-License-Identifier: BSD-2-Clause

//! Numeric key admission and detached ordering shared by Python containers.

use super::Request;
use crate::encoding::{NumericError, NumericKeys};
use crate::pool::Threads;
use pyo3::exceptions::{PyOverflowError, PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt};
use rayon::prelude::*;

pub(super) fn loaded_numpy(py: Python<'_>) -> PyResult<Option<Bound<'_, PyAny>>> {
    Ok(py
        .import("sys")?
        .getattr("modules")?
        .cast::<PyDict>()?
        .get_item("numpy")?
        .filter(|np| !np.is_none()))
}

/// Resolve optional `NumPy` scalar families once for one public call.
pub(super) struct ScalarTypes<'py> {
    numpy: Option<[Bound<'py, PyAny>; 3]>,
}

impl<'py> ScalarTypes<'py> {
    pub(super) fn new(py: Python<'py>) -> PyResult<Self> {
        let numpy = loaded_numpy(py)?
            .map(|np| {
                Ok::<_, PyErr>([
                    np.getattr("integer")?,
                    np.getattr("bool_")?,
                    np.getattr("floating")?,
                ])
            })
            .transpose()?;
        Ok(Self { numpy })
    }

    /// Admit only real scalar families, never arbitrary conversion protocols.
    pub(super) fn push(&self, keys: &mut NumericKeys, obj: &Bound<'_, PyAny>) -> PyResult<()> {
        let result = if obj.is_instance_of::<PyInt>() {
            push_int(keys, obj)?
        } else if obj.is_instance_of::<PyFloat>() {
            keys.push_f64(obj.extract()?)
        } else if let Some([integer, boolean, floating]) = &self.numpy {
            if obj.is_instance(integer)? {
                push_int(keys, obj)?
            } else if obj.is_instance(boolean)? {
                keys.push_i128(i128::from(obj.is_truthy()?))
            } else if obj.is_instance(floating)? {
                if obj
                    .getattr("dtype")?
                    .getattr("itemsize")?
                    .extract::<usize>()?
                    > 8
                {
                    return Err(PyTypeError::new_err(
                        "extended-precision float keys are unsupported; convert explicitly with float()",
                    ));
                }
                keys.push_f64(obj.extract()?)
            } else {
                return Err(PyTypeError::new_err(
                    "keys must be real integer or float scalars",
                ));
            }
        } else {
            return Err(PyTypeError::new_err(
                "keys must be real integer or float scalars",
            ));
        };
        result.map_err(|err| match err {
            NumericError::Overflow => {
                PyOverflowError::new_err("integer keys exceed a common 128-bit range")
            }
            NumericError::MixedNumeric => {
                PyTypeError::new_err("integer and float keys cannot be mixed")
            }
        })
    }
}

fn push_int(keys: &mut NumericKeys, obj: &Bound<'_, PyAny>) -> PyResult<Result<(), NumericError>> {
    if let Ok(value) = obj.extract::<i128>() {
        Ok(keys.push_i128(value))
    } else if let Ok(value) = obj.extract::<u128>() {
        Ok(keys.push_u128(value))
    } else {
        Err(PyOverflowError::new_err("integer key exceeds 128 bits"))
    }
}

pub(super) fn order_lanes(
    py: Python<'_>,
    lanes: &[NumericKeys],
    ranks: Option<&[usize]>,
    request: Request<'_>,
) -> PyResult<Vec<Vec<usize>>> {
    let parallel = request.threads != Threads::Serial;
    py.detach(|| {
        request.threads.run(|| {
            let lane_len = lanes.first().map_or(0, |keys| match keys {
                NumericKeys::Empty => 0,
                NumericKeys::I64(v) => v.len(),
                NumericKeys::U64(v) => v.len(),
                NumericKeys::I128(v) => v.len(),
                NumericKeys::U128(v) => v.len(),
                NumericKeys::F64(v) => v.len(),
            });
            let inner = crate::buffer::inner_parallel(parallel, lanes.len(), lane_len);
            let order = |keys: &NumericKeys| keys.order_indices(ranks, request.descending, inner);
            if parallel && lanes.len() > 1 {
                lanes.par_iter().map(order).collect()
            } else {
                lanes.iter().map(order).collect()
            }
        })
    })
    .map_err(|err| PyRuntimeError::new_err(err.to_string()))
}
