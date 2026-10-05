// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Dynamic properties: values an application can change after a processor is built, such as
//! the exposure of an `ExposureContrastTransform`. A port of
//! `src/OpenColorIO/DynamicProperty.h` and `DynamicProperty.cpp` @ v2.5.2, and of the public
//! `DynamicProperty` and `DynamicPropertyDouble` interfaces
//! (`include/OpenColorIO/OpenColorTransforms.h:766-779, 810-824`).
//!
//! So far: the base (the type and whether the property is dynamic, and equality), the
//! property that holds a double, and the GradingRGBCurve property (Phase 2, WP 2.6). The other
//! grading properties and `DynamicPropertyValue::As*` come with the grading ops (Phase 5).
//!
//! Upstream shares a property between the ops and processors that use it through a
//! `std::shared_ptr`, and the application changes it through the same pointer. Here the
//! handle is an [`Arc`], and the mutable state is atomic (PLAN.md §9), so a property can be
//! changed through a shared handle. A renderer reads the value once per `apply`.

use crate::exception::Result;
use crate::open_color_types::{DynamicPropertyType, RgbCurveType};
use crate::ops::gradingrgbcurve::grading_b_spline_curve::KnotsCoefs;
use crate::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// What every dynamic property holds: its type, and whether it is dynamic.
///
/// Port of `DynamicPropertyImpl` (src/OpenColorIO/DynamicProperty.h:19-64 @ v2.5.2), less
/// `equals`, which needs the value and so is on each kind of property
/// ([`DynamicPropertyDoubleImpl::equals`], [`DynamicPropertyRcPtr::equals`]).
#[derive(Debug)]
pub struct DynamicPropertyImpl {
    type_: DynamicPropertyType,
    is_dynamic: AtomicBool,
}

impl DynamicPropertyImpl {
    /// Port of `DynamicPropertyImpl::DynamicPropertyImpl(DynamicPropertyType, bool)`
    /// (DynamicProperty.cpp:63-67 @ v2.5.2).
    pub fn new(type_: DynamicPropertyType, dynamic: bool) -> Self {
        DynamicPropertyImpl {
            type_,
            is_dynamic: AtomicBool::new(dynamic),
        }
    }

    /// Port of `DynamicPropertyImpl::getType` (DynamicProperty.h:27-30 @ v2.5.2).
    pub fn get_type(&self) -> DynamicPropertyType {
        self.type_
    }

    /// Port of `DynamicPropertyImpl::isDynamic` (DynamicProperty.h:32-35 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.is_dynamic.load(Ordering::Relaxed)
    }

    /// Port of `DynamicPropertyImpl::makeDynamic` (DynamicProperty.h:37-40 @ v2.5.2).
    pub fn make_dynamic(&self) {
        self.is_dynamic.store(true, Ordering::Relaxed);
    }

    /// Port of `DynamicPropertyImpl::makeNonDynamic` (DynamicProperty.h:42-45 @ v2.5.2).
    pub fn make_non_dynamic(&self) {
        self.is_dynamic.store(false, Ordering::Relaxed);
    }
}

/// A shared dynamic property that holds a double.
///
/// Port of `DynamicPropertyDoubleImplRcPtr` (src/OpenColorIO/DynamicProperty.h:70 @ v2.5.2).
pub type DynamicPropertyDoubleImplRcPtr = Arc<DynamicPropertyDoubleImpl>;

/// A dynamic property that holds a double: exposure, contrast or gamma. It dereferences to its
/// [`DynamicPropertyImpl`], as the C++ class derives from it.
///
/// Port of `DynamicPropertyDoubleImpl` (src/OpenColorIO/DynamicProperty.h:72-85 @ v2.5.2) and
/// of the `DynamicPropertyDouble` interface it implements
/// (include/OpenColorIO/OpenColorTransforms.h:810-824).
pub struct DynamicPropertyDoubleImpl {
    base: DynamicPropertyImpl,
    /// The value's bits, so that any double, NaN payloads included, is kept as it is.
    value: AtomicU64,
}

