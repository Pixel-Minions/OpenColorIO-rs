// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What `ociobakelut --format icc` reaches of Little CMS 2.17, the color engine the wheel's tool
//! links statically, translated to Rust (docs/cards/phase10-icc.md). The files mirror lcms2's
//! `src/`, and the card lists what is reached and what isn't.
//!
//! The context parameter is gone: every call in the tool passes null, so the global context is
//! the only one, with no plug-ins. `cmsPipeline` and `cmsStage` own their contents, so the
//! `cms*Free` calls are gone too.

// A translation keeps the shape of the C, so that the two can be read side by side.
#![allow(
    clippy::explicit_counter_loop,
    clippy::if_same_then_else,
    clippy::manual_clamp,
    clippy::needless_late_init,
    clippy::needless_range_loop
)]

pub mod cmserr;
pub mod cmsgamma;
pub mod cmsintrp;
pub mod cmsio0;
pub mod cmslut;
pub mod cmsmtrx;
pub mod cmsnamed;
pub mod cmsopt;
pub mod cmspcs;
pub mod cmsplugin;
pub mod cmstypes;
pub mod cmswtpnt;
pub mod types;
