// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The registry of built-in transforms: a port of
//! `src/OpenColorIO/transforms/builtins/BuiltinTransformRegistry.h` and
//! `BuiltinTransformRegistry.cpp` @ v2.5.2.
//!
//! Every entry upstream registers is here, with its style and description in upstream's order,
//! so built-in transforms can be named, printed and compared before their ops exist. The
//! creators of the entries whose ops are not ported yet return an error ([`not_ported_yet`]);
//! each builder chunk replaces its entries' creators (WP 3.2e-g, `p3-after-p2`).

use std::fmt;
use std::sync::{Arc, OnceLock};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::c_str;

use crate::transforms::builtins::{
    aces, apple_cameras, arri_cameras, canon_cameras, displays, panasonic_cameras, red_cameras,
    sony_cameras,
};

/// A built-in transform's op creator: appends its ops, forward.
///
/// Port of `BuiltinTransformRegistryImpl::OpCreator` (BuiltinTransformRegistry.h:21 @ v2.5.2),
/// a `std::function<void(OpRcPtrVec &)>`; it returns an error here, for the entries whose ops
/// are not ported yet.
pub(crate) type OpCreator = Arc<dyn Fn(&mut OpVec) -> Result<()> + Send + Sync>;

/// The creator of an entry whose ops are not ported yet: it returns an error naming the style.
pub(crate) fn not_ported_yet(style: &'static [u8]) -> OpCreator {
    Arc::new(move |_ops: &mut OpVec| {
        Err(Exception::new(format!(
            "BuiltinTransform: the ops of '{}' are not ported yet.",
            String::from_utf8_lossy(style)
        )))
    })
}

/// One built-in transform: its style, its description and its op creator.
///
/// Port of `BuiltinTransformRegistryImpl::BuiltinData` (BuiltinTransformRegistry.h:23-49 @
/// v2.5.2).
#[derive(Clone)]
struct BuiltinData {
    /// `m_style`: the built-in transform style.
    style: Vec<u8>,
    /// `m_description`: the optional built-in transform description.
    description: Vec<u8>,
    /// `m_creator`: functor to create the op(s).
    creator: OpCreator,
}

impl BuiltinData {
    /// Port of `BuiltinData::BuiltinData(const char *, const char *, OpCreator)`
    /// (BuiltinTransformRegistry.h:25-30 @ v2.5.2): the strings up to their first NUL, a null
    /// description (`None`) empty.
    fn new(style: &[u8], description: Option<&[u8]>, creator: OpCreator) -> BuiltinData {
        BuiltinData {
            style: c_str(style).to_vec(),
            description: description.map_or_else(Vec::new, |d| c_str(d).to_vec()),
            creator,
        }
    }
}

impl fmt::Debug for BuiltinData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BuiltinData")
            .field("style", &String::from_utf8_lossy(&self.style))
            .field("description", &String::from_utf8_lossy(&self.description))
            .finish_non_exhaustive()
    }
}

/// The registry of built-in transforms: OCIO's list of transforms it knows how to build, each
/// named by a style (case-insensitive) and described in a sentence.
///
/// [`BuiltinTransformRegistry::get`] returns the global registry, which holds every built-in
/// transform of OCIO 2.5.2 (98), in upstream's order.
///
/// Port of `BuiltinTransformRegistry` (include/OpenColorIO/OpenColorIO.h:3963-3985 @ v2.5.2)
/// and `BuiltinTransformRegistryImpl` (BuiltinTransformRegistry.h:19-70 @ v2.5.2), its only
/// implementation.
#[derive(Debug, Clone, Default)]
pub struct BuiltinTransformRegistry {
    /// `m_builtins`.
    builtins: Vec<BuiltinData>,
}