impl std::ops::Deref for DynamicPropertyDoubleImpl {
    type Target = DynamicPropertyImpl;

    fn deref(&self) -> &DynamicPropertyImpl {
        &self.base
    }
}

impl fmt::Debug for DynamicPropertyDoubleImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicPropertyDoubleImpl")
            .field("type", &self.get_type())
            .field("is_dynamic", &self.is_dynamic())
            .field("value", &self.get_value())
            .finish()
    }
}

impl DynamicPropertyDoubleImpl {
    /// Port of `DynamicPropertyDoubleImpl::DynamicPropertyDoubleImpl` (DynamicProperty.cpp:
    /// 127-133 @ v2.5.2).
    pub fn new(type_: DynamicPropertyType, value: f64, dynamic: bool) -> Self {
        DynamicPropertyDoubleImpl {
            base: DynamicPropertyImpl::new(type_, dynamic),
            value: AtomicU64::new(value.to_bits()),
        }
    }

    /// Port of `DynamicPropertyDoubleImpl::getValue` (DynamicProperty.h:78 @ v2.5.2).
    pub fn get_value(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }

    /// Port of `DynamicPropertyDoubleImpl::setValue` (DynamicProperty.h:79 @ v2.5.2).
    pub fn set_value(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }

    /// A new property with the same type, value and dynamic state.
    ///
    /// Port of `DynamicPropertyDoubleImpl::createEditableCopy` (DynamicProperty.cpp:135-138 @
    /// v2.5.2).
    pub fn create_editable_copy(&self) -> DynamicPropertyDoubleImplRcPtr {
        Arc::new(DynamicPropertyDoubleImpl::new(
            self.get_type(),
            self.get_value(),
            self.is_dynamic(),
        ))
    }

    /// Whether two double properties are equal, as the optimizer sees it: a property equals
    /// itself; two different non-dynamic properties of the same type are equal when their
    /// values compare equal; and a dynamic property equals no other property, because its
    /// value can change once the processor is in use.
    ///
    /// Upstream's comment says that two dynamic properties are equal; its code, ported here,
    /// says they are not (DynamicProperty.h:47-55, DynamicProperty.cpp:115-120).
    ///
    /// Port of `DynamicPropertyImpl::equals` (src/OpenColorIO/DynamicProperty.cpp:69-125 @
    /// v2.5.2) for two double properties.
    pub fn equals(&self, rhs: &DynamicPropertyDoubleImpl) -> bool {
        if std::ptr::eq(self, rhs) {
            return true;
        }

        if self.is_dynamic() == rhs.is_dynamic() && self.get_type() == rhs.get_type() {
            if !self.is_dynamic() {
                // Both not dynamic, same value or not.
                return match self.get_type() {
                    DynamicPropertyType::Contrast
                    | DynamicPropertyType::Exposure
                    | DynamicPropertyType::Gamma => self.get_value() == rhs.get_value(),
                    // Upstream casts both to the grading property of the type, which a
                    // double property is not.
                    DynamicPropertyType::GradingPrimary
                    | DynamicPropertyType::GradingRgbCurve
                    | DynamicPropertyType::GradingTone
                    | DynamicPropertyType::GradingHueCurve => false,
                };
            }
            // Both dynamic, may not be same (this is used for processor optimization do not
            // assume they will always have same values even if it is currently the case).
            return false;
        }

        // One dynamic, not the other or different types.
        false
    }
}

/// Port of `operator==(const DynamicProperty &, const DynamicProperty &)`
/// (src/OpenColorIO/DynamicProperty.cpp:50-61 @ v2.5.2) for two double properties: different
/// types are not equal; otherwise [`DynamicPropertyDoubleImpl::equals`].
impl PartialEq for DynamicPropertyDoubleImpl {
    fn eq(&self, other: &Self) -> bool {
        self.get_type() == other.get_type() && self.equals(other)
    }
}

