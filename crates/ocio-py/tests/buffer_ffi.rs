// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The views of Python buffers in `ocio-py-ffi` (owner decision P6-10), the binding's only
//! `unsafe` code for them: what each function reads is what Python itself reports of the same
//! buffer, and each refuses what it can't lend soundly. No wheel is involved: the expected
//! values come from the interpreter (`memoryview`, NumPy's `__array_interface__`, `tobytes`),
//! and the refusals are the port's own (docs/improvements.md, U-150, U-156).
//! - `check_buffer`: whether the object's type exports buffers (its `__buffer__`, which CPython
//!   3.12+ shows for the slot);
//! - `Buffer::request`: the format, item size, dimensions, shape, strides (C strides when the
//!   exporter gives none), read-only flag and address, as `memoryview` and NumPy report them;
//!   the span of the items, also for views whose address is their last item; the export held
//!   until the `Buffer` drops, on every path (a `bytearray` can't be resized while it lasts);
//! - `with_bytes`: each buffer's bytes at the place it says, read or written in place; spans
//!   that share bytes with a written one lent as one region; a read-only buffer refused for
//!   writing (U-156); memory another thread's call writes refused meanwhile, reading allowed
//!   alongside reading (U-150); the regions given back when the closure panics.
#![cfg(feature = "python")]

mod common;

use std::ffi::CString;
use std::sync::{Arc, Barrier};

use ocio_py_ffi::{Buffer, Bytes, check_buffer, with_bytes};
use pyo3::prelude::*;

/// The value of the Python expression `code`, with NumPy as `np` and ctypes imported.
fn eval<'py>(py: Python<'py>, code: &str) -> Bound<'py, PyAny> {
    let globals = pyo3::types::PyDict::new(py);
    globals.set_item("np", py.import("numpy").unwrap()).unwrap();
    globals
        .set_item("ctypes", py.import("ctypes").unwrap())
        .unwrap();
    let code = CString::new(code).unwrap();
    py.eval(&code, Some(&globals), None)
        .unwrap_or_else(|e| panic!("{e}"))
}

/// What Python reports of `obj`'s buffer, as `memoryview(obj)` reads it.
fn python_view(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
) -> (Vec<u8>, isize, isize, Vec<isize>, Vec<isize>, bool) {
    let mv = py
        .import("builtins")
        .unwrap()
        .getattr("memoryview")
        .unwrap()
        .call1((obj,))
        .unwrap();
    let format: String = mv.getattr("format").unwrap().extract().unwrap();
    (
        format.into_bytes(),
        mv.getattr("itemsize").unwrap().extract().unwrap(),
        mv.getattr("ndim").unwrap().extract().unwrap(),
        mv.getattr("shape").unwrap().extract().unwrap(),
        mv.getattr("strides").unwrap().extract().unwrap(),
        mv.getattr("readonly").unwrap().extract().unwrap(),
    )
}

/// The address of a NumPy array's data.
fn address(obj: &Bound<'_, PyAny>) -> usize {
    obj.getattr("__array_interface__")
        .unwrap()
        .get_item("data")
        .unwrap()
        .get_item(0)
        .unwrap()
        .extract()
        .unwrap()
}

#[test]
fn check_buffer_is_whether_the_type_exports_buffers() {
    common::start_python();
    Python::attach(|py| {
        let cases = [
            "b'abc'",
            "bytearray(4)",
            "memoryview(b'ab')",
            "np.zeros(3, 'f4')",
            "np.float32(1.5)",
            "(ctypes.c_float * 3)()",
            "3",
            "1.5",
            "'abc'",
            "[1.0, 2.0]",
            "None",
            "np",
        ];
        let mut exporters = 0;
        for code in cases {
            let obj = eval(py, code);
            let expected: bool = obj.get_type().hasattr("__buffer__").unwrap();
            assert_eq!(check_buffer(&obj), expected, "{code}");
            exporters += usize::from(expected);
        }
        assert_eq!(exporters, 6);
    });
}

