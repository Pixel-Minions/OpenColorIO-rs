// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `PyDynamicProperty` and `bindPyDynamicProperty` (src/bindings/python/PyDynamicProperty.h,
//! PyDynamicProperty.cpp @ v2.5.2): the value an instance of `DynamicProperty` holds, which
//! `CPUProcessor.getDynamicProperty` returns (chunk 6.5b), and the class's methods (6.7a).
//!
//! **The curves.** `getGradingRGBCurve` and `getGradingHueCurve` return the curves the property
//! holds (`const ConstGradingRGBCurveRcPtr &`, which pybind11 hands out as the very object): a
//! change Python makes through them shows in what they return again (not in what the processor
//! renders: the property's knots are fitted by `setValue` only), until a `set...` replaces the
//! curves with a copy. The binding keeps, per property, the object it gave
//! ([`CURVES`]): the property's curves for Python while the library hasn't replaced them.

use std::sync::{Arc, Mutex, Weak};

use ocio_ops::dynamic_property::{
    DynamicPropertyGradingHueCurveImpl, DynamicPropertyGradingRgbCurveImpl, DynamicPropertyRcPtr,
};
use ocio_ops::ops::gradingprimary::grading_primary::GradingPrimary;
use ocio_ops::ops::gradingtone::grading_tone::GradingTone;
use pyo3::prelude::*;

use crate::dispatch::{Function, Param};
use crate::py_grading_data::{
    GRADING_HUE_CURVE, GRADING_PRIMARY, GRADING_RGB_CURVE, GRADING_TONE, HueCurveNode, HueCurveRc,
    RgbCurveNode, RgbCurveRc, hue_curve_value, rgb_curve_value,
};
use crate::py_open_color_io::py_result;
use crate::py_transform::{FLOAT, Out, class, def, enum_out, none, ok};
use crate::shared::{Shared, holder, holder_arg, out};
use crate::{method_def, pybind11, q};

pub(crate) const DYNAMIC_PROPERTY: &str = q!("DynamicProperty");
const DYNAMIC_PROPERTY_TYPE: &str = q!("DynamicPropertyType");
const SELF: Param = Param::new("self", DYNAMIC_PROPERTY);

/// The value of a `DynamicProperty` instance: `PyDynamicProperty`'s `m_prop`, the property it
/// shares with its processor.
#[pyclass(frozen, module = "pybind11_builtins", name = "_dynamic_property")]
#[derive(Debug)]
pub(crate) struct DynamicPropertyHolder(pub(crate) DynamicPropertyRcPtr);

/// How many [`DynamicPropertyHolder`]s live, by their property's address
/// ([`ocio_gpu::gpu_shader::property_address`]): Python's `shared_ptr`s of each property, which
/// keep it alive in the wheel (`PyDynamicProperty::m_prop`).
static HOLDERS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

fn holders() -> std::sync::MutexGuard<'static, Vec<(usize, usize)>> {
    HOLDERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl DynamicPropertyHolder {
    fn new(prop: DynamicPropertyRcPtr) -> DynamicPropertyHolder {
        let address = ocio_gpu::gpu_shader::property_address(&prop);
        let mut list = holders();
        match list.iter_mut().find(|(a, _)| *a == address) {
            Some((_, count)) => *count += 1,
            None => list.push((address, 1)),
        }
        DynamicPropertyHolder(prop)
    }
}

impl Drop for DynamicPropertyHolder {
    fn drop(&mut self) {
        let address = ocio_gpu::gpu_shader::property_address(&self.0);
        let mut list = holders();
        if let Some(k) = list.iter().position(|(a, _)| *a == address) {
            list[k].1 -= 1;
            if list[k].1 == 0 {
                list.swap_remove(k);
            }
        }
    }
}

/// Whether a Python object holds the dynamic property at `address`.
pub(crate) fn held_by_python(address: usize) -> bool {
    holders().iter().any(|(a, _)| *a == address)
}

