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

#[pymodule(gil_used = true)]
mod _core {
    use pyo3::prelude::*;

    #[pymodule_export]
    use super::{argpartition, argsort, partition, partition_inplace, sort, sort_inplace};

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
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
