// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The look: a port of `src/OpenColorIO/Look.cpp` @ v2.5.2. Its `CollectContextVariables`
//! comes with the look transform's op builder (WP 3.2b).

use std::collections::BTreeMap;
use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::Result;
use ocio_ops::open_color_types::{ColorSpaceDirection, TransformDirection};
use ocio_ops::utils::string_utils::c_str;

use crate::color_space::{get_interchange_attribute, set_interchange_attribute};
use crate::config::Config;
use crate::context::Context;
use crate::context_variable_utils::collect_context_variables;
use crate::transform::{Transform, put_c_str};

/// The interchange attributes a look knows.
///
/// Port of `knownInterchangeNames` (src/OpenColorIO/Look.cpp:14-18 @ v2.5.2).
const KNOWN_INTERCHANGE_NAMES: [&[u8]; 1] = [b"amf_transform_ids"];

/// A look: a named color correction applied in a process color space, with its transform and
/// optionally its own inverse.
///
/// A copy is upstream's `createEditableCopy`: it copies the transforms too.
///
/// Port of `Look` and its `Impl` (include/OpenColorIO/OpenColorIO.h:2393-2447,
/// src/OpenColorIO/Look.cpp:22-194 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct Look {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_processSpace`.
    process_space: Vec<u8>,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_interchangeAttribs`.
    interchange_attribs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// `m_transform`.
    transform: Option<Transform>,
    /// `m_inverseTransform`.
    inverse_transform: Option<Transform>,
}

impl Look {
    /// A look without a name, a process space or transforms.
    ///
    /// Port of `Look::Create` (Look.cpp:22-25 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> Look {
        Look::default()
    }

    /// Port of `Look::getName` (Look.cpp:91-94 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Sets the name, up to its first NUL (a C string).
    ///
    /// Port of `Look::setName` (Look.cpp:96-99 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&mut self, name: impl AsRef<[u8]>) {
        self.name = c_str(name.as_ref()).to_vec();
    }

    /// The color space the look's transform works in.
    ///
    /// Port of `Look::getProcessSpace` (Look.cpp:101-104 @ v2.5.2).
    #[doc(alias = "getProcessSpace")]
    pub fn process_space(&self) -> &[u8] {
        &self.process_space
    }

    /// Port of `Look::setProcessSpace` (Look.cpp:106-109 @ v2.5.2).
    #[doc(alias = "setProcessSpace")]
    pub fn set_process_space(&mut self, process_space: impl AsRef<[u8]>) {
        self.process_space = c_str(process_space.as_ref()).to_vec();
    }

    /// The look's transform, if it is set.
    ///
    /// Port of `Look::getTransform` (Look.cpp:111-114 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }

    /// Sets a copy of `transform` as the look's transform. Upstream dereferences a null
    /// pointer here (docs/improvements.md, U-50), which a reference can't be. The copy is
    /// upstream's `createEditableCopy` ([`Transform::create_editable_copy`]): a transform whose
    /// copy fails (an invalid FixedFunctionTransform, curves that don't validate) is refused
    /// with the copy's error, and the look keeps its transform.
    ///
    /// Port of `Look::setTransform` (Look.cpp:116-119 @ v2.5.2).
    #[doc(alias = "setTransform")]
    pub fn set_transform(&mut self, transform: &Transform) -> Result<()> {
        self.transform = Some(transform.create_editable_copy()?);
        Ok(())
    }

    /// The transform that undoes the look, if it is set.
    ///
    /// Port of `Look::getInverseTransform` (Look.cpp:121-124 @ v2.5.2).
    #[doc(alias = "getInverseTransform")]
    pub fn inverse_transform(&self) -> Option<&Transform> {
        self.inverse_transform.as_ref()
    }

    /// Sets a copy of `transform` as the look's inverse transform (U-50 and the copy as
    /// [`set_transform`](Self::set_transform)).
    ///
    /// Port of `Look::setInverseTransform` (Look.cpp:126-129 @ v2.5.2).
    #[doc(alias = "setInverseTransform")]
    pub fn set_inverse_transform(&mut self, transform: &Transform) -> Result<()> {
        self.inverse_transform = Some(transform.create_editable_copy()?);
        Ok(())
    }

    /// Stores `transform` itself (no copy, so not checked) as the look's transform; `None`
    /// removes it.
    ///
    /// For the Python module: the binding holds the transforms itself, as nodes Python objects
    /// share (upstream's getters return the very `TransformRcPtr` the object holds, through which
    /// Python changes it), and gives the library a copy of them to print or to use. That copy
    /// must not be checked again: upstream doesn't copy the transform it holds when it prints
    /// it, so an invalid FixedFunctionTransform made so through Python prints as it is.
    #[doc(hidden)]
    pub fn set_transform_as_is(&mut self, transform: Option<Transform>) {
        self.transform = transform;
    }

    /// [`set_transform_as_is`](Self::set_transform_as_is) of the inverse transform.
    #[doc(hidden)]
    pub fn set_inverse_transform_as_is(&mut self, transform: Option<Transform>) {
        self.inverse_transform = transform;
    }

    /// Port of `Look::getDescription` (Look.cpp:131-134 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &[u8] {
        &self.description
    }

    /// Port of `Look::setDescription` (Look.cpp:136-139 @ v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&mut self, description: impl AsRef<[u8]>) {
        self.description = c_str(description.as_ref()).to_vec();
    }

    /// The value of the interchange attribute `attr_name` (`amf_transform_ids`, ignoring
    /// case), or empty; "Unknown attribute name '<name>'." for any other name.
    ///
    /// Port of `Look::getInterchangeAttribute` (Look.cpp:141-162 @ v2.5.2).
    #[doc(alias = "getInterchangeAttribute")]
    pub fn interchange_attribute(&self, attr_name: impl AsRef<[u8]>) -> Result<&[u8]> {
        get_interchange_attribute(
            &KNOWN_INTERCHANGE_NAMES,
            &self.interchange_attribs,
            attr_name.as_ref(),
        )
    }

    /// Sets the interchange attribute `attr_name`; an empty value removes it.
    ///
    /// Port of `Look::setInterchangeAttribute` (Look.cpp:164-189 @ v2.5.2).
    #[doc(alias = "setInterchangeAttribute")]
    pub fn set_interchange_attribute(
        &mut self,
        attr_name: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<()> {
        set_interchange_attribute(
            &KNOWN_INTERCHANGE_NAMES,
            &mut self.interchange_attribs,
            attr_name.as_ref(),
            value.as_ref(),
        )
    }

    /// The interchange attributes that are set, by name.
    ///
    /// Port of `Look::getInterchangeAttributes` (Look.cpp:191-194 @ v2.5.2).
    #[doc(alias = "getInterchangeAttributes")]
    pub fn interchange_attributes(&self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        &self.interchange_attribs
    }

    /// Writes the look's text to `os`; the transforms on the same stream.
    ///
    /// Port of `operator<<(std::ostream &, const Look &)` (Look.cpp:273-305 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<Look");
        os.put_str(" name=");
        put_c_str(os, self.name());
        os.put_str(", processSpace=");
        put_c_str(os, self.process_space());

        if !self.description().is_empty() {
            os.put_str(", description=");
            put_c_str(os, self.description());
        }

        for (name, value) in self.interchange_attributes() {
            os.put_str(", ");
            put_c_str(os, name);
            os.put_str("=");
            put_c_str(os, value);
        }

        if let Some(transform) = self.transform() {
            os.put_str(",\n    transform=");
            os.put_str("\n        ");
            transform.write_text(os);
        }

        if let Some(inverse) = self.inverse_transform() {
            os.put_str(",\n    inverseTransform=");
            os.put_str("\n        ");
            inverse.write_text(os);
        }

        os.put_str(">");
    }
}

impl Look {
    /// The look's text, as Python's `repr()` prints it, in bytes: names and descriptions
    /// that aren't UTF-8 pass through unchanged, where `Display` replaces them.
    ///
    /// Port of `operator<<(std::ostream &, const Look &)` (Look.cpp:273-305 @ v2.5.2), on a
    /// new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }
}

/// Whether `look` uses context variables, which it adds to `used_context_vars`: those of the
/// transform applied in the direction `direction` (its own, else the other one inverted), and
/// those of its process space's transforms.
///
/// Port of `CollectContextVariables(const Config &, const Context &, TransformDirection, const
/// Look &, ContextRcPtr &)` (src/OpenColorIO/Look.cpp:196-271 @ v2.5.2).
pub(crate) fn collect_look_context_variables(
    config: &Config,
    context: &Context,
    direction: TransformDirection,
    look: &Look,
    used_context_vars: &mut Context,
) -> Result<bool> {
    let mut found_context_vars = false;

    let (own, other) = match direction {
        TransformDirection::Forward => (look.transform(), look.inverse_transform()),
        TransformDirection::Inverse => (look.inverse_transform(), look.transform()),
    };
    if let Some(tr) = own.or(other)
        && collect_context_variables(config, context, tr, used_context_vars)?
    {
        found_context_vars = true;
    }

    if let Some(cs) = config.color_space(look.process_space()) {
        for dir in [
            ColorSpaceDirection::ToReference,
            ColorSpaceDirection::FromReference,
        ] {
            if let Some(tr) = cs.transform(dir)
                && collect_context_variables(config, context, tr, used_context_vars)?
            {
                found_context_vars = true;
            }
        }
    }

    Ok(found_context_vars)
}

impl fmt::Display for Look {
    /// [`Look::to_bytes`], what isn't UTF-8 replaced by U+FFFD.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
mod as_is_tests {
    use super::*;

    /// Each transform is stored as it is, where the setters' copy refuses it; `None` removes
    /// it.
    #[test]
    fn set_transform_as_is_stores_without_checking() {
        let ff = crate::color_space::invalid_fixed_function();
        let mut look = Look::new();
        assert!(look.set_transform(&ff).is_err());
        assert!(look.set_inverse_transform(&ff).is_err());
        look.set_transform_as_is(Some(ff.clone()));
        assert!(
            matches!(look.transform(), Some(Transform::FixedFunction(t)) if t.params() == vec![1.0])
        );
        assert!(look.inverse_transform().is_none());
        look.set_inverse_transform_as_is(Some(ff));
        assert!(
            matches!(look.inverse_transform(), Some(Transform::FixedFunction(t)) if t.params() == vec![1.0])
        );
        look.set_transform_as_is(None);
        assert!(look.transform().is_none());
        assert!(look.inverse_transform().is_some());
        look.set_inverse_transform_as_is(None);
        assert!(look.inverse_transform().is_none());
    }
}