/// A shared GradingRGBCurve dynamic property.
///
/// Port of `DynamicPropertyGradingRGBCurveImplRcPtr` (src/OpenColorIO/DynamicProperty.h:139 @
/// v2.5.2).
pub type DynamicPropertyGradingRgbCurveImplRcPtr = Arc<DynamicPropertyGradingRgbCurveImpl>;

/// The value of a GradingRGBCurve property and the knots and coefficients computed from it.
#[derive(Debug, Clone)]
pub struct RgbCurveState {
    /// `m_gradingRGBCurve`.
    pub value: GradingRgbCurve,
    /// `m_knotsCoefs`: the four curves' knots and coefficients.
    pub knots_coefs: KnotsCoefs,
}

/// The dynamic property of a GradingRGBCurve op: its four curves, and their knots and
/// coefficients, which the renderers evaluate. It dereferences to its [`DynamicPropertyImpl`].
/// The value and the knots change together under a lock, so a renderer reads a consistent
/// pair ([`DynamicPropertyGradingRgbCurveImpl::state`]); upstream reads them unlocked.
///
/// Port of `DynamicPropertyGradingRGBCurveImpl` (src/OpenColorIO/DynamicProperty.h:141-174,
/// DynamicProperty.cpp:202-297 @ v2.5.2) and of the `DynamicPropertyGradingRGBCurve`
/// interface (include/OpenColorIO/OpenColorTransforms.h).
pub struct DynamicPropertyGradingRgbCurveImpl {
    base: DynamicPropertyImpl,
    state: RwLock<RgbCurveState>,
}

impl std::ops::Deref for DynamicPropertyGradingRgbCurveImpl {
    type Target = DynamicPropertyImpl;

    fn deref(&self) -> &DynamicPropertyImpl {
        &self.base
    }
}

impl fmt::Debug for DynamicPropertyGradingRgbCurveImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicPropertyGradingRgbCurveImpl")
            .field("is_dynamic", &self.is_dynamic())
            .field("state", &*self.state())
            .finish()
    }
}