#[test]
fn request_reads_what_python_reports_of_the_buffer() {
    common::start_python();
    Python::attach(|py| {
        for code in [
            "np.arange(12, dtype='<f4')",
            "np.arange(12, dtype='<f4')[::-1]",
            "np.arange(12, dtype='<f4')[1::3]",
            "np.arange(12, dtype='<f4').reshape(3, 4).T",
            "np.arange(12, dtype='<u2').reshape(2, 3, 2)[:, ::-1, :]",
            "np.zeros((0, 4), '<f4')",
            "np.float32(2.5)",
            "np.frombuffer(b'abcdefgh', '<f4')",
            "b'abcd'",
            "bytearray(b'abcdef')",
            "memoryview(bytearray(16)).cast('f')",
            "memoryview(bytearray(24)).cast('f', (2, 3))[::-1]",
            "(ctypes.c_float * 3)(1, 2, 3)",
            "((ctypes.c_short * 3) * 2)()",
        ] {
            let obj = eval(py, code);
            let buffer = Buffer::request(&obj).unwrap();
            let info = buffer.info();
            let (format, itemsize, ndim, shape, strides, readonly) = python_view(py, &obj);
            // ctypes' formats carry a byte order (`<f`), which memoryview reports as given.
            assert_eq!(info.format, format, "{code}");
            assert_eq!(
                (
                    info.itemsize,
                    info.ndim,
                    &info.shape,
                    &info.strides,
                    info.readonly
                ),
                (itemsize, ndim, &shape, &strides, readonly),
                "{code}"
            );
            // An array's address (a NumPy scalar makes a new array for each request).
            if obj
                .is_instance(&py.import("numpy").unwrap().getattr("ndarray").unwrap())
                .unwrap()
            {
                assert_eq!(info.ptr, address(&obj), "{code}");
            }
            // The span: the bytes from the lowest item's first to the highest's last, which
            // `tobytes` of the items (C order) visits in some order.
            let (start, end) = buffer.span();
            let items: usize = shape.iter().product::<isize>() as usize;
            if items == 0 {
                assert_eq!(start, end, "{code}");
                continue;
            }
            let mut low = 0isize;
            let mut high = itemsize;
            for (&n, &s) in shape.iter().zip(&strides) {
                low += ((n - 1) * s).min(0);
                high += ((n - 1) * s).max(0);
            }
            assert_eq!(start as isize, info.ptr as isize + low, "{code}");
            assert_eq!((end - start) as isize, high - low, "{code}");
            assert_eq!(buffer.span_offset() as isize, -low, "{code}");
        }
    });
}

#[test]
fn request_passes_the_exporters_error_through() {
    common::start_python();
    Python::attach(|py| {
        for code in ["3", "'abc'", "None"] {
            let obj = eval(py, code);
            let error = Buffer::request(&obj).expect_err(code);
            assert!(
                error.is_instance_of::<pyo3::exceptions::PyTypeError>(py),
                "{code}: {error}"
            );
        }
    });
}

/// A `bytearray` can't be resized while an export of it lasts: whether `b` has one.
fn exported(py: Python<'_>, b: &Bound<'_, PyAny>) -> bool {
    match b.call_method1("extend", (pyo3::types::PyBytes::new(py, b"x"),)) {
        Ok(_) => {
            b.call_method1("pop", ()).unwrap();
            false
        }
        Err(e) => {
            assert!(
                e.is_instance_of::<pyo3::exceptions::PyBufferError>(py),
                "{e}"
            );
            true
        }
    }
}

#[test]
fn the_export_lasts_until_the_buffer_drops_on_every_path() {
    common::start_python();
    Python::attach(|py| {
        let b = eval(py, "bytearray(b'abcdefgh')");
        assert!(!exported(py, &b));
        let buffer = Buffer::request(&b).unwrap();
        assert!(exported(py, &b));
        drop(buffer);
        assert!(!exported(py, &b));
        // Ended after with_bytes too, whether its closure returned, it refused, or the closure
        // panicked.
        let buffer = Buffer::request(&b).unwrap();
        with_bytes(py, &[(&buffer, true)], |_, _| ()).unwrap();
        assert!(exported(py, &b));
        drop(buffer);
        assert!(!exported(py, &b));
        let buffer = Buffer::request(&b).unwrap();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_bytes(py, &[(&buffer, true)], |_, _| panic!("probe"))
        }));
        assert!(panicked.is_err());
        drop(buffer);
        assert!(!exported(py, &b));
    });
}

