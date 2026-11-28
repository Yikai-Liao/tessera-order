// SPDX-License-Identifier: BSD-2-Clause

mod args;
mod encoding;
mod pool;

mod buffer;
mod containers;

use args::Axis;
use containers::{Output, Request};
use pool::Threads;
use pyo3::prelude::*;
use pyo3::types::PyDict;

// PyO3 extractors take one argument; the entry point supplies the None policy.
fn axis_arg<const ALLOW_NONE: bool>(obj: &Bound<'_, PyAny>) -> PyResult<Axis> {
    args::parse_axis(obj, ALLOW_NONE)
}

macro_rules! entry_point {
    ($name:ident, $output:ident, $allow_none:literal, $signature:literal $(, $kth:ident)?) => {
        #[pyfunction(
            signature = (a, $($kth,)? axis=Axis::Index(-1), *, descending=false, key=None, threads=Threads::Serial),
            text_signature = $signature
        )]
        fn $name(
            a: &Bound<'_, PyAny>, $($kth: &Bound<'_, PyAny>,)?
            #[pyo3(from_py_with = axis_arg::<$allow_none>)] axis: Axis,
            #[pyo3(from_py_with = args::parse_descending)] descending: bool,
            key: Option<&Bound<'_, PyAny>>,
            #[pyo3(from_py_with = args::parse_threads)] threads: Threads,
        ) -> PyResult<Py<PyAny>> {
            let key = key.map(args::parse_key).transpose()?.flatten();
            let kth: Option<args::RawKth> = entry_point!(@kth $($kth)?);
            containers::dispatch(a, Request {
                output: Output::$output, axis, descending, threads, kth: kth.as_ref(),
            }, key.as_ref())
        }
    };
    (@kth $kth:ident) => { Some(args::parse_kth($kth)?) };
    (@kth) => { None };
}

entry_point!(
    sort,
    Copy,
    true,
    "(a, axis=-1, *, descending=None, key=None, threads=1)"
);
entry_point!(
    sort_inplace,
    Inplace,
    false,
    "(a, axis=-1, *, descending=None, key=None, threads=1)"
);
entry_point!(
    argsort,
    Indices,
    true,
    "(a, axis=-1, *, descending=None, key=None, threads=1)"
);
entry_point!(
    partition,
    Copy,
    true,
    "(a, kth, axis=-1, *, descending=None, key=None, threads=1)",
    kth
);
entry_point!(
    partition_inplace,
    Inplace,
    false,
    "(a, kth, axis=-1, *, descending=None, key=None, threads=1)",
    kth
);
entry_point!(
    argpartition,
    Indices,
    true,
    "(a, kth, axis=-1, *, descending=None, key=None, threads=1)",
    kth
);

fn ensure_gil_enabled(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    if sys.hasattr("_is_gil_enabled")?
        && !sys.getattr("_is_gil_enabled")?.call0()?.extract::<bool>()?
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "tessera_order requires a GIL-enabled Python interpreter",
        ));
    }
    Ok(())
}

#[pyfunction]
fn _build_info(py: Python<'_>) -> PyResult<Py<PyDict>> {
    let info = PyDict::new(py);
    for (name, value) in [
        ("implementation", "tessera-order-py"),
        ("package_version", env!("TESSERA_PY_VERSION")),
        ("core_version", env!("CARGO_PKG_VERSION")),
        ("kernel", "tessera-order"),
        ("commit", env!("TESSERA_BUILD_COMMIT")),
        ("dirty", env!("TESSERA_BUILD_DIRTY")),
        ("source_sha256", env!("TESSERA_BUILD_SOURCE_SHA256")),
        ("rustc", env!("TESSERA_BUILD_RUSTC")),
        ("target", env!("TESSERA_BUILD_TARGET")),
        ("profile", env!("TESSERA_BUILD_PROFILE")),
        ("opt_level", env!("TESSERA_BUILD_OPT_LEVEL")),
        ("rustflags", env!("TESSERA_BUILD_RUSTFLAGS")),
        ("abi", env!("TESSERA_BUILD_ABI")),
    ] {
        info.set_item(name, value)?;
    }
    Ok(info.unbind())
}

