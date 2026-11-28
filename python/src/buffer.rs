// SPDX-License-Identifier: BSD-2-Clause

//! Numeric buffer access and lane execution. Exporters stay pinned until return;
//! only owned scratch crosses a GIL detach. Callers must prevent concurrent
//! C-level mutation of the input, even from code that has released the GIL.

use crate::args::{Axis, normalize_kth};
use crate::containers::{Output, Request};
use crate::encoding::{
    NumericElem, argpartition_slice, argsort_slice, partition_slice, sort_slice,
};
use crate::pool::Threads;
use pyo3::exceptions::{PyBufferError, PyMemoryError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::ffi::CStr;
use std::mem::{MaybeUninit, align_of, size_of};
use std::ptr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ByteOrder {
    Native,
    Swapped,
}

fn dtype(format: &[u8], itemsize: usize) -> PyResult<(DType, ByteOrder)> {
    let (prefix, code) = match format {
        [code] => (b'@', *code),
        [prefix @ (b'@' | b'=' | b'<' | b'>' | b'!'), code] => (*prefix, *code),
        _ => return Err(PyTypeError::new_err("unsupported buffer format")),
    };
    let dtype = match (code, itemsize) {
        (b'b' | b'h' | b'i' | b'l' | b'q' | b'n', 1) => DType::I8,
        (b'b' | b'h' | b'i' | b'l' | b'q' | b'n', 2) => DType::I16,
        (b'b' | b'h' | b'i' | b'l' | b'q' | b'n', 4) => DType::I32,
        (b'b' | b'h' | b'i' | b'l' | b'q' | b'n', 8) => DType::I64,
        (b'B' | b'H' | b'I' | b'L' | b'Q' | b'N', 1) => DType::U8,
        (b'B' | b'H' | b'I' | b'L' | b'Q' | b'N', 2) => DType::U16,
        (b'B' | b'H' | b'I' | b'L' | b'Q' | b'N', 4) => DType::U32,
        (b'B' | b'H' | b'I' | b'L' | b'Q' | b'N', 8) => DType::U64,
        (b'f', 4) => DType::F32,
        (b'd', 8) => DType::F64,
        _ => {
            return Err(PyTypeError::new_err(
                "unsupported numeric buffer dtype or itemsize",
            ));
        }
    };
    let swapped = itemsize > 1
        && match prefix {
            b'<' => cfg!(target_endian = "big"),
            b'>' | b'!' => cfg!(target_endian = "little"),
            _ => false,
        };
    Ok((
        dtype,
        if swapped {
            ByteOrder::Swapped
        } else {
            ByteOrder::Native
        },
    ))
}

/// Owns one export at a stable address: exporters may point shape/strides into
/// the `Py_buffer` itself (`array.array` does). Releases it with the GIL held.
struct PyBuf<'py> {
    raw: Box<ffi::Py_buffer>,
    _py: Python<'py>,
}

impl<'py> PyBuf<'py> {
    fn get(obj: &Bound<'py, PyAny>) -> PyResult<Self> {
        Self::get_flags(obj, ffi::PyBUF_STRIDES | ffi::PyBUF_FORMAT)
    }

    fn get_flags(obj: &Bound<'py, PyAny>, flags: i32) -> PyResult<Self> {
        // SAFETY: Py_buffer is a C struct whose zeroed state is valid for acquisition.
        let mut raw: Box<ffi::Py_buffer> = Box::new(unsafe { std::mem::zeroed() });
        // SAFETY: obj is alive with the GIL held; raw is writable for the call.
        if unsafe { ffi::PyObject_GetBuffer(obj.as_ptr(), &mut *raw, flags) } != 0 {
            let err = PyErr::fetch(obj.py());
            // NumPy dtypes without a PEP 3118 representation (e.g. datetime)
            // can reject acquisition with ValueError or BufferError.
            return Err(
                if err.is_instance_of::<PyValueError>(obj.py())
                    || err.is_instance_of::<PyBufferError>(obj.py())
                {
                    PyTypeError::new_err(format!("unsupported numeric buffer: {err}"))
                } else {
                    err
                },
            );
        }
        Ok(Self { raw, _py: obj.py() })
    }

    fn dtype(&self) -> PyResult<(DType, ByteOrder)> {
        if self.raw.format.is_null() || self.raw.itemsize <= 0 {
            return Err(PyTypeError::new_err(
                "buffer must have a numeric format and itemsize",
            ));
        }
        // SAFETY: a successful PyBUF_FORMAT export owns a NUL-terminated format
        // string for the lifetime of this guard.
        dtype(
            unsafe { CStr::from_ptr(self.raw.format) }.to_bytes(),
            self.raw.itemsize as usize,
        )
    }

    fn writable(&self, layout: &Layout) -> PyResult<()> {
        if self.raw.readonly != 0 {
            return Err(PyValueError::new_err("buffer is read-only"));
        }
        layout.non_overlapping(layout.itemsize)?;
        Ok(())
    }
}

impl Drop for PyBuf<'_> {
    fn drop(&mut self) {
        // SAFETY: acquisition succeeded, the GIL remains held, and this is the
        // unique release of this export (also on metadata-validation errors).
        unsafe { ffi::PyBuffer_Release(&mut *self.raw) };
    }
}