impl BuiltinTransformRegistry {
    /// The global registry, with every built-in transform registered the first time it is
    /// asked for.
    ///
    /// Port of `BuiltinTransformRegistry::Get` (BuiltinTransformRegistry.cpp:36-47 @ v2.5.2).
    #[doc(alias = "Get")]
    pub fn get() -> &'static BuiltinTransformRegistry {
        static GLOBAL_REGISTRY: OnceLock<BuiltinTransformRegistry> = OnceLock::new();
        GLOBAL_REGISTRY.get_or_init(|| {
            let mut registry = BuiltinTransformRegistry::new();
            registry.register_all();
            registry
        })
    }

    /// An empty registry.
    ///
    /// Port of `BuiltinTransformRegistryImpl::BuiltinTransformRegistryImpl`
    /// (BuiltinTransformRegistry.h:53 @ v2.5.2).
    pub(crate) fn new() -> BuiltinTransformRegistry {
        BuiltinTransformRegistry::default()
    }

    /// Adds a built-in transform, or replaces the one whose style is the same ignoring case.
    ///
    /// Port of `BuiltinTransformRegistryImpl::addBuiltin` (BuiltinTransformRegistry.cpp:49-63
    /// @ v2.5.2).
    pub(crate) fn add_builtin(
        &mut self,
        style: &[u8],
        description: Option<&[u8]>,
        creator: OpCreator,
    ) {
        let data = BuiltinData::new(style, Some(description.unwrap_or(b"")), creator);

        for builtin in &mut self.builtins {
            if strcasecmp(&data.style, &builtin.style).is_eq() {
                *builtin = data;
                return;
            }
        }

        self.builtins.push(data);
    }

    /// The number of built-in transforms.
    ///
    /// Port of `BuiltinTransformRegistryImpl::getNumBuiltins` (BuiltinTransformRegistry.cpp:
    /// 65-68 @ v2.5.2).
    #[doc(alias = "getNumBuiltins")]
    pub fn num_builtins(&self) -> usize {
        self.builtins.len()
    }

    /// The style of the built-in transform at `index`, or "Invalid index.".
    ///
    /// Port of `BuiltinTransformRegistryImpl::getBuiltinStyle` (BuiltinTransformRegistry.cpp:
    /// 70-78 @ v2.5.2).
    #[doc(alias = "getBuiltinStyle")]
    pub fn builtin_style(&self, index: usize) -> Result<&[u8]> {
        match self.builtins.get(index) {
            Some(builtin) => Ok(&builtin.style),
            None => Err(Exception::new("Invalid index.")),
        }
    }

    /// The description of the built-in transform at `index` (possibly empty), or "Invalid
    /// index.".
    ///
    /// Port of `BuiltinTransformRegistryImpl::getBuiltinDescription`
    /// (BuiltinTransformRegistry.cpp:80-88 @ v2.5.2).
    #[doc(alias = "getBuiltinDescription")]
    pub fn builtin_description(&self, index: usize) -> Result<&[u8]> {
        match self.builtins.get(index) {
            Some(builtin) => Ok(&builtin.description),
            None => Err(Exception::new("Invalid index.")),
        }
    }

    /// Appends the ops of the built-in transform at `index`, forward, or "Invalid index.".
    ///
    /// Port of `BuiltinTransformRegistryImpl::createOps` (BuiltinTransformRegistry.cpp:90-98 @
    /// v2.5.2).
    pub(crate) fn create_ops(&self, index: usize, ops: &mut OpVec) -> Result<()> {
        match self.builtins.get(index) {
            Some(builtin) => (builtin.creator)(ops),
            None => Err(Exception::new("Invalid index.")),
        }
    }

    /// Registers every built-in transform, in upstream's order: the identity, then ACES, the
    /// cameras (Apple, ARRI, Canon, Panasonic, RED, Sony) and the displays.
    ///
    /// Port of `BuiltinTransformRegistryImpl::registerAll` (BuiltinTransformRegistry.cpp:
    /// 100-122 @ v2.5.2).
    pub(crate) fn register_all(&mut self) {
        self.builtins.clear();

        // The identity's op, an identity matrix (`CreateIdentityMatrixOp`), comes with WP 3.2e.
        self.builtins.push(BuiltinData::new(
            b"IDENTITY",
            Some(b""),
            not_ported_yet(b"IDENTITY"),
        ));

        // ACES support.
        aces::register_all(self);

        // Camera support.
        apple_cameras::register_all(self);
        arri_cameras::register_all(self);
        canon_cameras::register_all(self);
        panasonic_cameras::register_all(self);
        red_cameras::register_all(self);
        sony_cameras::register_all(self);

        // Display support.
        displays::register_all(self);
    }
}

/// Appends the ops of the global registry's built-in transform at `name_index`, in the
/// direction `direction`: inverse ops are the forward ones inverted ([`OpVec::invert`]).
///
/// Port of `CreateBuiltinTransformOps` (BuiltinTransformRegistry.cpp:125-152 @ v2.5.2).
pub(crate) fn create_builtin_transform_ops(
    ops: &mut OpVec,
    name_index: usize,
    direction: TransformDirection,
) -> Result<()> {
    if name_index >= BuiltinTransformRegistry::get().num_builtins() {
        return Err(Exception::new("Invalid built-in transform name."));
    }

    let registry = BuiltinTransformRegistry::get();

    match direction {
        TransformDirection::Forward => {
            registry.create_ops(name_index, ops)?;
        }
        TransformDirection::Inverse => {
            let mut tmp = OpVec::new();
            registry.create_ops(name_index, &mut tmp)?;

            let t = tmp.invert()?;
            let end = ops.len();
            ops.insert(end, &t);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "builtin_transform_registry_tests.rs"]
mod tests;