#[test]
fn with_bytes_lends_each_buffers_items_where_it_says() {
    common::start_python();
    Python::attach(|py| {
        let base = eval(py, "np.arange(24, dtype='<f4')");
        let views = [
            eval(py, "lambda a: a[::-1]").call1((&base,)).unwrap(),
            eval(py, "lambda a: a[3::4]").call1((&base,)).unwrap(),
            eval(py, "lambda a: a.reshape(4, 6).T[1:3]")
                .call1((&base,))
                .unwrap(),
        ];
        for view in &views {
            let buffer = Buffer::request(view).unwrap();
            let info = buffer.info().clone();
            let expected: Vec<u8> = view.call_method0("tobytes").unwrap().extract().unwrap();
            let read = with_bytes(py, &[(&buffer, false)], |regions, places| {
                let Bytes::Read(bytes) = &regions[places[0].0] else {
                    panic!("lent to write");
                };
                // The items in C order, from `buf` with the strides.
                let mut out = Vec::new();
                let mut index = vec![0isize; info.shape.len()];
                loop {
                    let at = places[0].1 as isize
                        + index
                            .iter()
                            .zip(&info.strides)
                            .map(|(i, s)| i * s)
                            .sum::<isize>();
                    out.extend_from_slice(&bytes[at as usize..at as usize + 4]);
                    let mut d = index.len();
                    loop {
                        if d == 0 {
                            return out;
                        }
                        d -= 1;
                        index[d] += 1;
                        if index[d] < info.shape[d] {
                            break;
                        }
                        index[d] = 0;
                    }
                }
            })
            .unwrap();
            assert_eq!(read, expected);
        }
        // Written in place: the first item of the reversed view is the base's last.
        let buffer = Buffer::request(&views[0]).unwrap();
        with_bytes(py, &[(&buffer, true)], |mut regions, places| {
            let Bytes::Write(bytes) = &mut regions[0] else {
                panic!("lent to read");
            };
            let at = places[0].1;
            bytes[at..at + 4].copy_from_slice(&(-7.5f32).to_le_bytes());
        })
        .unwrap();
        drop(buffer);
        let last: f32 = base.get_item(23).unwrap().extract().unwrap();
        assert_eq!(last.to_bits(), (-7.5f32).to_bits());
    });
}

#[test]
fn spans_sharing_bytes_with_a_written_one_are_one_region() {
    common::start_python();
    Python::attach(|py| {
        let base = eval(py, "np.arange(16, dtype='<f4')");
        let other = eval(py, "np.zeros(4, '<f4')");
        let first = eval(py, "lambda a: a[:8]").call1((&base,)).unwrap();
        let second = eval(py, "lambda a: a[4:12]").call1((&base,)).unwrap();
        let third = eval(py, "lambda a: a[10:]").call1((&base,)).unwrap();
        let buffers: Vec<Buffer> = [&first, &second, &third, &other]
            .iter()
            .map(|o| Buffer::request(o).unwrap())
            .collect();
        let base_address = address(&base);
        // The first read, the second written: the third shares bytes with the second only, and
        // joins its region; the other array is a region of its own.
        let lends = [
            (&buffers[0], false),
            (&buffers[1], true),
            (&buffers[2], false),
            (&buffers[3], false),
        ];
        let (kinds, places) = with_bytes(py, &lends, |regions, places| {
            let kinds: Vec<(bool, usize)> = regions
                .iter()
                .map(|r| match r {
                    Bytes::Read(b) => (false, b.len()),
                    Bytes::Write(b) => (true, b.len()),
                })
                .collect();
            (kinds, places)
        })
        .unwrap();
        assert_eq!(kinds, vec![(true, 64), (false, 16)]);
        assert_eq!(places, vec![(0, 0), (0, 16), (0, 40), (1, 0)]);
        assert_eq!(buffers[0].span().0, base_address);
        // Only reads: each span a region of its own, overlapping or not.
        let reads: Vec<(&Buffer, bool)> = buffers.iter().map(|b| (b, false)).collect();
        let count = with_bytes(py, &reads, |regions, _| regions.len()).unwrap();
        assert_eq!(count, 4);
    });
}

#[test]
fn a_read_only_buffer_is_refused_for_writing() {
    common::start_python();
    Python::attach(|py| {
        for code in [
            "np.frombuffer(b'abcdefgh', '<f4')",
            "b'abcd'",
            "memoryview(b'ab')",
        ] {
            let obj = eval(py, code);
            let buffer = Buffer::request(&obj).unwrap();
            assert!(buffer.info().readonly, "{code}");
            let error = with_bytes(py, &[(&buffer, true)], |_, _| ()).expect_err(code);
            assert!(
                error.is_instance_of::<pyo3::exceptions::PyBufferError>(py),
                "{error}"
            );
            assert!(error.to_string().contains("U-156"), "{error}");
            with_bytes(py, &[(&buffer, false)], |_, _| ()).unwrap();
        }
        // Without items, nothing is written.
        let empty = eval(py, "b''");
        let buffer = Buffer::request(&empty).unwrap();
        with_bytes(py, &[(&buffer, true)], |_, _| ()).unwrap();
    });
}

