// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums of `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2 that only the GPU side
//! uses. The public `ocio` crate re-exports them.
//!
//! So far: `GpuLanguage`.

/// A language OCIO writes shader programs in.
///
/// The discriminants are upstream's enumerator values. Upstream's deprecated alias
/// `GPU_LANGUAGE_HLSL_DX11` is [`GpuLanguage::HLSL_DX11`].
///
/// Port of `GpuLanguage` (include/OpenColorIO/OpenColorTypes.h:466-481 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuLanguage {
    /// `GPU_LANGUAGE_CG`: Nvidia Cg shader.
    Cg = 0,
    /// `GPU_LANGUAGE_GLSL_1_2`: OpenGL Shading Language.
    Glsl1_2,
    /// `GPU_LANGUAGE_GLSL_1_3`: OpenGL Shading Language.
    Glsl1_3,
    /// `GPU_LANGUAGE_GLSL_4_0`: OpenGL Shading Language.
    Glsl4_0,
    /// `GPU_LANGUAGE_GLSL_VK_4_6`: OpenGL Shading Language for Vulkan.
    GlslVk4_6,
    /// `GPU_LANGUAGE_HLSL_SM_5_0`: DirectX High Level Shading Language.
    HlslSm5_0,
    /// `LANGUAGE_OSL_1`: Open Shading Language.
    Osl1,
    /// `GPU_LANGUAGE_GLSL_ES_1_0`: OpenGL ES Shading Language.
    GlslEs1_0,
    /// `GPU_LANGUAGE_GLSL_ES_3_0`: OpenGL ES Shading Language.
    GlslEs3_0,
    /// `GPU_LANGUAGE_MSL_2_0`: Metal Shading Language.
    Msl2_0,
}

impl GpuLanguage {
    /// `GPU_LANGUAGE_HLSL_DX11`, upstream's deprecated name for [`GpuLanguage::HlslSm5_0`].
    pub const HLSL_DX11: GpuLanguage = GpuLanguage::HlslSm5_0;

    /// Every language, in upstream's enumerator order.
    pub const ALL: [GpuLanguage; 10] = [
        GpuLanguage::Cg,
        GpuLanguage::Glsl1_2,
        GpuLanguage::Glsl1_3,
        GpuLanguage::Glsl4_0,
        GpuLanguage::GlslVk4_6,
        GpuLanguage::HlslSm5_0,
        GpuLanguage::Osl1,
        GpuLanguage::GlslEs1_0,
        GpuLanguage::GlslEs3_0,
        GpuLanguage::Msl2_0,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The discriminants follow the order of `ALL`, from 0 as upstream's enumerators do.
    #[test]
    fn all_is_in_enumerator_order() {
        for (i, lang) in GpuLanguage::ALL.iter().enumerate() {
            assert_eq!(*lang as usize, i, "{lang:?}");
        }
        assert_eq!(GpuLanguage::HLSL_DX11, GpuLanguage::HlslSm5_0);
    }
}
