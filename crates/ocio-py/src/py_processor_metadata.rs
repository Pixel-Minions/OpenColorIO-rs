// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyProcessorMetadata` (src/bindings/python/PyProcessorMetadata.cpp @ v2.5.2): the class
//! `ProcessorMetadata` and its two iterator classes (chunk 6.3d). Its objects hold their
//! metadata as pybind11 holds a `ProcessorMetadataRcPtr` ([`Shared`]), which the iterators
//! share.

use ocio::ProcessorMetadata;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Param};
use crate::py_context::null_string;
use crate::py_transform::{INT, Out, class, def, nested_class, none, ok, string_out};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct};
use crate::{method_def, q, this_iter, this_shared};

const PROCESSOR_METADATA: &str = q!("ProcessorMetadata");
const FILE_ITERATOR: &str = q!("ProcessorMetadata.FileIterator");
const LOOK_ITERATOR: &str = q!("ProcessorMetadata.LookIterator");
const SELF: Param = Param::new("self", PROCESSOR_METADATA);

type MetadataRc = Shared<ProcessorMetadata>;

// .def(py::init(&ProcessorMetadata::Create)) (PyProcessorMetadata.cpp:40-41 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[crate::dispatch::Overload {
        params: &[SELF],
        ret: "None",
        call: |py, _, a, _| {
            construct(
                &a[0],
                PROCESSOR_METADATA,
                Shared::new(ProcessorMetadata::new()),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getFiles", [](ProcessorMetadataRcPtr & self) { return FileIterator(self); })
// (PyProcessorMetadata.cpp:43-46 @ v2.5.2).
static GET_FILES: Function = method_def!("getFiles", [SELF], FILE_ITERATOR, |py, o, a, _| {
    let rc = this_shared!(o, a, ProcessorMetadata);
    Ok(Some(
        new_iterator(py, FILE_ITERATOR, Some(Box::new(rc)))?.unbind(),
    ))
});

// .def("getLooks", [](ProcessorMetadataRcPtr & self) { return LookIterator(self); })
// (PyProcessorMetadata.cpp:47-50 @ v2.5.2).
static GET_LOOKS: Function = method_def!("getLooks", [SELF], LOOK_ITERATOR, |py, o, a, _| {
    let rc = this_shared!(o, a, ProcessorMetadata);
    Ok(Some(
        new_iterator(py, LOOK_ITERATOR, Some(Box::new(rc)))?.unbind(),
    ))
});

// .def("addFile", &ProcessorMetadata::addFile, "fileName"_a) (PyProcessorMetadata.cpp:51-52 @
// v2.5.2): a null name is U-123 (`files.insert(fname)` makes a std::string of it).
static ADD_FILE: Function = method_def!(
    "addFile",
    [SELF, Param::new("fileName", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ProcessorMetadata);
        let Some(name) = crate::arg!(a, 1, convert, CharPtr).0 else {
            return Err(null_string("addFile(fileName)"));
        };
        rc.lock().add_file(&name);
        none(py)
    }
);

// .def("addLook", &ProcessorMetadata::addLook, "look"_a) (PyProcessorMetadata.cpp:53-54 @
// v2.5.2): a null look is U-123 (`looks.push_back(look)`).
static ADD_LOOK: Function = method_def!(
    "addLook",
    [SELF, Param::new("look", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ProcessorMetadata);
        let Some(look) = crate::arg!(a, 1, convert, CharPtr).0 else {
            return Err(null_string("addLook(look)"));
        };
        rc.lock().add_look(&look);
        none(py)
    }
);

/// The metadata an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<MetadataRc>()?.clone();
        (it, rc)
    }};
}

fn num_files(rc: &MetadataRc) -> i32 {
    rc.lock().num_files()
}

fn num_looks(rc: &MetadataRc) -> i32 {
    rc.lock().num_looks()
}

fn file(py: Python<'_>, rc: &MetadataRc, i: i32) -> Out {
    let f = rc.lock().file(i).to_vec();
    string_out(py, &f)
}

fn look(py: Python<'_>, rc: &MetadataRc, i: i32) -> Out {
    let l = rc.lock().look(i).to_vec();
    string_out(py, &l)
}

/// The four methods of an iterator class (PyProcessorMetadata.cpp:56-82 @ v2.5.2).
macro_rules! iterator {
    ($self:expr, $qualified:expr, $num:expr, $item:expr,
     $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let (_, rc) = iter_rc!(o, a);
            ok(py, $num(&rc))
        });

        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            "str",
            |py, o, a, convert| {
                let (_, rc) = iter_rc!(o, a);
                let i = crate::arg!(a, 1, convert, i32);
                check_index(i, $num(&rc))?;
                $item(py, &rc, i)
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = iter_rc!(o, a);
            Ok(Some(a[0].clone().unbind()))
        });

        static $next: Function = method_def!("__next__", [$self], "str", |py, o, a, _| {
            let (it, rc) = iter_rc!(o, a);
            let i = it.get().next_index($num(&rc))?;
            $item(py, &rc, i)
        });
    };
}

const FILE_SELF: Param = Param::new("self", FILE_ITERATOR);
const LOOK_SELF: Param = Param::new("self", LOOK_ITERATOR);

iterator!(
    FILE_SELF,
    FILE_ITERATOR,
    num_files,
    file,
    FILE_LEN,
    FILE_GETITEM,
    FILE_ITER,
    FILE_NEXT
);
iterator!(
    LOOK_SELF,
    LOOK_ITERATOR,
    num_looks,
    look,
    LOOK_LEN,
    LOOK_GETITEM,
    LOOK_ITER,
    LOOK_NEXT
);

/// Port of `bindPyProcessorMetadata` (src/bindings/python/PyProcessorMetadata.cpp:26-83 @
/// v2.5.2), in its order.
pub(crate) fn bind_py_processor_metadata(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, PROCESSOR_METADATA)?;
    let files = nested_class(&cls, "ProcessorMetadata", "FileIterator")?;
    let looks = nested_class(&cls, "ProcessorMetadata", "LookIterator")?;
    for f in [&INIT, &GET_FILES, &GET_LOOKS, &ADD_FILE, &ADD_LOOK] {
        def(&cls, f)?;
    }
    for f in [&FILE_LEN, &FILE_GETITEM, &FILE_ITER, &FILE_NEXT] {
        def(&files, f)?;
    }
    for f in [&LOOK_LEN, &LOOK_GETITEM, &LOOK_ITER, &LOOK_NEXT] {
        def(&looks, f)?;
    }
    Ok(())
}