#[pymodule(gil_used = true)]
mod _core {
    use pyo3::prelude::*;

    #[pymodule_export]
    use super::{
        _build_info, argpartition, argsort, partition, partition_inplace, sort, sort_inplace,
    };

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        super::ensure_gil_enabled(module.py())?;
        module.add("__version__", env!("TESSERA_PY_VERSION"))?;
        module.add("__core_version__", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::exceptions::{PyTypeError, PyValueError};
    use pyo3::types::PyDict;
    use std::ffi::CString;

    #[test]
    fn gil_enabled_interpreter_required() {
        Python::initialize();
        Python::attach(|py| {
            let sys = py.import("sys").unwrap();
            let original = sys.getattr("_is_gil_enabled").ok();
            // Restore before asserting so a failing check cannot poison other tests.
            let result = (|| -> PyResult<_> {
                sys.setattr("_is_gil_enabled", py.eval(c"lambda: False", None, None)?)?;
                let rejected = ensure_gil_enabled(py);
                sys.setattr("_is_gil_enabled", py.eval(c"lambda: True", None, None)?)?;
                let accepted = ensure_gil_enabled(py);
                Ok((rejected, accepted))
            })();
            if let Some(original) = original {
                sys.setattr("_is_gil_enabled", original).unwrap();
            } else {
                sys.delattr("_is_gil_enabled").unwrap();
            }
            let (rejected, accepted) = result.unwrap();
            assert!(
                rejected
                    .unwrap_err()
                    .is_instance_of::<pyo3::exceptions::PyRuntimeError>(py)
            );
            accepted.unwrap();
        });
    }

    #[test]
    fn exported_signatures_and_shared_validation() {
        Python::initialize();
        Python::attach(|py| {
            let module = pyo3::wrap_pymodule!(_core)(py);
            let module = module.bind(py);
            assert_eq!(
                module
                    .getattr("__version__")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                env!("TESSERA_PY_VERSION")
            );
            assert_eq!(
                module
                    .getattr("__core_version__")
                    .unwrap()
                    .extract::<String>()
                    .unwrap(),
                env!("CARGO_PKG_VERSION")
            );
            let locals = PyDict::new(py);
            locals.set_item("core", module).unwrap();
            let eval = |source: &str| py.eval(&CString::new(source).unwrap(), None, Some(&locals));
            for name in [
                "sort",
                "sort_inplace",
                "argsort",
                "partition",
                "partition_inplace",
                "argpartition",
            ] {
                let partition = name.contains("partition");
                let required = if partition { "[2, 1], 0" } else { "[2, 1]" };
                let parameters = if partition { "a, kth" } else { "a" };
                let signature =
                    format!("({parameters}, axis=-1, *, descending=None, key=None, threads=1)");
                assert_eq!(
                    module
                        .getattr(name)
                        .unwrap()
                        .getattr("__text_signature__")
                        .unwrap()
                        .extract::<String>()
                        .unwrap(),
                    signature
                );
                for extra in [
                    "",
                    ", descending=True, key=abs, threads=None",
                    ", descending=False, threads=2",
                ] {
                    eval(&format!("core.{name}({required}{extra})")).unwrap();
                }
                for extra in [
                    ", axis=True",
                    ", descending=1",
                    ", key=3",
                    ", threads=True",
                    ", threads=0.0",
                    ", axis=0, descending=None, key=None, threads='2'",
                    ", -1, True",
                ] {
                    assert!(
                        eval(&format!("core.{name}({required}{extra})"))
                            .unwrap_err()
                            .is_instance_of::<PyTypeError>(py),
                        "{name}{extra}"
                    );
                }
                assert!(
                    eval(&format!("core.{name}({required}, threads=0)"))
                        .unwrap_err()
                        .is_instance_of::<PyValueError>(py)
                );
                let result = eval(&format!("core.{name}({required}, axis=None)"));
                if name.ends_with("inplace") {
                    assert!(result.unwrap_err().is_instance_of::<PyTypeError>(py));
                } else {
                    result.unwrap();
                }
                if partition {
                    assert!(
                        eval(&format!("core.{name}([], [True])"))
                            .unwrap_err()
                            .is_instance_of::<PyTypeError>(py)
                    );
                }
            }
        });
    }
}