/// A returned `PyDynamicProperty` (a value, which pybind11 moves into a new instance).
pub(crate) fn dynamic_property_out(py: Python<'_>, prop: DynamicPropertyRcPtr) -> Out {
    let cls = class(py, DYNAMIC_PROPERTY)?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        DYNAMIC_PROPERTY,
        Bound::new(py, DynamicPropertyHolder::new(prop))?.into_any(),
    )?;
    Ok(Some(obj.unbind()))
}

/// The property `self` holds, or the next overload.
macro_rules! this_prop {
    ($o:expr, $a:expr) => {{
        let Some(this) = pybind11::instance_of(&$a[0], $o.params[0].ty) else {
            return Ok(None);
        };
        let value = pybind11::read_value(&this, $o.params[0].ty)?;
        value.cast::<DynamicPropertyHolder>()?.get().0.clone()
    }};
}

/// The curves object a property gave Python ([`CURVES`]).
#[derive(Debug)]
enum Curves {
    Rgb(Weak<DynamicPropertyGradingRgbCurveImpl>, RgbCurveRc),
    Hue(Weak<DynamicPropertyGradingHueCurveImpl>, HueCurveRc),
}

impl Curves {
    /// Whether the property it belongs to is gone.
    fn dead(&self) -> bool {
        match self {
            Curves::Rgb(w, _) => w.strong_count() == 0,
            Curves::Hue(w, _) => w.strong_count() == 0,
        }
    }
}

/// The curves objects the properties gave Python, while the library hasn't replaced their
/// curves: each property's by its address (and a weak reference to it, so that an entry of a
/// property gone is dropped, and never mistaken for a new property at the same address).
static CURVES: Mutex<Vec<Curves>> = Mutex::new(Vec::new());

fn curves() -> std::sync::MutexGuard<'static, Vec<Curves>> {
    let mut list = CURVES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    list.retain(|c| !c.dead());
    list
}

/// The RGB curves object of the property `p`: the one Python holds, or a new one of its curves.
fn rgb_curves(p: &Arc<DynamicPropertyGradingRgbCurveImpl>) -> RgbCurveRc {
    let mut list = curves();
    let weak = Arc::downgrade(p);
    for c in list.iter() {
        if let Curves::Rgb(w, rc) = c
            && w.ptr_eq(&weak)
        {
            return rc.clone();
        }
    }
    let rc = Shared::new(RgbCurveNode::new(&p.get_value()));
    list.push(Curves::Rgb(weak, rc.clone()));
    rc
}

/// The hue curves object of the property `p` (see [`rgb_curves`]).
fn hue_curves(p: &Arc<DynamicPropertyGradingHueCurveImpl>) -> HueCurveRc {
    let mut list = curves();
    let weak = Arc::downgrade(p);
    for c in list.iter() {
        if let Curves::Hue(w, rc) = c
            && w.ptr_eq(&weak)
        {
            return rc.clone();
        }
    }
    let rc = Shared::new(HueCurveNode::new(&p.get_value()));
    list.push(Curves::Hue(weak, rc.clone()));
    rc
}

/// The library replaced the curves of the property at `address`: the object Python holds is no
/// longer them.
fn forget(address: *const ()) {
    curves().retain(|c| match c {
        Curves::Rgb(w, _) => w.as_ptr() as *const () != address,
        Curves::Hue(w, _) => w.as_ptr() as *const () != address,
    });
}

// .def("getType", &PyDynamicProperty::getType) (PyDynamicProperty.cpp:16-17 @ v2.5.2;
// PyDynamicProperty.h:62-65).
static GET_TYPE: Function = method_def!("getType", [SELF], DYNAMIC_PROPERTY_TYPE, |py, o, a, _| {
    let prop = this_prop!(o, a);
    enum_out(py, DYNAMIC_PROPERTY_TYPE, prop.base().get_type() as i64)
});

