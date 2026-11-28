// SPDX-License-Identifier: BSD-2-Clause

use super::{Output, Request, keys};
use crate::args::{Axis, normalize_kth};
use crate::buffer::{self, Layout, RawSnapshot};
use crate::encoding::NumericKeys;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple};

// NumPy stays optional: even the type lookup must never initiate its import.
pub(super) fn detect<'py>(a: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Some(np) = keys::loaded_numpy(a.py())? else {
        return Ok(None);
    };
    if a.is_instance(&np.getattr("ndarray")?)? {
        Ok(Some(np))
    } else {
        Ok(None)
    }
}

pub(super) fn execute(
    a: &Bound<'_, PyAny>,
    np: &Bound<'_, PyAny>,
    request: Request<'_>,
    key: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    if let Some(key) = key {
        return execute_key(a, np, request, key);
    }
    if request.output == Output::Inplace {
        buffer::execute(a, None, request)?;
        return Ok(a.py().None());
    }
    let shape = buffer::validate_output(a, request)?;
    let dtype = if request.output == Output::Indices {
        np.getattr("intp")?
    } else {
        a.getattr("dtype")?
    };
    let output = empty(np, &shape, &dtype)?;
    buffer::execute(a, Some(&output), request)?;
    Ok(output.unbind())
}

fn empty<'py>(
    np: &Bound<'py, PyAny>,
    shape: &[usize],
    dtype: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let kwargs = PyDict::new(np.py());
    kwargs.set_item("dtype", dtype)?;
    np.getattr("empty")?.call((shape.to_vec(),), Some(&kwargs))
}

fn layout(a: &Bound<'_, PyAny>, axis: Axis) -> PyResult<Layout> {
    let shape: Vec<usize> = a.getattr("shape")?.extract()?;
    if a.getattr("ndim")?.extract::<usize>()? != shape.len() {
        return Err(PyValueError::new_err("ndim does not match shape"));
    }
    Layout::from_shape(
        shape,
        a.getattr("strides")?.extract()?,
        a.getattr("itemsize")?.extract()?,
        axis,
    )
}

fn writable(base: &Bound<'_, PyAny>, layout: &Layout) -> PyResult<()> {
    if !base
        .getattr("flags")?
        .getattr("writeable")?
        .extract::<bool>()?
    {
        return Err(PyValueError::new_err("array is read-only"));
    }
    layout.non_overlapping()
}

/// Allocates and validates NumPy-owned output before any element writeback.
fn allocate_output<'py>(
    a: &Bound<'py, PyAny>,
    np: &Bound<'py, PyAny>,
    ndarray: &Bound<'py, PyAny>,
    source: &Layout,
    request: Request<'_>,
    dtype: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let shape = source.out_shape();
    let dtype = if request.output == Output::Indices {
        np.getattr("intp")?
    } else {
        dtype.clone()
    };
    let output = empty(np, &shape, &dtype)?;
    if !output.is_instance(ndarray)? {
        return Err(PyTypeError::new_err("output array does not match request"));
    }
    let output_base = ndarray.getattr("view")?.call1((&output, ndarray))?;
    if output_base.getattr("shape")?.extract::<Vec<usize>>()? != shape
        || !output_base.getattr("dtype")?.eq(&dtype)?
    {
        return Err(PyTypeError::new_err("output array does not match request"));
    }
    if output.is(a)
        || np
            .getattr("shares_memory")?
            .call1((a, &output))?
            .extract::<bool>()?
    {
        return Err(PyValueError::new_err("output buffer aliases the input"));
    }
    let axis = if request.axis == Axis::None {
        Axis::Index(0)
    } else {
        request.axis
    };
    writable(&output_base, &layout(&output_base, axis)?)?;
    Ok(output)
}