#[derive(PartialEq, Eq)]
pub(crate) struct Layout {
    itemsize: usize,
    pub(crate) shape: Vec<usize>,
    strides: Vec<isize>,
    axis: Option<usize>,
    pub(crate) total_len: usize,
    pub(crate) lane_len: usize,
    pub(crate) num_lanes: usize,
}

impl Layout {
    fn new(buf: &PyBuf<'_>, axis: Axis) -> PyResult<Self> {
        let raw = &buf.raw;
        let ndim =
            usize::try_from(raw.ndim).map_err(|_| PyTypeError::new_err("invalid buffer ndim"))?;
        if ndim > 0 && (raw.shape.is_null() || raw.strides.is_null()) {
            return Err(PyTypeError::new_err("buffer must expose shape and strides"));
        }
        let (mut shape, mut strides) = (Vec::with_capacity(ndim), Vec::with_capacity(ndim));
        for d in 0..ndim {
            // SAFETY: a successful ND/STRIDES export has ndim metadata entries;
            // the guard keeps those arrays alive and ndim > 0 implies non-null.
            let (len, stride) = unsafe { (*raw.shape.add(d), *raw.strides.add(d)) };
            shape.push(
                usize::try_from(len)
                    .map_err(|_| PyTypeError::new_err("negative buffer dimension"))?,
            );
            strides.push(stride);
            if !raw.suboffsets.is_null() {
                // SAFETY: non-null suboffsets has ndim entries under the buffer contract.
                if unsafe { *raw.suboffsets.add(d) } >= 0 {
                    return Err(PyTypeError::new_err("indirect buffers are unsupported"));
                }
            }
        }
        let itemsize = usize::try_from(raw.itemsize)
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| PyTypeError::new_err("invalid buffer itemsize"))?;
        let layout = Self::from_shape(shape, strides, itemsize, axis)?;
        if layout.total_len.checked_mul(itemsize) != usize::try_from(raw.len).ok() {
            return Err(PyTypeError::new_err("buffer length does not match shape"));
        }
        if layout.total_len > 0 && raw.buf.is_null() {
            return Err(PyValueError::new_err("invalid buffer address"));
        }
        Ok(layout)
    }

    pub(crate) fn from_shape(
        shape: Vec<usize>,
        strides: Vec<isize>,
        itemsize: usize,
        axis: Axis,
    ) -> PyResult<Self> {
        let ndim = shape.len();
        if strides.len() != ndim {
            return Err(PyTypeError::new_err("strides do not match shape"));
        }
        let axis = match axis {
            Axis::None => None,
            Axis::Index(index) => {
                let index = if index < 0 {
                    ndim as i128 + index as i128
                } else {
                    index as i128
                };
                if index < 0 || index >= ndim as i128 {
                    return Err(PyValueError::new_err("axis is out of bounds"));
                }
                Some(index as usize)
            }
        };
        let total_len = if shape.contains(&0) {
            0
        } else {
            shape
                .iter()
                .try_fold(1usize, |n, &len| n.checked_mul(len))
                .ok_or_else(|| PyValueError::new_err("buffer is too large"))?
        };
        let lane_len = axis.map_or(total_len, |ax| shape[ax]);
        let num_lanes = match axis {
            None => 1,
            Some(ax) => shape
                .iter()
                .enumerate()
                .filter(|(d, _)| *d != ax)
                .try_fold(1usize, |n, (_, &len)| n.checked_mul(len))
                .ok_or_else(|| PyValueError::new_err("too many lanes"))?,
        };
        // Bound all offset arithmetic before any traversal; empty buffers never
        // need data pointers, even when their unused strides are extreme.
        if total_len > 0 {
            let span = shape
                .iter()
                .zip(&strides)
                .try_fold(itemsize, |n, (&len, &stride)| {
                    (len - 1)
                        .checked_mul(stride.unsigned_abs())
                        .and_then(|x| n.checked_add(x))
                });
            if span.is_none_or(|n| n > isize::MAX as usize) {
                return Err(PyValueError::new_err("invalid buffer address span"));
            }
        }
        Ok(Self {
            itemsize,
            shape,
            strides,
            axis,
            total_len,
            lane_len,
            num_lanes,
        })
    }

    pub(crate) fn out_shape(&self) -> Vec<usize> {
        if self.axis.is_none() {
            vec![self.total_len]
        } else {
            self.shape.clone()
        }
    }

    pub(crate) fn non_overlapping(&self, itemsize: usize) -> PyResult<()> {
        // Empty layouts have no element pairs that could overlap.
        if self.total_len == 0 {
            return Ok(());
        }
        let mut active: Vec<_> = self
            .shape
            .iter()
            .zip(&self.strides)
            .filter(|(len, _)| **len > 1)
            .map(|(&len, &stride)| (stride.unsigned_abs(), len))
            .collect();
        active.sort_unstable();
        if active.iter().any(|&(stride, _)| stride == 0) {
            return Err(PyValueError::new_err("buffer has overlapping elements"));
        }
        let overlap = || PyValueError::new_err("buffer has overlapping elements");
        if active.is_empty() {
            return Ok(());
        }
        if active[0].0 < itemsize {
            return Err(overlap());
        }
        // Independent outer axes cannot introduce collisions within the prefix.
        let mut span = itemsize as u128
            + active
                .iter()
                .map(|&(s, n)| (n - 1) as u128 * s as u128)
                .sum::<u128>();
        while active.len() > 1 {
            let &(stride, len) = active.last().unwrap();
            let prefix_span = span - (len - 1) as u128 * stride as u128;
            if (stride as u128) < prefix_span {
                break;
            }
            active.pop();
            span = prefix_span;
        }
        if active.len() <= 1 {
            return Ok(());
        }
        // Pigeonhole bound: disjoint elements need at least active_len * itemsize bytes.
        let active_len = active
            .iter()
            .try_fold(1u128, |n, &(_, len)| n.checked_mul(len as u128));
        if active_len.is_none_or(|n| {
            n.checked_mul(itemsize as u128)
                .is_none_or(|need| span < need)
        }) {
            return Err(overlap());
        }
        if let &[(s0, n0), (s1, n1)] = active.as_slice() {
            let (mut a, mut b) = (s0, s1);
            while b != 0 {
                (a, b) = (b, a % b);
            }
            if a >= itemsize {
                return if s1 / a >= n0 || s0 / a >= n1 {
                    Ok(())
                } else {
                    Err(overlap())
                };
            }
        }
        let pivot = active
            .iter()
            .enumerate()
            .max_by_key(|(_, (_, n))| n)
            .unwrap()
            .0;
        let (stride, len) = active.remove(pivot);
        let rest = active
            .iter()
            .try_fold(1usize, |n, &(_, len)| n.checked_mul(len))
            .ok_or_else(overlap)?;
        let capacity = rest
            .checked_mul(2)
            .ok_or_else(|| PyMemoryError::new_err("layout overlap check exceeds memory"))?;
        let mut points: Vec<(usize, isize)> = Vec::new();
        points
            .try_reserve_exact(capacity)
            .map_err(|_| PyMemoryError::new_err("layout overlap check exceeds memory"))?;
        // from_shape bounds span <= isize::MAX, so offset, rem + stride, and q - 1 fit.
        for mut index in 0..rest {
            let mut offset = 0usize;
            for &(s, n) in &active {
                offset += (index % n) * s;
                index /= n;
            }
            let (rem, q) = (offset % stride, (offset / stride) as isize);
            points.push((rem, q));
            points.push((rem + stride, q - 1));
        }
        points.sort_unstable();
        // Each base offset spans len consecutive periods. Wrapped copies expose
        // byte collisions across period boundaries without enumerating the pivot.
        let mut window = BTreeSet::new();
        let mut left = 0;
        for (i, &(rem, q)) in points.iter().enumerate() {
            while rem - points[left].0 >= itemsize {
                window.remove(&(points[left].1, left));
                left += 1;
            }
            let before = window.range(..=(q, usize::MAX)).next_back();
            let after = window.range((q, 0)..).next();
            if before
                .into_iter()
                .chain(after)
                .any(|&(other, _)| q.abs_diff(other) < len)
            {
                return Err(overlap());
            }
            window.insert((q, i));
        }
        Ok(())
    }

    pub(crate) fn coordinates(&self, mut lane: usize, mut index: usize) -> Vec<usize> {
        let mut coordinates = vec![0; self.shape.len()];
        for d in (0..self.shape.len()).rev() {
            coordinates[d] = if self.axis == Some(d) {
                index
            } else if self.axis.is_some() {
                let value = lane % self.shape[d];
                lane /= self.shape[d];
                value
            } else {
                let value = index % self.shape[d];
                index /= self.shape[d];
                value
            };
        }
        coordinates
    }

    // A lane index enumerates all coordinates except the selected axis in C-order.
    fn offset(&self, mut lane: usize, index: usize) -> isize {
        if let Some(ax) = self.axis {
            let mut offset = index as isize * self.strides[ax];
            for d in (0..self.shape.len()).rev().filter(|&d| d != ax) {
                offset += (lane % self.shape[d]) as isize * self.strides[d];
                lane /= self.shape[d];
            }
            offset
        } else {
            let mut index = index;
            let mut offset = 0;
            for d in (0..self.shape.len()).rev() {
                offset += (index % self.shape[d]) as isize * self.strides[d];
                index /= self.shape[d];
            }
            offset
        }
    }

    fn address_range(&self, buf: &PyBuf<'_>) -> PyResult<std::ops::Range<usize>> {
        let (mut low, mut high) = (0isize, 0isize);
        for (&len, &stride) in self.shape.iter().zip(&self.strides) {
            let extent = (len - 1) as isize * stride;
            if extent < 0 {
                low += extent;
            } else {
                high += extent;
            }
        }
        let address = buf.raw.buf as usize;
        let start = address.checked_add_signed(low);
        let end = address
            .checked_add_signed(high)
            .and_then(|n| n.checked_add(buf.raw.itemsize as usize));
        match (start, end) {
            (Some(start), Some(end)) => Ok(start..end),
            _ => Err(PyValueError::new_err("invalid buffer address range")),
        }
    }

    fn direct<T>(&self, data: Data, order: ByteOrder) -> bool {
        if order != ByteOrder::Native || !(data.0 as usize).is_multiple_of(align_of::<T>()) {
            return false;
        }
        match self.axis {
            Some(ax) => {
                self.strides[ax] == size_of::<T>() as isize
                    && self
                        .shape
                        .iter()
                        .zip(&self.strides)
                        .all(|(&len, &stride)| len <= 1 || stride % align_of::<T>() as isize == 0)
            }
            None => {
                let mut stride = size_of::<T>();
                for (&len, &actual) in self.shape.iter().zip(&self.strides).rev() {
                    if len > 1 && actual != stride as isize {
                        return false;
                    }
                    stride *= len;
                }
                true
            }
        }
    }
}