// Each getter and setter below starts with `DynamicPropertyValue::As...(m_prop)`, which throws
// "Dynamic property value is not a ..." for a property of another kind
// (src/OpenColorIO/DynamicProperty.cpp:18-53 @ v2.5.2; the library's `as_...`): the binding's
// own "Invalid dynamic property type" errors (PyDynamicProperty.h:74, 84, ...) are never
// reached.

// .def("getDouble", &PyDynamicProperty::getDouble) (PyDynamicProperty.cpp:18-19 @ v2.5.2;
// PyDynamicProperty.h:67-75).
static GET_DOUBLE: Function = method_def!("getDouble", [SELF], "float", |py, o, a, _| {
    let p = py_result(py, this_prop!(o, a).as_double())?;
    ok(py, p.get_value())
});

// .def("setDouble", &PyDynamicProperty::setDouble, "val"_a) (PyDynamicProperty.cpp:20-21 @
// v2.5.2; PyDynamicProperty.h:77-85).
static SET_DOUBLE: Function = method_def!(
    "setDouble",
    [SELF, Param::new("val", FLOAT)],
    "None",
    |py, o, a, convert| {
        let prop = this_prop!(o, a);
        let v = crate::arg!(a, 1, convert, f64);
        py_result(py, prop.as_double())?.set_value(v);
        none(py)
    }
);

// .def("getGradingPrimary", &PyDynamicProperty::getGradingPrimary) (PyDynamicProperty.cpp:22-23
// @ v2.5.2; PyDynamicProperty.h:87-95): a `const GradingPrimary &`, copied.
static GET_PRIMARY: Function = method_def!(
    "getGradingPrimary",
    [SELF],
    GRADING_PRIMARY,
    |py, o, a, _| {
        let p = py_result(py, this_prop!(o, a).as_grading_primary())?;
        out(py, GRADING_PRIMARY, &Shared::new(p.get_value()))
    }
);

// .def("setGradingPrimary", &PyDynamicProperty::setGradingPrimary, "val"_a)
// (PyDynamicProperty.cpp:24-25 @ v2.5.2; PyDynamicProperty.h:97-105).
static SET_PRIMARY: Function = method_def!(
    "setGradingPrimary",
    [SELF, Param::new("val", GRADING_PRIMARY)],
    "None",
    |py, o, a, _| {
        let prop = this_prop!(o, a);
        let Some(v) = holder::<GradingPrimary>(&a[1], o.params[1].ty)? else {
            return Ok(None);
        };
        let v = *v.lock();
        let p = py_result(py, prop.as_grading_primary())?;
        py_result(py, p.set_value(&v))?;
        none(py)
    }
);

/// The port's error where the wheel validates a null curve through its pointer (U-167).
fn null_curves(class: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{class}: None for a curve, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-167)"
    ))
}

// .def("getGradingRGBCurve", &PyDynamicProperty::getGradingRGBCurve) (PyDynamicProperty.cpp:
// 26-27 @ v2.5.2; PyDynamicProperty.h:107-115): the property's own curves.
static GET_RGB_CURVE: Function = method_def!(
    "getGradingRGBCurve",
    [SELF],
    GRADING_RGB_CURVE,
    |py, o, a, _| {
        let p = py_result(py, this_prop!(o, a).as_grading_rgb_curve())?;
        out(py, GRADING_RGB_CURVE, &rgb_curves(&p))
    }
);

// .def("setGradingRGBCurve", &PyDynamicProperty::setGradingRGBCurve, "val"_a)
// (PyDynamicProperty.cpp:28-29 @ v2.5.2; PyDynamicProperty.h:117-125): `setValue` validates the
// curves, then holds a copy (DynamicProperty.cpp:216-223).
static SET_RGB_CURVE: Function = method_def!(
    "setGradingRGBCurve",
    [SELF, Param::new("val", GRADING_RGB_CURVE)],
    "None",
    |py, o, a, convert| {
        let prop = this_prop!(o, a);
        let Some(v) = holder_arg::<RgbCurveNode>(&a[1], o.params[1].ty, convert)? else {
            return Ok(None);
        };
        let p = py_result(py, prop.as_grading_rgb_curve())?;
        let value = rgb_curve_value(&v.ok_or_else(|| null_curves(GRADING_RGB_CURVE))?);
        if value.validate().is_ok() {
            forget(Arc::as_ptr(&p) as *const ());
        }
        py_result(py, p.set_value(&value))?;
        none(py)
    }
);

