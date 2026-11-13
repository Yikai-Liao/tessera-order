// SPDX-License-Identifier: BSD-2-Clause

//! Native sort entry points, record layouts and runtime dispatch.

use super::{
    record::{BenchmarkRecord, PreparedRecords, WideRecord},
    variant::{KernelVisitor, Variant},
};
use crate::case::Row;
use serde_json::{Value, json};
use std::{
    ffi::{CStr, c_char, c_void},
    mem::{align_of, offset_of},
};

/// Column order shared by native kernel and layout lookup.
#[repr(u32)]
pub(super) enum NativeType {
    U32,
    U64,
    F64,
    Row32,
    Row64,
    Record128,
}

/// A record representation accepted by the native registry.
///
/// # Safety
/// `Self` must have the C layout registered for `TYPE`, including its payload offset.
pub(super) unsafe trait NativeRecord: BenchmarkRecord {
    /// Selects the registry column and layout row for this record.
    const TYPE: NativeType;
    /// Locates the observable payload, or zero for a scalar.
    const PAYLOAD_OFFSET: usize = 0;
}

// SAFETY: Rust u32 and C uint32_t have the same scalar representation.
unsafe impl NativeRecord for u32 {
    const TYPE: NativeType = NativeType::U32;
}
// SAFETY: Rust u64 and C uint64_t have the same scalar representation.
unsafe impl NativeRecord for u64 {
    const TYPE: NativeType = NativeType::U64;
}
// SAFETY: Rust f64 and C double use the same IEEE 754 representation on this target.
unsafe impl NativeRecord for f64 {
    const TYPE: NativeType = NativeType::F64;
}
// SAFETY: Row<u32> is repr(C) with the fields of BenchRow32 in the same order.
unsafe impl NativeRecord for Row<u32> {
    const TYPE: NativeType = NativeType::Row32;
    const PAYLOAD_OFFSET: usize = offset_of!(Self, row);
}
// SAFETY: Row<u64> is repr(C) with the fields of BenchRow64 in the same order.
unsafe impl NativeRecord for Row<u64> {
    const TYPE: NativeType = NativeType::Row64;
    const PAYLOAD_OFFSET: usize = offset_of!(Self, row);
}
// SAFETY: WideRecord is repr(C) with the fields of BenchRecord in the same order.
unsafe impl NativeRecord for WideRecord {
    const TYPE: NativeType = NativeType::Record128;
    const PAYLOAD_OFFSET: usize = offset_of!(Self, payload);
}

/// Row order shared with the C++ registry; names are checked across the FFI.
#[derive(Clone, Copy)]
#[repr(u32)]
enum NativeAlgorithm {
    Clausecker,
    Fluxsort,
    Ips2ra,
    Ips4o,
    Vqsort,
    X86simdsort,
    Crumsort,
    Duckdb,
}
impl NativeAlgorithm {
    /// Resolves a catalog name before any timing begins.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "clausecker" => Self::Clausecker,
            "fluxsort" => Self::Fluxsort,
            "ips2ra" => Self::Ips2ra,
            "ips4o" => Self::Ips4o,
            "vqsort" => Self::Vqsort,
            "x86simdsort" => Self::X86simdsort,
            "crumsort" => Self::Crumsort,
            "duckdb" => Self::Duckdb,
            _ => return None,
        })
    }
}

