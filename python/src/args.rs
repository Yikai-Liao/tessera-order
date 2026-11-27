// SPDX-License-Identifier: BSD-2-Clause

//! Shared Python argument validation; lane bounds are checked after dispatch.

use crate::pool::Threads;
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyString};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    None,
    Index(isize),
}

pub(crate) fn parse_axis(obj: &Bound<'_, PyAny>, allow_none: bool) -> PyResult<Axis> {
    if obj.is_none() {
        return if allow_none {
            Ok(Axis::None)
        } else {
            Err(PyTypeError::new_err(
                "axis=None is not supported for in-place operations",
            ))
        };
    }
    reject_bool(obj, "axis")?;
    let index = obj.extract().map_err(|err: PyErr| {
        if err.is_instance_of::<PyOverflowError>(obj.py()) {
            PyValueError::new_err("axis is out of bounds")
        } else {
            err
        }
    })?;
    Ok(Axis::Index(index))
}

pub(crate) fn parse_descending(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    if obj.is_none() {
        return Ok(false);
    }
    if !obj.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err("descending must be bool or None"));
    }
    obj.extract()
}

pub(crate) fn parse_threads(obj: &Bound<'_, PyAny>) -> PyResult<Threads> {
    if obj.is_none() {
        return Ok(Threads::Global);
    }
    reject_bool(obj, "threads")?;
    match obj.extract::<isize>() {
        Ok(n) if n < 1 => return Err(PyValueError::new_err("threads must be at least 1")),
        Ok(1) => return Ok(Threads::Serial),
        Ok(n) => return Ok(Threads::Fixed(n as usize)),
        Err(err) if !err.is_instance_of::<PyOverflowError>(obj.py()) => return Err(err),
        Err(_) => {}
    }
    let index = obj
        .py()
        .import("operator")?
        .getattr("index")?
        .call1((obj,))?;
    if index.lt(1)? {
        return Err(PyValueError::new_err("threads must be at least 1"));
    }
    match index.extract::<usize>()? {
        1 => Ok(Threads::Serial),
        n => Ok(Threads::Fixed(n)),
    }
}