/// Runs a call on the array `code` (`first`: written) that holds its memory while another
/// thread runs a call on `other(array)` (`second`: written); whether the other thread's call
/// was allowed.
fn concurrent(code: &str, other: &str, first: bool, second: bool) -> bool {
    common::start_python();
    let (obj, other) = Python::attach(|py| {
        let obj = eval(py, code);
        let other = eval(py, other).call1((&obj,)).unwrap();
        (obj.unbind(), other.unbind())
    });
    let lent = Arc::new(Barrier::new(2));
    let tried = Arc::new(Barrier::new(2));
    let (l, t) = (lent.clone(), tried.clone());
    let worker = std::thread::spawn(move || {
        l.wait();
        let allowed = Python::attach(|py| {
            let buffer = Buffer::request(other.bind(py)).unwrap();
            match with_bytes(py, &[(&buffer, second)], |_, _| ()) {
                Ok(()) => true,
                Err(e) => {
                    assert!(e.to_string().contains("U-150"), "{e}");
                    false
                }
            }
        });
        t.wait();
        allowed
    });
    Python::attach(|py| {
        let buffer = Buffer::request(obj.bind(py)).unwrap();
        // The other thread's call runs while this one holds the memory, the GIL released.
        with_bytes(py, &[(&buffer, first)], |_, _| {
            lent.wait();
            tried.wait();
        })
        .unwrap();
    });
    worker.join().unwrap()
}

#[test]
fn memory_another_threads_call_writes_is_refused_meanwhile() {
    let code = "np.arange(8, dtype='<f4')";
    let overlapping = "lambda a: a[2:]";
    assert!(concurrent(code, overlapping, false, false));
    assert!(!concurrent(code, overlapping, true, false));
    assert!(!concurrent(code, overlapping, false, true));
    assert!(!concurrent(code, overlapping, true, true));
    // Memory that doesn't overlap: allowed.
    let separate = "lambda a: np.zeros(4, '<f4')";
    assert!(concurrent(code, separate, true, true));
    // Adjacent memory of one array: the spans share no byte.
    assert!(concurrent(
        "np.arange(8, dtype='<f4')[:4]",
        "lambda a: a.base[4:]",
        true,
        true
    ));
    // Bytes: spans that share exactly one byte overlap; spans that touch don't.
    assert!(!concurrent(
        "np.arange(8, dtype='u1')[0:4]",
        "lambda a: a.base[3:7]",
        true,
        false
    ));
    assert!(concurrent(
        "np.arange(8, dtype='u1')[0:4]",
        "lambda a: a.base[4:8]",
        true,
        true
    ));
}

/// Spans that share exactly one byte, one of them written, are one region; spans that touch
/// without sharing a byte are two.
#[test]
fn spans_sharing_one_byte_are_one_region() {
    common::start_python();
    Python::attach(|py| {
        let base = eval(py, "np.arange(8, dtype='u1')");
        let view = |code: &str| eval(py, code).call1((&base,)).unwrap();
        let (a, b, c) = (
            view("lambda a: a[0:4]"),
            view("lambda a: a[3:7]"),
            view("lambda a: a[4:8]"),
        );
        let (a, b, c) = (
            Buffer::request(&a).unwrap(),
            Buffer::request(&b).unwrap(),
            Buffer::request(&c).unwrap(),
        );
        let regions = |lends: &[(&Buffer, bool)]| {
            with_bytes(py, lends, |regions, places| (regions.len(), places)).unwrap()
        };
        assert_eq!(
            regions(&[(&a, false), (&b, true)]),
            (1, vec![(0, 0), (0, 3)])
        );
        assert_eq!(
            regions(&[(&a, true), (&b, false)]),
            (1, vec![(0, 0), (0, 3)])
        );
        assert_eq!(
            regions(&[(&a, false), (&c, true)]),
            (2, vec![(0, 0), (1, 0)])
        );
    });
}

/// The span of items whose sums leave 128 bits (a view a C exporter could give, of 16
/// dimensions of 2^62 + 1 items 2^62 bytes apart, whose sum wraps to the item size) is refused
/// (U-157), checked, in either direction; one that fits in 128 bits but not in the address
/// space too.
#[test]
fn a_span_past_128_bits_is_refused() {
    let info = |dims: usize, stride: isize| ocio_py_ffi::BufferInfo {
        ptr: 1 << 40,
        itemsize: 4,
        format: b"f".to_vec(),
        ndim: dims as isize,
        shape: vec![(1 << 62) + 1; dims],
        strides: vec![stride; dims],
        readonly: false,
    };
    for stride in [1 << 62, -(1 << 62)] {
        assert_eq!(ocio_py_ffi::items_span(&info(16, stride)), None);
        assert_eq!(ocio_py_ffi::items_span(&info(2, stride)), None);
    }
}

#[test]
fn the_regions_are_given_back_when_the_closure_panics() {
    common::start_python();
    Python::attach(|py| {
        let obj = eval(py, "np.zeros(4, '<f4')");
        let buffer = Buffer::request(&obj).unwrap();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_bytes(py, &[(&buffer, true)], |_, _| panic!("probe"))
        }));
        assert!(panicked.is_err());
        with_bytes(py, &[(&buffer, true)], |_, _| ()).unwrap();
    });
}
