// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The embedding API (chunk 6.1c): a Rust program registers `PyOpenColorIO`, starts Python in
//! its own process, and Python code imports the module from the program itself, sharing the
//! process's OCIO state with the Rust side. Built with ocio-py's `python` feature, against
//! the interpreter of target/py-port (the gate's test-py step, `cargo xtask py-check`).
#![cfg(feature = "python")]

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Runs `code` with `OCIO` (the module, imported as embedded Python code imports it) in its
/// globals, and returns them.
fn run<'py>(py: Python<'py>, code: &std::ffi::CStr) -> Bound<'py, PyDict> {
    let globals = PyDict::new(py);
    globals
        .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
        .unwrap();
    py.run(code, Some(&globals), None)
        .unwrap_or_else(|e| panic!("{e}"));
    globals
}

/// One test: an interpreter starts once per process. The module is a built-in module of the
/// interpreter (no package on disk), named `PyOpenColorIO`; an environment variable the Rust
/// side sets is the one Python reads, and the other way round; an OCIO error raised in Python
/// is `OCIO.Exception`, catchable as Python's `Exception`.
#[test]
fn an_embedded_interpreter_imports_the_module_and_shares_the_process() {
    ocio_py::embed::append_to_inittab();
    Python::initialize();
    // Registering again, once the interpreter runs, is refused.
    assert!(std::panic::catch_unwind(ocio_py::embed::append_to_inittab).is_err());

    Python::attach(|py| {
        let globals = run(
            py,
            c"import sys\n\
              builtin = 'PyOpenColorIO' in sys.builtin_module_names\n\
              name = OCIO.__name__\n\
              has_file = hasattr(OCIO, '__file__')\n",
        );
        let get = |name: &str| globals.get_item(name).unwrap().unwrap();
        assert!(get("builtin").extract::<bool>().unwrap());
        assert_eq!(get("name").extract::<String>().unwrap(), "PyOpenColorIO");
        assert!(!get("has_file").extract::<bool>().unwrap());

        ocio::set_env_variable("OCIO_RS_EMBED_FROM_RUST", Some(b"rust")).unwrap();
        let globals = run(
            py,
            c"from_rust = OCIO.GetEnvVariable('OCIO_RS_EMBED_FROM_RUST')\n\
              OCIO.SetEnvVariable('OCIO_RS_EMBED_FROM_PYTHON', 'python')\n",
        );
        let from_rust = globals.get_item("from_rust").unwrap().unwrap();
        assert_eq!(from_rust.extract::<String>().unwrap(), "rust");
        assert_eq!(
            ocio::get_env_variable("OCIO_RS_EMBED_FROM_PYTHON"),
            b"python"
        );

        let globals = run(
            py,
            c"caught = []\n\
              try:\n    raise OCIO.Exception('from python')\n\
              except Exception as e:\n    caught.append((type(e).__qualname__, str(e)))\n",
        );
        let caught: Vec<(String, String)> = globals
            .get_item("caught")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(
            caught,
            [("Exception".to_string(), "from python".to_string())]
        );

        // A Rust config in Python is a copy; a Python config in Rust is a snapshot, with the
        // edits Python made through the config's own objects.
        let mut config = ocio::Config::create_raw().unwrap().as_ref().clone();
        config.set_name("from rust");
        let obj = ocio_py::embed::config_to_python(py, &config).unwrap();
        let globals = PyDict::new(py);
        globals.set_item("c", &obj).unwrap();
        globals
            .set_item("OCIO", py.import("PyOpenColorIO").unwrap())
            .unwrap();
        py.run(
            c"name = c.getName()\nc.setName('from python')\n\
              c.getColorSpace('raw').setDescription('edited in python')\n",
            Some(&globals),
            None,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let name: String = globals
            .get_item("name")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(name, "from rust");
        assert_eq!(config.name(), b"from rust");
        let snapshot = ocio_py::embed::config_from_python(obj.bind(py)).unwrap();
        assert_eq!(snapshot.name(), b"from python");
        assert_eq!(
            snapshot.color_space("raw").unwrap().description(),
            b"edited in python"
        );
        py.run(c"c.setName('later')", Some(&globals), None).unwrap();
        assert_eq!(snapshot.name(), b"from python");
        let not_config = py.eval(c"1", None, None).unwrap();
        assert!(ocio_py::embed::config_from_python(&not_config).is_err());
    });
}