// The snapshot owns both mutation evidence and the storage used for writeback.
enum Snapshot<'py> {
    Raw(RawSnapshot),
    RawBytes(RawSnapshot, Bound<'py, PyAny>),
    Owned(Bound<'py, PyAny>),
}

impl<'py> Snapshot<'py> {
    fn get(
        a: &Bound<'py, PyAny>,
        base: &Bound<'py, PyAny>,
        ndarray: &Bound<'py, PyAny>,
        axis: Axis,
    ) -> PyResult<Self> {
        Ok(match RawSnapshot::get(a, axis) {
            Ok(raw) if raw.byte_comparable() => Self::Raw(raw),
            // NumPy initializes Python bytes from padding; Rust keeps raw storage
            // as MaybeUninit throughout capture and permutation.
            Ok(raw) => Self::RawBytes(raw, base.call_method0("tobytes")?),
            Err(_) => Self::Owned(ndarray.getattr("copy")?.call1((base,))?),
        })
    }

    fn matches(&self, a: &Bound<'py, PyAny>, np: &Bound<'py, PyAny>, axis: Axis) -> PyResult<bool> {
        match self {
            Self::Raw(raw) => raw.matches(a, axis),
            Self::RawBytes(_, saved) => {
                let ndarray = np.getattr("ndarray")?;
                let live = ndarray.getattr("view")?.call1((a, &ndarray))?;
                live.call_method0("tobytes")?.eq(saved)
            }
            Self::Owned(saved) => {
                let ndarray = np.getattr("ndarray")?;
                let live = ndarray.getattr("view")?.call1((a, &ndarray))?;
                same_elements(np, &live, saved)
            }
        }
    }

    fn write(
        &self,
        output: &Bound<'py, PyAny>,
        ndarray: &Bound<'py, PyAny>,
        axis: Axis,
        source: &Layout,
        target: &Layout,
        indices: &[Vec<usize>],
    ) -> PyResult<()> {
        match self {
            Self::Raw(raw) | Self::RawBytes(raw, _) => raw.write(output, axis, indices),
            Self::Owned(copy) => {
                // Write through a base ndarray view so subclass __setitem__ overrides
                // cannot silently drop or alter in-place permutations.
                let dst = ndarray.getattr("view")?.call1((output, ndarray))?;
                for (lane, order) in indices.iter().enumerate() {
                    for (i, &index) in order.iter().enumerate() {
                        let from = PyTuple::new(output.py(), source.coordinates(lane, index))?;
                        let to = PyTuple::new(output.py(), target.coordinates(lane, i))?;
                        dst.set_item(to, copy.get_item(from)?)?;
                    }
                }
                Ok(())
            }
        }
    }
}

fn execute_key(
    a: &Bound<'_, PyAny>,
    np: &Bound<'_, PyAny>,
    request: Request<'_>,
    key: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let py = a.py();
    // The base view exposes the real dtype even when a subclass overrides it.
    let ndarray = np.getattr("ndarray")?;
    let base = ndarray.getattr("view")?.call1((a, &ndarray))?;
    let source = layout(&base, request.axis)?;
    let ranks = request
        .kth
        .map(|raw| normalize_kth(raw, source.lane_len))
        .transpose()?;
    if request.output == Output::Inplace {
        writable(&base, &source)?;
    }
    let dtype = base.getattr("dtype")?;
    if source.total_len == 0 || source.lane_len == 0 {
        return if request.output == Output::Inplace {
            Ok(py.None())
        } else {
            Ok(allocate_output(a, np, &ndarray, &source, request, &dtype)?.unbind())
        };
    }
    let reported_dtype = a.getattr("dtype")?;
    let strides: Vec<isize> = a.getattr("strides")?.extract()?;
    let unchanged = || {
        if a.getattr("shape")?.extract::<Vec<usize>>()? != source.shape
            || a.getattr("strides")?.extract::<Vec<isize>>()? != strides
            || !a.getattr("dtype")?.eq(&reported_dtype)?
        {
            return Err(PyValueError::new_err("array modified during sort"));
        }
        Ok(())
    };
    // Only the exported PEP 3118 format authorizes raw copying. NumPy owns
    // references and custom dtype storage when that format is unavailable.
    let snapshot = Snapshot::get(a, &base, &ndarray, request.axis)?;
    let content_unchanged = || {
        unchanged()?;
        let matches = snapshot.matches(a, np, request.axis)?;
        if !matches {
            return Err(PyValueError::new_err("array modified during sort"));
        }
        Ok(())
    };
    let scalars = keys::ScalarTypes::new(py)?;
    let mut lanes = Vec::with_capacity(source.num_lanes);
    for lane in 0..source.num_lanes {
        let mut numeric = NumericKeys::Empty;
        for i in 0..source.lane_len {
            unchanged()?;
            let position = PyTuple::new(py, source.coordinates(lane, i))?;
            let result = key.call1((base.get_item(position)?,));
            unchanged()?;
            if result.is_err() {
                content_unchanged()?;
            }
            let value = result?;
            if let Err(err) = scalars.push(&mut numeric, &value) {
                content_unchanged()?;
                return Err(err);
            }
        }
        lanes.push(numeric);
    }
    content_unchanged()?;
    let indices = keys::order_lanes(py, &lanes, ranks.as_deref(), request)?;
    let out_axis = if request.axis == Axis::None {
        Axis::Index(0)
    } else {
        request.axis
    };
    let output = if request.output == Output::Inplace {
        a.clone()
    } else {
        allocate_output(a, np, &ndarray, &source, request, &dtype)?
    };
    content_unchanged()?;
    let output_base = ndarray.getattr("view")?.call1((&output, &ndarray))?;
    let target = layout(&output_base, out_axis)?;
    writable(&output_base, &target)?;
    match request.output {
        Output::Indices => buffer::write_indices(&output, out_axis, &indices)?,
        _ => snapshot.write(&output, &ndarray, out_axis, &source, &target, &indices)?,
    }

    Ok(if request.output == Output::Inplace {
        py.None()
    } else {
        output.unbind()
    })
}

/// Compare fields rather than struct padding, and object identities rather than
/// Python equality. Owned copies keep object slots alive throughout callbacks.
fn same_elements(
    np: &Bound<'_, PyAny>,
    live: &Bound<'_, PyAny>,
    saved: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    if !live.getattr("shape")?.eq(saved.getattr("shape")?)?
        || !live.getattr("dtype")?.eq(saved.getattr("dtype")?)?
    {
        return Ok(false);
    }
    let dtype = saved.getattr("dtype")?;
    let names = dtype.getattr("names")?;
    if !names.is_none() {
        for name in names.try_iter()? {
            let name = name?;
            if !same_elements(np, &live.get_item(&name)?, &saved.get_item(&name)?)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let kind: String = dtype.getattr("kind")?.extract()?;
    if kind == "O" || kind == "T" {
        for (a, b) in live
            .getattr("flat")?
            .try_iter()?
            .zip(saved.getattr("flat")?.try_iter()?)
        {
            let (a, b) = (a?, b?);
            if !a.is(&b)
                && !(kind == "T"
                    && a.is_instance_of::<PyString>()
                    && b.is_instance_of::<PyString>()
                    && a.eq(&b)?)
            {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let itemsize = dtype.getattr("itemsize")?.extract::<usize>()?;
    if kind == "c" && itemsize > 16 {
        return Ok(
            same_elements(np, &live.getattr("real")?, &saved.getattr("real")?)?
                && same_elements(np, &live.getattr("imag")?, &saved.getattr("imag")?)?,
        );
    }
    if kind == "f" && itemsize > 8 {
        // Compare all significant extended-float bytes (including signed zeros and
        // NaN payloads) while skipping uninitialized x87 80-bit alignment padding.
        let finfo = np.getattr("finfo")?.call1((&dtype,))?;
        let nmant: usize = finfo.getattr("nmant")?.extract()?;
        let nexp: usize = finfo.getattr("nexp")?.extract()?;
        let used = if nmant == 63 && nexp == 15 && itemsize >= 10 {
            10
        } else {
            itemsize
        };
        let byteorder: String = dtype.getattr("byteorder")?.extract()?;
        let big_endian = byteorder == ">" || (byteorder == "=" && cfg!(target_endian = "big"));
        let (start, end) = if big_endian {
            (itemsize - used, itemsize)
        } else {
            (0, used)
        };
        let live_bytes: Vec<u8> = live.call_method0("tobytes")?.extract()?;
        let saved_bytes: Vec<u8> = saved.call_method0("tobytes")?.extract()?;
        if live_bytes.len() != saved_bytes.len() {
            return Ok(false);
        }
        return Ok(live_bytes
            .chunks_exact(itemsize)
            .zip(saved_bytes.chunks_exact(itemsize))
            .all(|(a, b)| a[start..end] == b[start..end]));
    }
    // NumPy performs the byte copy; Rust never interprets dtype padding as u8.
    live.call_method0("tobytes")?
        .eq(saved.call_method0("tobytes")?)
}
