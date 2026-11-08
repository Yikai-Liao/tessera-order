// SPDX-License-Identifier: BSD-2-Clause

//! Native sort entry points, record layouts and runtime dispatch.

use super::variant::{
    BenchmarkRecord, Kernel, PreparedRecords, Variant, WideRecord, prepare_kernel,
};
use crate::case::Row;
use serde_json::{Value, json};
use std::{
    ffi::{CStr, c_char, c_void},
    mem::{align_of, offset_of},
};

/// A native entry point that sorts one complete record buffer.
type Sort = unsafe extern "C" fn(*mut c_void, usize);
unsafe extern "C" {
    /// Looks up a native algorithm and record representation.
    fn bench_kernel(algorithm: u32, ty: u32) -> Option<Sort>;
    /// Reads a native record size, alignment or payload offset.
    fn bench_layout(ty: u32, field: u32) -> usize;
    /// Returns `VQSort`'s runtime instruction-set target.
    fn bench_vqsort_target() -> *const c_char;
    /// Returns x86-simd-sort's runtime instruction-set target.
    fn bench_x86simdsort_target() -> *const c_char;
}
/// Creates a kernel when the native registry supports this record representation.
pub(super) fn prepare<T: BenchmarkRecord>(
    v: Variant,
    input: std::rc::Rc<PreparedRecords<T>>,
    ty: u32,
) -> Option<Box<dyn Kernel>> {
    let algorithm = [
        "clausecker",
        "fluxsort",
        "ips2ra",
        "ips4o",
        "vqsort",
        "x86simdsort",
        "crumsort",
        "duckdb",
    ]
    .iter()
    .position(|&n| n == v.name)?;
    // SAFETY: The registry returns a static function pointer or NULL for these indices.
    let sort = unsafe { bench_kernel(algorithm as u32, ty) }?;
    Some(prepare_kernel(input, v.stable, move |values| {
        // SAFETY: Each call site supplies the type index matching T's repr(C) layout.
        unsafe { sort(values.as_mut_ptr().cast(), values.len()) };
    }))
}

/// Records the SIMD targets selected on the measuring host.
pub(super) fn read_dispatch_targets() -> Value {
    // SAFETY: Both functions return non-null static NUL-terminated strings.
    unsafe {
        json!({"vqsort":CStr::from_ptr(bench_vqsort_target()).to_string_lossy(), "x86simdsort":CStr::from_ptr(bench_x86simdsort_target()).to_string_lossy()})
    }
}
/// Checks the Rust and native sizes, alignments and payload offsets.
pub(super) fn check_layouts() {
    let layouts = [
        [
            size_of::<Row<u32>>(),
            align_of::<Row<u32>>(),
            offset_of!(Row<u32>, row),
        ],
        [
            size_of::<Row<u64>>(),
            align_of::<Row<u64>>(),
            offset_of!(Row<u64>, row),
        ],
        [
            size_of::<WideRecord>(),
            align_of::<WideRecord>(),
            offset_of!(WideRecord, payload),
        ],
    ];
    for (ty, fields) in layouts.iter().enumerate() {
        for (field, &expected) in fields.iter().enumerate() {
            // SAFETY: Indices are within the native layout table's dimensions.
            let actual = unsafe { bench_layout(ty as u32, field as u32) };
            assert_eq!(expected, actual, "native record layout mismatch");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::variant::CaseInput;
    use super::*;

    #[test]
    fn record_layouts_match() {
        check_layouts();
    }

    #[test]
    fn duckdb_sorts_boundaries() {
        let variant = Variant {
            name: "duckdb",
            stable: false,
            adapter: "native",
            threads: None,
        };
        // Exercise both sides of upstream small-sort and radix thresholds.
        let plan = crate::plan::Plan::parse(
            r#"
            schema = 1
            harness = "comparison"
            algorithms = ["duckdb"]
            [[include]]
            lengths = [1, 2, 23, 24, 127, 128, 129, 1023, 1024, 1025, 8192]
        "#,
            "comparison",
        )
        .unwrap();
        let mut cases = plan.inputs().unwrap();
        // The public plan requires positive lengths; the FFI also accepts empty input.
        cases.extend(crate::plan::catalog().comparison.types.iter().map(|ty| {
            crate::plan::InputCase {
                ty: ty.clone(),
                distribution: "random".into(),
                len: 0,
            }
        }));
        for case in cases {
            let input = CaseInput::new(&case);
            if let Some(mut kernel) = variant.prepare(&input, 0) {
                kernel.restore(1);
                kernel.run_batch();
                kernel
                    .check()
                    .unwrap_or_else(|error| panic!("{}: {error}", case.id()));
            } else {
                assert!(matches!(case.ty.as_str(), "f64" | "owned"));
            }
        }
    }
}