impl DynamicPropertyGradingRgbCurveImpl {
    /// A property holding a copy of `value`, not validated, with its knots and coefficients;
    /// the errors of fitting the curves (too many control points).
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::DynamicPropertyGradingRGBCurveImpl`
    /// (src/OpenColorIO/DynamicProperty.cpp:202-209 @ v2.5.2).
    pub fn new(value: &GradingRgbCurve, dynamic: bool) -> Result<Self> {
        let mut state = RgbCurveState {
            value: value.clone(),
            knots_coefs: KnotsCoefs::new(4),
        };
        // Convert control points from the UI into knots and coefficients for the apply.
        Self::precompute(&mut state)?;
        Ok(DynamicPropertyGradingRgbCurveImpl {
            base: DynamicPropertyImpl::new(DynamicPropertyType::GradingRgbCurve, dynamic),
            state: RwLock::new(state),
        })
    }

    /// A property holding `state` as it is: the curves and their knots, not fitted again.
    pub fn from_state(state: RgbCurveState, dynamic: bool) -> Self {
        DynamicPropertyGradingRgbCurveImpl {
            base: DynamicPropertyImpl::new(DynamicPropertyType::GradingRgbCurve, dynamic),
            state: RwLock::new(state),
        }
    }

    /// The value and its knots and coefficients, read-locked.
    pub fn state(&self) -> RwLockReadGuard<'_, RgbCurveState> {
        self.state.read().unwrap_or_else(|e| e.into_inner())
    }

    fn state_mut(&self) -> RwLockWriteGuard<'_, RgbCurveState> {
        self.state.write().unwrap_or_else(|e| e.into_inner())
    }

    /// A copy of the curves.
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::getValue` (DynamicProperty.cpp:211-214 @
    /// v2.5.2).
    pub fn get_value(&self) -> GradingRgbCurve {
        self.state().value.clone()
    }

    /// Validates `value`, then holds a copy of it and computes its knots and coefficients.
    /// When fitting the curves fails, the property keeps the new value and the knots of the
    /// curves fitted before the failure, as upstream's does.
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::setValue` (DynamicProperty.cpp:216-223 @
    /// v2.5.2).
    pub fn set_value(&self, value: &GradingRgbCurve) -> Result<()> {
        value.validate()?;
        let mut state = self.state_mut();
        state.value = value.clone();
        // Convert control points from the UI into knots and coefficients for the apply.
        Self::precompute(&mut state)
    }

    /// Port of `DynamicPropertyGradingRGBCurveImpl::getLocalBypass` (DynamicProperty.cpp:
    /// 225-228 @ v2.5.2).
    pub fn get_local_bypass(&self) -> bool {
        self.state().knots_coefs.local_bypass
    }

    /// Port of `DynamicPropertyGradingRGBCurveImpl::getNumKnots` (DynamicProperty.cpp:230-233
    /// @ v2.5.2).
    pub fn get_num_knots(&self) -> i32 {
        self.state().knots_coefs.num_knots
    }

    /// Port of `DynamicPropertyGradingRGBCurveImpl::getNumCoefs` (DynamicProperty.cpp:235-238
    /// @ v2.5.2).
    pub fn get_num_coefs(&self) -> i32 {
        self.state().knots_coefs.num_coefs
    }

    /// The offset and count of each of the four curves.
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::GetNumOffsetValues` (DynamicProperty.h:154
    /// @ v2.5.2).
    pub const NUM_OFFSET_VALUES: i32 = 8;

    /// Port of `DynamicPropertyGradingRGBCurveImpl::GetMaxKnots` (DynamicProperty.cpp:260-263
    /// @ v2.5.2).
    pub const MAX_KNOTS: u32 = KnotsCoefs::MAX_NUM_KNOTS as u32;

    /// Port of `DynamicPropertyGradingRGBCurveImpl::GetMaxCoefs` (DynamicProperty.cpp:265-268
    /// @ v2.5.2).
    pub const MAX_COEFS: u32 = KnotsCoefs::MAX_NUM_COEFS as u32;

    /// Fits the four curves, in order, packing their knots and coefficients.
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::precompute` (DynamicProperty.cpp:270-290
    /// @ v2.5.2). Its "unexpected curve implementation" can't happen.
    fn precompute(state: &mut RgbCurveState) -> Result<()> {
        let knots_coefs = &mut state.knots_coefs;
        knots_coefs.local_bypass = false;
        knots_coefs.num_coefs = 0;
        knots_coefs.num_knots = 0;

        // Compute knots and coefficients for each control point and pack all knots and coefs
        // of all curves in one knots array and one coef array, using an offset array to find
        // specific curve data.
        for (c, curve) in RgbCurveType::CURVES.iter().zip(state.value.curves()) {
            curve.compute_knots_and_coefs(knots_coefs, *c as usize, false)?;
        }
        if knots_coefs.num_knots <= 0 {
            knots_coefs.local_bypass = true;
        }
        Ok(())
    }

    /// A new property with the same value, knots and dynamic state.
    ///
    /// Port of `DynamicPropertyGradingRGBCurveImpl::createEditableCopy` (DynamicProperty.cpp:
    /// 292-297 @ v2.5.2): it makes a property of the value, which fits the curves again, then
    /// copies the knots over.
    pub fn create_editable_copy(&self) -> Result<DynamicPropertyGradingRgbCurveImplRcPtr> {
        let state = self.state().clone();
        let res = DynamicPropertyGradingRgbCurveImpl::new(&state.value, self.is_dynamic())?;
        res.state_mut().knots_coefs = state.knots_coefs;
        Ok(Arc::new(res))
    }

    /// Whether two RGB curve properties are equal, as the optimizer sees it: a property
    /// equals itself; two non-dynamic properties are equal when their curves are; a dynamic
    /// property equals no other property.
    ///
    /// Port of `DynamicPropertyImpl::equals` (src/OpenColorIO/DynamicProperty.cpp:69-125 @
    /// v2.5.2) for two GradingRGBCurve properties.
    pub fn equals(&self, rhs: &DynamicPropertyGradingRgbCurveImpl) -> bool {
        if std::ptr::eq(self, rhs) {
            return true;
        }
        if self.is_dynamic() == rhs.is_dynamic() {
            if !self.is_dynamic() {
                // Both not dynamic, same value or not.
                return self.state().value == rhs.state().value;
            }
            // Both dynamic, may not be same.
            return false;
        }
        // One dynamic, not the other.
        false
    }
}

/// Port of `operator==(const DynamicProperty &, const DynamicProperty &)`
/// (src/OpenColorIO/DynamicProperty.cpp:50-61 @ v2.5.2) for two RGB curve properties.
impl PartialEq for DynamicPropertyGradingRgbCurveImpl {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

/// A shared handle to a dynamic property of any kind: what `Processor::getDynamicProperty`
/// returns. `==` compares the properties, as upstream's `operator==` on `DynamicProperty`
/// references does.
///
/// Port of `DynamicPropertyRcPtr` (`std::shared_ptr<DynamicProperty>`,
/// include/OpenColorIO/OpenColorTypes.h:156) and of the `DynamicProperty` interface
/// (include/OpenColorIO/OpenColorTransforms.h:766-779 @ v2.5.2). Upstream throws
/// "Unknown DynamicProperty implementation." when `operator==` meets a class of its caller's;
/// this enum has no such variant.
#[derive(Debug, Clone)]
pub enum DynamicPropertyRcPtr {
    /// Exposure, contrast or gamma.
    Double(DynamicPropertyDoubleImplRcPtr),
    /// A GradingRGBCurve's curves.
    GradingRgbCurve(DynamicPropertyGradingRgbCurveImplRcPtr),
}

impl DynamicPropertyRcPtr {
    /// The property's type and dynamic state.
    pub fn base(&self) -> &DynamicPropertyImpl {
        match self {
            DynamicPropertyRcPtr::Double(p) => p,
            DynamicPropertyRcPtr::GradingRgbCurve(p) => p,
        }
    }

    /// Port of `DynamicProperty::getType` (include/OpenColorIO/OpenColorTransforms.h:769 @
    /// v2.5.2).
    pub fn get_type(&self) -> DynamicPropertyType {
        self.base().get_type()
    }

    /// Port of `DynamicPropertyImpl::equals` (src/OpenColorIO/DynamicProperty.cpp:69-125 @
    /// v2.5.2): properties of different kinds are never equal.
    pub fn equals(&self, rhs: &DynamicPropertyRcPtr) -> bool {
        match (self, rhs) {
            (DynamicPropertyRcPtr::Double(l), DynamicPropertyRcPtr::Double(r)) => l.equals(r),
            (
                DynamicPropertyRcPtr::GradingRgbCurve(l),
                DynamicPropertyRcPtr::GradingRgbCurve(r),
            ) => l.equals(r),
            (DynamicPropertyRcPtr::Double(_), _)
            | (DynamicPropertyRcPtr::GradingRgbCurve(_), _) => false,
        }
    }
}

impl From<DynamicPropertyDoubleImplRcPtr> for DynamicPropertyRcPtr {
    fn from(p: DynamicPropertyDoubleImplRcPtr) -> Self {
        DynamicPropertyRcPtr::Double(p)
    }
}

impl From<DynamicPropertyGradingRgbCurveImplRcPtr> for DynamicPropertyRcPtr {
    fn from(p: DynamicPropertyGradingRgbCurveImplRcPtr) -> Self {
        DynamicPropertyRcPtr::GradingRgbCurve(p)
    }
}

/// Port of `operator==(const DynamicProperty &, const DynamicProperty &)`
/// (src/OpenColorIO/DynamicProperty.cpp:50-61 @ v2.5.2).
impl PartialEq for DynamicPropertyRcPtr {
    fn eq(&self, other: &Self) -> bool {
        if self.get_type() != other.get_type() {
            return false;
        }
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "dynamic_property_tests.rs"]
mod tests;