/// Validates dtype, axis and ranks before an adapter allocates its result.
pub(crate) fn validate_output(
    obj: &Bound<'_, PyAny>,
    request: Request<'_>,
) -> PyResult<Vec<usize>> {
    let buf = PyBuf::get(obj)?;
    buf.dtype()?;
    let layout = Layout::new(&buf, request.axis)?;
    if let Some(raw) = request.kth {
        normalize_kth(raw, layout.lane_len)?;
    }
    Ok(layout.out_shape())
}

/// Checks fresh-copy storage before an adapter sorts or fills it.
pub(crate) fn check_copy(
    input: &Bound<'_, PyAny>,
    output: &Bound<'_, PyAny>,
    axis: Axis,
) -> PyResult<()> {
    if input.is(output) {
        return Err(PyValueError::new_err("output buffer aliases the input"));
    }
    let source = PyBuf::get_flags(input, ffi::PyBUF_STRIDES)?;
    let target = PyBuf::get_flags(output, ffi::PyBUF_STRIDES)?;
    let layout = Layout::new(&source, axis)?;
    let out = Layout::new(
        &target,
        if axis == Axis::None {
            Axis::Index(0)
        } else {
            axis
        },
    )?;
    if layout.out_shape() != out.shape || source.raw.itemsize != target.raw.itemsize {
        return Err(PyTypeError::new_err("output buffer does not match request"));
    }
    target.writable(&out)?;
    if layout.total_len > 0 && out.total_len > 0 {
        let src = layout.address_range(&source)?;
        let dst = out.address_range(&target)?;
        if src.start < dst.end && dst.start < src.end {
            return Err(PyValueError::new_err("output buffer aliases the input"));
        }
    }
    Ok(())
}

