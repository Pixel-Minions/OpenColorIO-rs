// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The context: a port of `src/OpenColorIO/Context.cpp` @ v2.5.2, as far as the transforms need
//! it so far: none of its state. Its search paths, working directory, string variables and
//! environment come with the config (Phase 3).

/// The context in which a config resolves file paths and variables.
///
/// Port of `Context` (include/OpenColorIO/OpenColorIO.h, src/OpenColorIO/Context.cpp @ v2.5.2),
/// so far without state.
#[derive(Debug, Clone, Default)]
pub struct Context {}

impl Context {
    /// An empty context.
    ///
    /// Port of `Context::Create`.
    #[doc(alias = "Create")]
    pub fn new() -> Context {
        Context {}
    }
}
