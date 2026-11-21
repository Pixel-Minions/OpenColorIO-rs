// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The texts of the CG built-in configs.
//!
//! Port of `CG.cpp.in` (src/OpenColorIO/builtinconfigs/CG.cpp.in:11-15 @ v2.6.0), which the
//! build fills with the bytes of `builtinconfigs/configs/cg-config-*.ocio`
//! (src/OpenColorIO/CMakeLists.txt:266-303 @ v2.6.0); each line ends as in the wheel of the
//! platform (see the [module](super) and I-148).

use super::{embedded_len, embedded_text};

const CG_V100_SOURCE: &[u8] = include_bytes!("configs/cg-config-v1.0.0_aces-v1.3_ocio-v2.1.ocio");
const CG_V210_SOURCE: &[u8] = include_bytes!("configs/cg-config-v2.1.0_aces-v1.3_ocio-v2.3.ocio");
const CG_V220_SOURCE: &[u8] = include_bytes!("configs/cg-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio");
const CG_V400_SOURCE: &[u8] = include_bytes!("configs/cg-config-v4.0.0_aces-v2.0_ocio-v2.5.ocio");
const CG_V500_SOURCE: &[u8] = include_bytes!("configs/cg-config-v5.0.0_aces-v2.1_ocio-v2.6.ocio");

/// `CG_CONFIG_V100_ACES_V13_OCIO_V21`.
pub(crate) const CG_CONFIG_V100_ACES_V13_OCIO_V21: &[u8] =
    &embedded_text::<{ embedded_len(CG_V100_SOURCE) }>(CG_V100_SOURCE);
/// `CG_CONFIG_V210_ACES_V13_OCIO_V23`.
pub(crate) const CG_CONFIG_V210_ACES_V13_OCIO_V23: &[u8] =
    &embedded_text::<{ embedded_len(CG_V210_SOURCE) }>(CG_V210_SOURCE);
/// `CG_CONFIG_V220_ACES_V13_OCIO_V24`.
pub(crate) const CG_CONFIG_V220_ACES_V13_OCIO_V24: &[u8] =
    &embedded_text::<{ embedded_len(CG_V220_SOURCE) }>(CG_V220_SOURCE);
/// `CG_CONFIG_V400_ACES_V20_OCIO_V25`.
pub(crate) const CG_CONFIG_V400_ACES_V20_OCIO_V25: &[u8] =
    &embedded_text::<{ embedded_len(CG_V400_SOURCE) }>(CG_V400_SOURCE);
/// `CG_CONFIG_V500_ACES_V21_OCIO_V26`.
pub(crate) const CG_CONFIG_V500_ACES_V21_OCIO_V26: &[u8] =
    &embedded_text::<{ embedded_len(CG_V500_SOURCE) }>(CG_V500_SOURCE);