/// Fail closed on pointer codes, unknown extensions and malformed struct syntax.
fn is_pod_format(format: &[u8]) -> bool {
    fn fields(format: &[u8], pos: &mut usize, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        let mut any = false;
        while *pos < format.len() && format[*pos] != b'}' {
            if matches!(format[*pos], b'@' | b'=' | b'<' | b'>' | b'!') {
                *pos += 1;
            }
            while format.get(*pos).is_some_and(u8::is_ascii_digit) {
                *pos += 1;
            }
            match format.get(*pos) {
                Some(b'T') if format.get(*pos + 1) == Some(&b'{') => {
                    *pos += 2;
                    if !fields(format, pos, depth + 1) || format.get(*pos) != Some(&b'}') {
                        return false;
                    }
                    *pos += 1;
                }
                Some(code) if b"bBhHiIlLqQnNefd?cxspuw".contains(code) => {
                    *pos += 1;
                }
                _ => return false,
            }
            any = true;
            if format.get(*pos) == Some(&b':') {
                if depth == 0 {
                    return false;
                }
                *pos += 1;
                while format.get(*pos).is_some_and(|&b| b != b':') {
                    *pos += 1;
                }
                if format.get(*pos) != Some(&b':') {
                    return false;
                }
                *pos += 1;
            }
        }
        any
    }
    let mut pos = 0;
    fields(format, &mut pos, 0) && pos == format.len()
}

fn pod_format<'a>(buf: &'a PyBuf<'_>) -> PyResult<&'a [u8]> {
    if !buf.raw.format.is_null() {
        // SAFETY: PyBUF_FORMAT keeps the NUL-terminated format alive until release.
        let format = unsafe { CStr::from_ptr(buf.raw.format) }.to_bytes();
        if is_pod_format(format) {
            return Ok(format);
        }
    }
    Err(PyTypeError::new_err("buffer format is not POD"))
}

/// Owned POD lane bytes, admitted only by the exported format. Padding stays
/// `MaybeUninit` throughout copying and is never interpreted as a Rust value.
pub(crate) struct RawSnapshot {
    layout: Layout,
    format: Vec<u8>,
    itemsize: usize,
    lanes: Vec<Vec<MaybeUninit<u8>>>,
}

impl RawSnapshot {
    pub(crate) fn get(obj: &Bound<'_, PyAny>, axis: Axis) -> PyResult<Self> {
        let buf = PyBuf::get_flags(obj, ffi::PyBUF_STRIDES | ffi::PyBUF_FORMAT)?;
        let format = pod_format(&buf)?.to_vec();
        let layout = Layout::new(&buf, axis)?;
        let itemsize = layout.itemsize;
        let lane_bytes = layout
            .lane_len
            .checked_mul(itemsize)
            .ok_or_else(|| PyValueError::new_err("lane is too large"))?;
        let mut lanes = Vec::with_capacity(layout.num_lanes);
        for lane in 0..layout.num_lanes {
            let mut bytes = vec![MaybeUninit::uninit(); lane_bytes];
            if itemsize > 0 {
                for i in 0..layout.lane_len {
                    // SAFETY: the export pins each validated source element;
                    // the owned byte slice has itemsize bytes per element.
                    // MaybeUninit preserves possibly uninitialized dtype padding.
                    unsafe {
                        ptr::copy_nonoverlapping(
                            Data(buf.raw.buf.cast()).at(&layout, lane, i),
                            bytes.as_mut_ptr().cast::<u8>().add(i * itemsize),
                            itemsize,
                        );
                    }
                }
            }
            lanes.push(bytes);
        }
        Ok(Self {
            layout,
            format,
            itemsize,
            lanes,
        })
    }