// .def("getGradingHueCurve", &PyDynamicProperty::getGradingHueCurve) (PyDynamicProperty.cpp:
// 30-31 @ v2.5.2; PyDynamicProperty.h:127-135): the property's own curves.
static GET_HUE_CURVE: Function = method_def!(
    "getGradingHueCurve",
    [SELF],
    GRADING_HUE_CURVE,
    |py, o, a, _| {
        let p = py_result(py, this_prop!(o, a).as_grading_hue_curve())?;
        out(py, GRADING_HUE_CURVE, &hue_curves(&p))
    }
);

// .def("setGradingHueCurve", &PyDynamicProperty::setGradingHueCurve, "val"_a)
// (PyDynamicProperty.cpp:32-33 @ v2.5.2; PyDynamicProperty.h:137-145; DynamicProperty.cpp:
// 315-322).
static SET_HUE_CURVE: Function = method_def!(
    "setGradingHueCurve",
    [SELF, Param::new("val", GRADING_HUE_CURVE)],
    "None",
    |py, o, a, convert| {
        let prop = this_prop!(o, a);
        let Some(v) = holder_arg::<HueCurveNode>(&a[1], o.params[1].ty, convert)? else {
            return Ok(None);
        };
        let p = py_result(py, prop.as_grading_hue_curve())?;
        let value = hue_curve_value(&v.ok_or_else(|| null_curves(GRADING_HUE_CURVE))?);
        if value.validate().is_ok() {
            forget(Arc::as_ptr(&p) as *const ());
        }
        py_result(py, p.set_value(&value))?;
        none(py)
    }
);

// .def("getGradingTone", &PyDynamicProperty::getGradingTone) (PyDynamicProperty.cpp:34-35 @
// v2.5.2; PyDynamicProperty.h:147-155): a `const GradingTone &`, copied.
static GET_TONE: Function = method_def!("getGradingTone", [SELF], GRADING_TONE, |py, o, a, _| {
    let p = py_result(py, this_prop!(o, a).as_grading_tone())?;
    out(py, GRADING_TONE, &Shared::new(p.get_value()))
});

// .def("setGradingTone", &PyDynamicProperty::setGradingTone, "val"_a) (PyDynamicProperty.cpp:
// 36-37 @ v2.5.2; PyDynamicProperty.h:157-165).
static SET_TONE: Function = method_def!(
    "setGradingTone",
    [SELF, Param::new("val", GRADING_TONE)],
    "None",
    |py, o, a, _| {
        let prop = this_prop!(o, a);
        let Some(v) = holder::<GradingTone>(&a[1], o.params[1].ty)? else {
            return Ok(None);
        };
        let v = *v.lock();
        let p = py_result(py, prop.as_grading_tone())?;
        py_result(py, p.set_value(&v))?;
        none(py)
    }
);

/// Port of `bindPyDynamicProperty` (src/bindings/python/PyDynamicProperty.cpp:10-38 @ v2.5.2),
/// in its order.
pub(crate) fn bind_py_dynamic_property(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), DYNAMIC_PROPERTY)?;
    for f in [
        &GET_TYPE,
        &GET_DOUBLE,
        &SET_DOUBLE,
        &GET_PRIMARY,
        &SET_PRIMARY,
        &GET_RGB_CURVE,
        &SET_RGB_CURVE,
        &GET_HUE_CURVE,
        &SET_HUE_CURVE,
        &GET_TONE,
        &SET_TONE,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