/// A native entry point that sorts one complete record buffer.
type Sort = unsafe extern "C" fn(*mut c_void, usize);
unsafe extern "C" {
    /// Looks up a native algorithm and record representation.
    fn bench_kernel(algorithm: u32, ty: u32) -> Option<Sort>;
    /// Reads a static algorithm name, or NULL outside the registry.
    #[cfg(test)]
    fn bench_algorithm_name(algorithm: u32) -> *const c_char;
    /// Reads a native record size, alignment or payload offset.
    fn bench_layout(ty: u32, field: u32) -> usize;
    /// Returns `VQSort`'s runtime instruction-set target.
    fn bench_vqsort_target() -> *const c_char;
    /// Returns x86-simd-sort's runtime instruction-set target.
    fn bench_x86simdsort_target() -> *const c_char;
}
/// Creates a kernel when the native registry supports this record representation.
pub(super) fn prepare<T: NativeRecord, V: KernelVisitor>(
    v: Variant,
    input: std::rc::Rc<PreparedRecords<T>>,
    visitor: V,
) -> Option<V::Output> {
    let algorithm = NativeAlgorithm::from_name(v.name)?;
    // SAFETY: The registry returns a static function pointer or NULL for these indices.
    let sort = unsafe { bench_kernel(algorithm as u32, T::TYPE as u32) }?;
    Some(visitor.visit(input, v.stable, 0, move |values| {
        // SAFETY: NativeRecord binds T to the registry's C layout for TYPE.
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
    fn check<T: NativeRecord>() {
        for (field, expected) in [size_of::<T>(), align_of::<T>(), T::PAYLOAD_OFFSET]
            .into_iter()
            .enumerate()
        {
            // SAFETY: The registry bounds-checks both indices.
            let actual = unsafe { bench_layout(T::TYPE as u32, field as u32) };
            assert_eq!(
                expected,
                actual,
                "native layout mismatch for {}",
                std::any::type_name::<T>()
            );
        }
    }
    check::<u32>();
    check::<u64>();
    check::<f64>();
    check::<Row<u32>>();
    check::<Row<u64>>();
    check::<WideRecord>();
}
#[cfg(test)]
mod tests {
    use super::super::record::CaseInput;
    use super::*;

    #[test]
    fn record_layouts_match() {
        check_layouts();
    }

    #[test]
    fn algorithm_names_match() {
        let algorithms: Vec<_> = crate::plan::catalog()
            .algorithms
            .iter()
            .filter(|algorithm| algorithm.adapter == "native")
            .collect();
        for algorithm in &algorithms {
            let row = NativeAlgorithm::from_name(&algorithm.id)
                .unwrap_or_else(|| panic!("unregistered native algorithm: {}", algorithm.id));
            // SAFETY: The registry bounds-checks the row and returns a static name or NULL.
            let pointer = unsafe { bench_algorithm_name(row as u32) };
            assert!(
                !pointer.is_null(),
                "missing native row for {}",
                algorithm.id
            );
            // SAFETY: The non-null registry name is a static NUL-terminated string.
            let name = unsafe { CStr::from_ptr(pointer) };
            assert_eq!(name.to_str().unwrap(), algorithm.id);
        }
        // SAFETY: Registry lookups reject indices outside their tables.
        unsafe {
            assert!(bench_algorithm_name(algorithms.len() as u32).is_null());
            assert!(bench_kernel(u32::MAX, 0).is_none());
            assert!(bench_kernel(0, u32::MAX).is_none());
            assert_eq!(bench_layout(u32::MAX, 0), 0);
            assert_eq!(bench_layout(0, 3), 0);
        }
    }

    #[test]
    fn registered_kernels_sort_boundaries() {
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
        for algorithm in crate::plan::catalog()
            .algorithms
            .iter()
            .filter(|a| a.adapter == "native")
        {
            let variant = Variant {
                name: &algorithm.id,
                stable: algorithm.stable,
                adapter: &algorithm.adapter,
                threads: None,
            };
            for case in &cases {
                // This independent oracle preserves native capabilities and
                // verifies that opt-in Reverse keys decline every native kernel.
                let expected = match case.ty.as_str() {
                    "reverse_u64" | "reverse_f64" => false,
                    "u32" | "u64" | "f64" | "row_u32" | "row_u64" | "record128" | "owned" => {
                        match variant.name {
                            "clausecker" => case.ty == "row_u32",
                            "vqsort" | "x86simdsort" => matches!(case.ty.as_str(), "u32" | "u64"),
                            "duckdb" => !matches!(case.ty.as_str(), "f64" | "owned"),
                            "fluxsort" | "ips2ra" | "ips4o" | "crumsort" => case.ty != "owned",
                            _ => panic!("missing support expectation for {}", variant.name),
                        }
                    }
                    _ => panic!("missing support expectation for record type {}", case.ty),
                };
                let input = CaseInput::new(case);
                let prepared = variant.prepare(&input, 0);
                assert_eq!(
                    prepared.is_some(),
                    expected,
                    "{} / {}",
                    variant.name,
                    case.ty
                );
                if let Some(mut kernel) = prepared {
                    kernel.restore(1);
                    kernel.run_batch();
                    kernel.check().unwrap_or_else(|error| {
                        panic!("{} / {}: {error}", variant.name, case.id())
                    });
                }
            }
        }
    }
}