pub(crate) fn parse_key<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyAny>>> {
    if obj.is_none() {
        Ok(None)
    } else if obj.is_callable() {
        Ok(Some(obj.clone()))
    } else {
        Err(PyTypeError::new_err("key must be callable or None"))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RawKth {
    Single(isize),
    Many(Vec<isize>),
}

pub(crate) fn parse_kth(obj: &Bound<'_, PyAny>) -> PyResult<RawKth> {
    fn rank(obj: &Bound<'_, PyAny>) -> PyResult<isize> {
        reject_bool(obj, "kth")?;
        obj.extract().map_err(|err: PyErr| {
            if err.is_instance_of::<PyOverflowError>(obj.py()) {
                PyValueError::new_err("kth is out of bounds")
            } else {
                err
            }
        })
    }
    reject_bool(obj, "kth")?;
    match rank(obj) {
        Ok(value) => return Ok(RawKth::Single(value)),
        Err(err) if !err.is_instance_of::<PyTypeError>(obj.py()) => return Err(err),
        Err(_) => {}
    }
    if obj.is_instance_of::<PyString>()
        || obj.is_instance_of::<PyBytes>()
        || obj.is_instance_of::<PyByteArray>()
    {
        return Err(PyTypeError::new_err(
            "kth must be an integer or a sequence of integers",
        ));
    }
    // Vec extraction accepts the sequence protocol, including NumPy arrays.
    let sequence = obj.extract::<Vec<Bound<'_, PyAny>>>()?;
    let ranks = sequence.iter().map(rank).collect::<PyResult<_>>()?;
    Ok(RawKth::Many(ranks))
}

/// Normalizes negative ranks, checks lane bounds, and sorts and deduplicates.
pub(crate) fn normalize_kth(raw: &RawKth, len: usize) -> PyResult<Vec<usize>> {
    let ranks = match raw {
        RawKth::Single(rank) => std::slice::from_ref(rank),
        RawKth::Many(ranks) => ranks,
    };
    let mut normalized = ranks
        .iter()
        .map(|&rank| {
            let index = if rank < 0 {
                len as i128 + rank as i128
            } else {
                rank as i128
            };
            if index < 0 || index >= len as i128 {
                Err(PyValueError::new_err("kth is out of bounds"))
            } else {
                Ok(index as usize)
            }
        })
        .collect::<PyResult<Vec<_>>>()?;
    normalized.sort_unstable();
    normalized.dedup();
    Ok(normalized)
}

fn reject_bool(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<()> {
    if obj.is_instance_of::<PyBool>() {
        Err(PyTypeError::new_err(format!("{name} must not be bool")))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn parsers_and_rank_normalization() {
        Python::initialize();
        Python::attach(|py| {
            let eval = |source: &str| py.eval(&CString::new(source).unwrap(), None, None).unwrap();
            assert_eq!(parse_axis(&eval("None"), true).unwrap(), Axis::None);
            assert_eq!(parse_axis(&eval("-1"), false).unwrap(), Axis::Index(-1));
            assert!(
                parse_axis(&eval("None"), false)
                    .unwrap_err()
                    .is_instance_of::<PyTypeError>(py)
            );
            for value in ["True", "1.0", "'0'"] {
                assert!(
                    parse_axis(&eval(value), true)
                        .unwrap_err()
                        .is_instance_of::<PyTypeError>(py)
                );
            }
            for value in ["2**200", "-2**200"] {
                assert!(
                    parse_axis(&eval(value), true)
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
            }
            for (source, expected) in [("None", false), ("False", false), ("True", true)] {
                assert_eq!(parse_descending(&eval(source)).unwrap(), expected);
            }
            for value in ["0", "1", "''", "[]"] {
                assert!(
                    parse_descending(&eval(value))
                        .unwrap_err()
                        .is_instance_of::<PyTypeError>(py)
                );
            }
            for (source, expected) in [
                ("None", Threads::Global),
                ("1", Threads::Serial),
                ("3", Threads::Fixed(3)),
                (
                    "type('Index', (), {'__index__': lambda self: 4})()",
                    Threads::Fixed(4),
                ),
            ] {
                assert_eq!(parse_threads(&eval(source)).unwrap(), expected);
            }
            for value in ["True", "False", "1.0", "0.0", "'2'"] {
                assert!(
                    parse_threads(&eval(value))
                        .unwrap_err()
                        .is_instance_of::<PyTypeError>(py)
                );
            }
            for value in ["0", "-1", "-2**200"] {
                assert!(
                    parse_threads(&eval(value))
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
            }
            assert!(
                parse_threads(&eval("2**200"))
                    .unwrap_err()
                    .is_instance_of::<PyOverflowError>(py)
            );
            assert!(parse_key(&eval("None")).unwrap().is_none());
            assert!(parse_key(&eval("abs")).unwrap().is_some());
            assert!(
                parse_key(&eval("1"))
                    .unwrap_err()
                    .is_instance_of::<PyTypeError>(py)
            );
            assert_eq!(parse_kth(&eval("-1")).unwrap(), RawKth::Single(-1));
            assert_eq!(
                parse_kth(&eval("(2, -1, 2, 0)")).unwrap(),
                RawKth::Many(vec![2, -1, 2, 0])
            );
            assert_eq!(parse_kth(&eval("[]")).unwrap(), RawKth::Many(vec![]));
            assert_eq!(
                parse_kth(&eval("range(3)")).unwrap(),
                RawKth::Many(vec![0, 1, 2])
            );
            // A sequence can also expose a scalar __index__ that rejects non-scalars.
            let sequence = eval(
                "type('Ranks', (), {'__getitem__': lambda self, i: (2, -1)[i], '__index__': lambda self: None})()",
            );
            assert_eq!(parse_kth(&sequence).unwrap(), RawKth::Many(vec![2, -1]));
            for value in [
                "True",
                "[False]",
                "1.5",
                "[1.5]",
                "''",
                "b''",
                "None",
                "iter([1])",
            ] {
                assert!(
                    parse_kth(&eval(value))
                        .unwrap_err()
                        .is_instance_of::<PyTypeError>(py),
                    "{value}"
                );
            }
            for value in ["2**200", "[-2**200]"] {
                assert!(
                    parse_kth(&eval(value))
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
            }
            assert_eq!(
                normalize_kth(&RawKth::Many(vec![2, -1, 2, 0, -4]), 4).unwrap(),
                [0, 2, 3]
            );
            assert_eq!(normalize_kth(&RawKth::Single(-1), 1).unwrap(), [0]);
            assert!(normalize_kth(&RawKth::Many(vec![]), 0).unwrap().is_empty());
            for (rank, len) in [(0, 0), (-1, 0), (-5, 4), (4, 4), (isize::MIN, 1)] {
                assert!(
                    normalize_kth(&RawKth::Single(rank), len)
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
            }
        });
    }
}