    /// Only one scalar/string token has no implicit alignment or struct padding.
    pub(crate) fn byte_comparable(&self) -> bool {
        let mut format = self.format.as_slice();
        if matches!(format.first(), Some(b'@' | b'=' | b'<' | b'>' | b'!')) {
            format = &format[1..];
        }
        while format.first().is_some_and(u8::is_ascii_digit) {
            format = &format[1..];
        }
        format.len() == 1 && format[0] != b'x'
    }

    /// Checks an initialized scalar buffer after callbacks, without retaining an export.
    pub(crate) fn matches(&self, obj: &Bound<'_, PyAny>, axis: Axis) -> PyResult<bool> {
        let buf = PyBuf::get_flags(obj, ffi::PyBUF_STRIDES | ffi::PyBUF_FORMAT)?;
        let format = pod_format(&buf)?;
        let layout = Layout::new(&buf, axis)?;
        if layout != self.layout || format != self.format {
            return Ok(false);
        }
        if !self.byte_comparable() {
            return Err(PyTypeError::new_err(
                "buffer may contain uninitialized padding",
            ));
        }
        for (lane, bytes) in self.lanes.iter().enumerate() {
            for i in 0..layout.lane_len {
                // SAFETY: the scalar format excludes padding; both the live
                // pinned element and its snapshot contain initialized bytes.
                let (live, saved) = unsafe {
                    (
                        std::slice::from_raw_parts(
                            Data(buf.raw.buf.cast()).at(&layout, lane, i),
                            self.itemsize,
                        ),
                        std::slice::from_raw_parts(
                            bytes.as_ptr().cast::<u8>().add(i * self.itemsize),
                            self.itemsize,
                        ),
                    )
                };
                if live != saved {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn write(
        &self,
        obj: &Bound<'_, PyAny>,
        axis: Axis,
        indices: &[Vec<usize>],
    ) -> PyResult<()> {
        let buf = PyBuf::get_flags(obj, ffi::PyBUF_STRIDES | ffi::PyBUF_FORMAT)?;
        pod_format(&buf)?;
        let layout = Layout::new(&buf, axis)?;
        buf.writable(&layout)?;
        if layout.itemsize != self.itemsize
            || layout.num_lanes != self.lanes.len()
            || indices.len() != self.lanes.len()
        {
            return Err(PyValueError::new_err("output layout changed during sort"));
        }
        // Validate every lane before the first write.
        for (bytes, order) in self.lanes.iter().zip(indices) {
            if order.len() != layout.lane_len
                || bytes.len() != order.len() * self.itemsize
                || order.iter().any(|&i| i >= order.len())
            {
                return Err(PyValueError::new_err("output layout changed during sort"));
            }
        }
        if self.itemsize > 0 {
            for (lane, order) in indices.iter().enumerate() {
                for (i, &source) in order.iter().enumerate() {
                    // SAFETY: all indices and byte lengths were validated; owned
                    // snapshots cannot alias the pinned, writable destination.
                    unsafe {
                        ptr::copy_nonoverlapping(
                            self.lanes[lane]
                                .as_ptr()
                                .cast::<u8>()
                                .add(source * self.itemsize),
                            Data(buf.raw.buf.cast()).at(&layout, lane, i),
                            self.itemsize,
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn write_indices(
    obj: &Bound<'_, PyAny>,
    axis: Axis,
    indices: &[Vec<usize>],
) -> PyResult<()> {
    let buf = PyBuf::get(obj)?;
    let (dtype, order) = buf.dtype()?;
    let layout = Layout::new(&buf, axis)?;
    buf.writable(&layout)?;
    let expected = if size_of::<isize>() == 8 {
        DType::I64
    } else {
        DType::I32
    };
    if dtype != expected
        || order != ByteOrder::Native
        || indices.len() != layout.num_lanes
        || indices.iter().any(|lane| lane.len() != layout.lane_len)
    {
        return Err(PyTypeError::new_err("output buffer does not match indices"));
    }
    for (lane, values) in indices.iter().enumerate() {
        for (i, &value) in values.iter().enumerate() {
            // SAFETY: validated writable coordinates each contain an intp slot;
            // lane-relative indices fit isize under the validated address span.
            unsafe {
                ptr::write_unaligned(
                    Data(buf.raw.buf.cast())
                        .at(&layout, lane, i)
                        .cast::<isize>(),
                    value as isize,
                );
            }
        }
    }
    Ok(())
}

// Validated numeric kernels and POD snapshots construct these handles. Reads permit overlap;
// write handles refer to proven disjoint lanes in a writable export or owned Vec.
#[derive(Clone, Copy)]
struct Data(*mut u8);
// SAFETY: exports/Vec storage outlive all joined work. The caller holds the GIL
// during external reads/writes; detached work contains only owned scratch.
unsafe impl Send for Data {}
// SAFETY: execute validates the disjoint-lane invariant for every write target;
// concurrent input access is read-only unless it is the same exclusive lane.
unsafe impl Sync for Data {}

impl Data {
    unsafe fn at(self, layout: &Layout, lane: usize, index: usize) -> *mut u8 {
        // SAFETY: caller supplies valid coordinates in a nonempty validated layout;
        // the exporter guarantees its strided elements belong to live storage.
        unsafe { self.0.offset(layout.offset(lane, index)) }
    }
}

trait Element: NumericElem + Copy {
    fn swap(self) -> Self;
}
macro_rules! elements {
    ($($ty:ty),*) => {$(impl Element for $ty { fn swap(self) -> Self { self.swap_bytes() } })*};
}
elements!(i8, i16, i32, i64, u8, u16, u32, u64);
impl Element for f32 {
    fn swap(self) -> Self {
        Self::from_bits(self.to_bits().swap_bytes())
    }
}
impl Element for f64 {
    fn swap(self) -> Self {
        Self::from_bits(self.to_bits().swap_bytes())
    }
}

fn order_values<T: NumericElem>(
    values: &mut [T],
    ranks: Option<&[usize]>,
    descending: bool,
    parallel: bool,
) {
    if let Some(ranks) = ranks {
        partition_slice(values, ranks, descending, parallel);
    } else {
        sort_slice(values, descending, parallel);
    }
}

fn order_indices<T: NumericElem>(
    values: &[T],
    ranks: Option<&[usize]>,
    descending: bool,
    parallel: bool,
) -> Vec<usize> {
    if let Some(ranks) = ranks {
        argpartition_slice(values, ranks, descending, parallel)
    } else {
        argsort_slice(values, descending, parallel)
    }
}

pub(crate) fn inner_parallel(threaded: bool, num_lanes: usize, lane_len: usize) -> bool {
    threaded && (num_lanes == 1 || (num_lanes < rayon::current_num_threads() && lane_len >= 16_384))
}

fn pool_error(err: rayon::ThreadPoolBuildError) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// Executes a validated numeric request. A missing destination means either
/// in-place data or a returned Vec of indices; supplied outputs are fresh copies.
pub(crate) fn execute(
    input: &Bound<'_, PyAny>,
    output: Option<&Bound<'_, PyAny>>,
    request: Request<'_>,
) -> PyResult<Vec<usize>> {
    let source = PyBuf::get(input)?;
    let (dtype, byte_order) = source.dtype()?;
    let layout = Layout::new(&source, request.axis)?;
    let ranks = request
        .kth
        .map(|raw| normalize_kth(raw, layout.lane_len))
        .transpose()?;
    if request.output == Output::Inplace {
        source.writable(&layout)?;
    }
    let destination = output.map(PyBuf::get).transpose()?;
    let output_layout = destination
        .as_ref()
        .map(|buf| {
            let output_axis = if layout.axis.is_none() {
                Axis::Index(0)
            } else {
                request.axis
            };
            let out = Layout::new(buf, output_axis)?;
            let (out_dtype, out_order) = buf.dtype()?;
            let valid_dtype = if request.output == Output::Indices {
                out_order == ByteOrder::Native
                    && match size_of::<isize>() {
                        8 => out_dtype == DType::I64,
                        _ => out_dtype == DType::I32,
                    }
            } else {
                out_dtype == dtype && out_order == byte_order
            };
            if !valid_dtype || out.shape != layout.out_shape() {
                return Err(PyTypeError::new_err("output buffer does not match request"));
            }
            buf.writable(&out)?;
            Ok(out)
        })
        .transpose()?;
    let mut indices = if request.output == Output::Indices && output.is_none() {
        vec![0usize; layout.total_len]
    } else {
        Vec::new()
    };
    // Validate every contract above, including empty inputs, before addressing data.
    if layout.total_len == 0 || layout.lane_len == 0 {
        return Ok(indices);
    }
    if let (Some(destination), Some(out)) = (&destination, &output_layout) {
        // Adapters request fresh allocation, but subclass methods or a replaced
        // numpy.empty can return aliases. Reject them before creating Rust slices.
        let source_range = layout.address_range(&source)?;
        let output_range = out.address_range(destination)?;
        if source_range.start < output_range.end && output_range.start < source_range.end {
            return Err(PyValueError::new_err("output buffer aliases the input"));
        }
    }
    let source_data = Data(source.raw.buf.cast());
    let target = if let Some(buf) = &destination {
        Data(buf.raw.buf.cast())
    } else if request.output == Output::Indices {
        Data(indices.as_mut_ptr().cast())
    } else {
        source_data
    };
    let index_layout = (request.output == Output::Indices && output.is_none()).then(|| Layout {
        itemsize: size_of::<usize>(),
        shape: vec![layout.total_len],
        strides: vec![size_of::<usize>() as isize],
        axis: Some(0),
        total_len: layout.total_len,
        lane_len: layout.total_len,
        num_lanes: 1,
    });
    let target_layout = output_layout
        .as_ref()
        .or(index_layout.as_ref())
        .unwrap_or(&layout);
    macro_rules! dispatch {
        ($ty:ty) => {
            execute_typed::<$ty>(
                input.py(),
                source_data,
                target,
                &layout,
                target_layout,
                byte_order,
                ranks.as_deref(),
                request,
            )?
        };
    }
    match dtype {
        DType::I8 => dispatch!(i8),
        DType::I16 => dispatch!(i16),
        DType::I32 => dispatch!(i32),
        DType::I64 => dispatch!(i64),
        DType::U8 => dispatch!(u8),
        DType::U16 => dispatch!(u16),
        DType::U32 => dispatch!(u32),
        DType::U64 => dispatch!(u64),
        DType::F32 => dispatch!(f32),
        DType::F64 => dispatch!(f64),
    }
    Ok(indices)
}

#[allow(clippy::too_many_arguments)]
fn execute_typed<T: Element>(
    py: Python<'_>,
    source: Data,
    target: Data,
    layout: &Layout,
    out: &Layout,
    byte_order: ByteOrder,
    ranks: Option<&[usize]>,
    request: Request<'_>,
) -> PyResult<()> {
    let threaded = request.threads != Threads::Serial;
    let direct = layout.direct::<T>(source, byte_order)
        && (request.output != Output::Copy || out.direct::<T>(target, byte_order));
    // SAFETY: every target has been validated writable and nonoverlapping. Each
    // invocation writes one distinct lane; index values fit isize (buffer bounds).
    let write_indices = |lane, indices: &[usize]| {
        for (i, &index) in indices.iter().enumerate() {
            // SAFETY: out enumerates this lane's distinct live output elements;
            // np.intp and usize have identical width, and indices are nonnegative.
            unsafe {
                ptr::write_unaligned(target.at(out, lane, i).cast::<isize>(), index as isize)
            };
        }
    };
    if direct {
        request
            .threads
            .run(|| {
                let parallel = inner_parallel(threaded, layout.num_lanes, layout.lane_len);
                let work = |lane| {
                    if request.output == Output::Indices {
                        // SAFETY: direct() proves native alignment and contiguous lane
                        // storage, the export pins it, and no kernel writes the input.
                        let values = unsafe {
                            std::slice::from_raw_parts(
                                source.at(layout, lane, 0).cast::<T>(),
                                layout.lane_len,
                            )
                        };
                        write_indices(
                            lane,
                            &order_indices(values, ranks, request.descending, parallel),
                        );
                    } else {
                        if request.output == Output::Copy {
                            // SAFETY: both lanes are contiguous, equally sized and
                            // disjoint; execute() rejected source/output aliasing.
                            unsafe {
                                ptr::copy_nonoverlapping(
                                    source.at(layout, lane, 0).cast::<T>(),
                                    target.at(out, lane, 0).cast::<T>(),
                                    layout.lane_len,
                                );
                            }
                        }
                        // SAFETY: direct() proves native alignment/contiguity; the
                        // overlap check proves this lane is exclusive across workers.
                        let values = unsafe {
                            std::slice::from_raw_parts_mut(
                                target.at(out, lane, 0).cast::<T>(),
                                layout.lane_len,
                            )
                        };
                        order_values(values, ranks, request.descending, parallel);
                    }
                };
                if threaded && layout.num_lanes > 1 {
                    (0..layout.num_lanes).into_par_iter().for_each(work);
                } else {
                    (0..layout.num_lanes).for_each(work);
                }
            })
            .map_err(pool_error)?;
        return Ok(());
    }
    // Gather under the GIL, detach only owned scratch, then scatter under the GIL.
    // Batching bounds scratch to one lane per worker instead of a full array copy.
    let width = if threaded {
        request
            .threads
            .run(rayon::current_num_threads)
            .map_err(pool_error)?
    } else {
        1
    };
    for start in (0..layout.num_lanes).step_by(width) {
        let end = (start + width).min(layout.num_lanes);
        let mut lanes: Vec<Vec<T>> = (start..end)
            .map(|lane| {
                (0..layout.lane_len)
                    .map(|i| {
                        // SAFETY: nonempty validated coordinates point at an initialized T;
                        // read_unaligned handles arbitrary byte alignment, including views.
                        let value =
                            unsafe { ptr::read_unaligned(source.at(layout, lane, i).cast::<T>()) };
                        if byte_order == ByteOrder::Swapped {
                            value.swap()
                        } else {
                            value
                        }
                    })
                    .collect()
            })
            .collect();
        let ordered_indices = py
            .detach(|| {
                request.threads.run(|| {
                    let parallel = inner_parallel(threaded, layout.num_lanes, layout.lane_len);
                    let work = |values: &mut Vec<T>| {
                        if request.output == Output::Indices {
                            order_indices(values, ranks, request.descending, parallel)
                        } else {
                            order_values(values, ranks, request.descending, parallel);
                            Vec::new()
                        }
                    };
                    if threaded && layout.num_lanes > 1 {
                        lanes.par_iter_mut().map(work).collect::<Vec<_>>()
                    } else {
                        lanes.iter_mut().map(work).collect()
                    }
                })
            })
            .map_err(pool_error)?;
        for (offset, values) in lanes.into_iter().enumerate() {
            let lane = start + offset;
            if request.output == Output::Indices {
                write_indices(lane, &ordered_indices[offset]);
            } else {
                for (i, value) in values.into_iter().enumerate() {
                    let value = if byte_order == ByteOrder::Swapped {
                        value.swap()
                    } else {
                        value
                    };
                    // SAFETY: target is writable and each coordinate is distinct;
                    // write_unaligned supports strided and unaligned output storage.
                    unsafe { ptr::write_unaligned(target.at(out, lane, i).cast::<T>(), value) };
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaved_layout_overlap() {
        let cases = [
            (vec![2, 65_536], vec![12, 8], true),
            (vec![3, 65_536], vec![12, 8], false),
            (vec![2, 65_536], vec![10, 8], false),
            (vec![2, 2], vec![14, 8], true),
            (vec![2, 3], vec![14, 8], false),
            (vec![2, 65_536], vec![-12, 8], true),
            (vec![3, 65_536], vec![12, -8], false),
            (vec![2, 2], vec![-14, -8], true),
            (vec![2, 3], vec![-14, 8], false),
            (vec![3, 2, 65_536], vec![600_000, 12, 8], true),
            (vec![3, 3, 65_536], vec![-600_000, 12, -8], false),
            (vec![2, 2, 65_536], vec![12, 16, 32], true),
            (vec![2, 2, 65_536], vec![14, 8, 32], true),
            (vec![2, 2, 65_536], vec![14, 8, 24], false),
            (vec![2, 2, 65_536], vec![12, 16, 24], true),
            (vec![2, 2, 65_536], vec![-12, 16, -26], false),
            (vec![2, 2, 65_536], vec![12, 16, 28], false),
            (vec![0, 65_536], vec![0, 0], true),
            (vec![1, 1], vec![0, 0], true),
            (vec![2, 1], vec![0, 0], false),
            (vec![2, 2], vec![2, 8], false),
            (vec![256, 257, 258], vec![4, 6, 8], false),
            (vec![257, 257, 257], vec![1028, 1032, 527_356], true),
            (vec![257, 257, 257], vec![1028, 1032, 527_358], false),
            (vec![257, 257, 257], vec![1028, 1032, 527_360], false),
        ];
        for (shape, strides, valid) in cases {
            let layout = Layout::from_shape(shape, strides, 4, Axis::Index(-1)).unwrap();
            assert_eq!(
                layout.non_overlapping(4).is_ok(),
                valid,
                "{:?} {:?}",
                layout.shape,
                layout.strides
            );
        }
    }

    #[test]
    fn modular_overlap_matches_element_ranges() {
        // Small exhaustive layouts exercise partial-byte overlap and wraparound.
        for n0 in 2..=4 {
            for n1 in 2..=4 {
                for n2 in 1..=3 {
                    for s0 in 1..=16 {
                        for s1 in 1..=16 {
                            for s2 in [4, 9, 20] {
                                let layout = Layout::from_shape(
                                    vec![n0, n1, n2],
                                    vec![s0, -s1, s2],
                                    4,
                                    Axis::None,
                                )
                                .unwrap();
                                let mut offsets: Vec<_> =
                                    (0..layout.total_len).map(|i| layout.offset(0, i)).collect();
                                offsets.sort_unstable();
                                let expected = offsets.windows(2).all(|p| p[1] - p[0] >= 4);
                                assert_eq!(
                                    layout.non_overlapping(4).is_ok(),
                                    expected,
                                    "{:?} {:?}",
                                    layout.shape,
                                    layout.strides
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pod_formats_fail_closed() {
        for format in [
            "b",
            ">8s",
            "@i",
            "T{i:score:4xT{d:value:}:nested:}",
            "2T{?b:tag:}",
            "u",
            "w",
        ] {
            assert!(is_pod_format(format.as_bytes()), "{format}");
        }
        for format in [
            "",
            "O",
            "P",
            "T{i:score:O:object:}",
            "T{T{P:ptr:}:inner:}",
            "Zf",
            "g",
            "T{i",
            "T{}",
            "3",
            "@",
            "i}",
            "i:name:",
            "T{i:unterminated}",
        ] {
            assert!(!is_pod_format(format.as_bytes()), "{format}");
        }
    }

    #[test]
    fn formats_use_signedness_and_actual_width() {
        for code in b"bhilqn" {
            for (width, expected) in [
                (1, DType::I8),
                (2, DType::I16),
                (4, DType::I32),
                (8, DType::I64),
            ] {
                assert_eq!(dtype(&[*code], width).unwrap().0, expected);
            }
        }
        for code in b"BHILQN" {
            for (width, expected) in [
                (1, DType::U8),
                (2, DType::U16),
                (4, DType::U32),
                (8, DType::U64),
            ] {
                assert_eq!(dtype(&[*code], width).unwrap().0, expected);
            }
        }
        for format in [
            b"?".as_slice(),
            b"e",
            b"g",
            b"u",
            b"w",
            b"O",
            b"Zf",
            b"T{i:x:}",
            b"2i",
            b"",
        ] {
            assert!(dtype(format, 4).is_err());
        }
        assert!(dtype(b"f", 8).is_err());
        assert!(dtype(b"d", 4).is_err());
        assert_eq!(
            dtype(b"<d", 8).unwrap().1,
            if cfg!(target_endian = "little") {
                ByteOrder::Native
            } else {
                ByteOrder::Swapped
            }
        );
        assert_eq!(
            dtype(b">d", 8).unwrap().1,
            if cfg!(target_endian = "big") {
                ByteOrder::Native
            } else {
                ByteOrder::Swapped
            }
        );
    }
}
