// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Dynamic properties: values an application can change after a processor is built, such as
//! the exposure of an `ExposureContrastTransform`. A port of
//! `src/OpenColorIO/DynamicProperty.h` and `DynamicProperty.cpp` @ v2.5.2, and of the public
//! `DynamicProperty` and `DynamicPropertyDouble` interfaces
//! (`include/OpenColorIO/OpenColorTransforms.h:766-779, 810-824`).
//!
//! So far: the base (the type and whether the property is dynamic, and equality) and the
//! property that holds a double. The grading properties and `DynamicPropertyValue::As*`
//! come with the grading ops (Phase 5).
//!
//! Upstream shares a property between the ops and processors that use it through a
//! `std::shared_ptr`, and the application changes it through the same pointer. Here the
//! handle is an [`Arc`], and the mutable state is atomic (PLAN.md §9), so a property can be
//! changed through a shared handle. A renderer reads the value once per `apply`.

use crate::open_color_types::DynamicPropertyType;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
}

impl DynamicPropertyRcPtr {
    /// The property's type and dynamic state.
    pub fn base(&self) -> &DynamicPropertyImpl {
        match self {
            DynamicPropertyRcPtr::Double(p) => p,
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
        }
    }
}

impl From<DynamicPropertyDoubleImplRcPtr> for DynamicPropertyRcPtr {
    fn from(p: DynamicPropertyDoubleImplRcPtr) -> Self {
        DynamicPropertyRcPtr::Double(p)
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
