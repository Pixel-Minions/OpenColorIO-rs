// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The texts of the Studio built-in configs.
//!
//! Port of `Studio.cpp.in` (src/OpenColorIO/builtinconfigs/Studio.cpp.in:11-25 @ v2.5.2), which
//! the build fills with the bytes of `builtinconfigs/configs/studio-config-*.ocio`
//! (src/OpenColorIO/CMakeLists.txt:262-298 @ v2.5.2); each line ends as in the wheel of the
//! platform (see the [module](super) and I-148).

use super::{embedded_len, embedded_text};

const STUDIO_V100_SOURCE: &[u8] = include_bytes!(
    "../../../../upstream/OpenColorIO/src/OpenColorIO/builtinconfigs/configs/studio-config-v1.0.0_aces-v1.3_ocio-v2.1.ocio"
);
const STUDIO_V210_SOURCE: &[u8] = include_bytes!(
    "../../../../upstream/OpenColorIO/src/OpenColorIO/builtinconfigs/configs/studio-config-v2.1.0_aces-v1.3_ocio-v2.3.ocio"
);
const STUDIO_V220_SOURCE: &[u8] = include_bytes!(
    "../../../../upstream/OpenColorIO/src/OpenColorIO/builtinconfigs/configs/studio-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio"
);
const STUDIO_V400_SOURCE: &[u8] = include_bytes!(
    "../../../../upstream/OpenColorIO/src/OpenColorIO/builtinconfigs/configs/studio-config-v4.0.0_aces-v2.0_ocio-v2.5.ocio"
);

/// `STUDIO_CONFIG_V100_ACES_V13_OCIO_V21`.
pub(crate) const STUDIO_CONFIG_V100_ACES_V13_OCIO_V21: &[u8] =
    &embedded_text::<{ embedded_len(STUDIO_V100_SOURCE) }>(STUDIO_V100_SOURCE);
/// `STUDIO_CONFIG_V210_ACES_V13_OCIO_V23`.
pub(crate) const STUDIO_CONFIG_V210_ACES_V13_OCIO_V23: &[u8] =
    &embedded_text::<{ embedded_len(STUDIO_V210_SOURCE) }>(STUDIO_V210_SOURCE);
/// `STUDIO_CONFIG_V220_ACES_V13_OCIO_V24`.
pub(crate) const STUDIO_CONFIG_V220_ACES_V13_OCIO_V24: &[u8] =
    &embedded_text::<{ embedded_len(STUDIO_V220_SOURCE) }>(STUDIO_V220_SOURCE);
/// `STUDIO_CONFIG_V400_ACES_V20_OCIO_V25`.
pub(crate) const STUDIO_CONFIG_V400_ACES_V20_OCIO_V25: &[u8] =
    &embedded_text::<{ embedded_len(STUDIO_V400_SOURCE) }>(STUDIO_V400_SOURCE);
