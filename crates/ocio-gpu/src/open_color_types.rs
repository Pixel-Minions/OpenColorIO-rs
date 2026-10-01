// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums of `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2 that only the GPU side
//! uses. The public `ocio` crate re-exports them.
//!
//! So far: `GpuLanguage`, with `GpuLanguageToString` from `src/OpenColorIO/ParseUtils.cpp`,
//! and `UniformDataType`.

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

/// The name of a language in cache IDs and config files.
///
/// Port of `GpuLanguageToString` (src/OpenColorIO/ParseUtils.cpp:258-274 @ v2.5.2), whose
/// "Unsupported GPU shader language." for a value outside the enumerators can't be reached
/// with [`GpuLanguage`].
pub fn gpu_language_to_string(language: GpuLanguage) -> &'static str {
    match language {
        GpuLanguage::Cg => "cg",
        GpuLanguage::Glsl1_2 => "glsl_1.2",
        GpuLanguage::Glsl1_3 => "glsl_1.3",
        GpuLanguage::Glsl4_0 => "glsl_4.0",
        GpuLanguage::GlslVk4_6 => "glsl_vk_4.6",
        GpuLanguage::GlslEs1_0 => "glsl_es_1.0",
        GpuLanguage::GlslEs3_0 => "glsl_es_3.0",
        GpuLanguage::HlslSm5_0 => "hlsl_sm_5.0",
        GpuLanguage::Msl2_0 => "msl_2",
        GpuLanguage::Osl1 => "osl_1",
    }
}

/// The type of a uniform's value.
///
/// Port of `UniformDataType` (include/OpenColorIO/OpenColorTypes.h:623-631 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UniformDataType {
    /// `UNIFORM_DOUBLE`.
    Double = 0,
    /// `UNIFORM_BOOL`.
    Bool,
    /// `UNIFORM_FLOAT3`: an array of 3 floats.
    Float3,
    /// `UNIFORM_VECTOR_FLOAT`: a vector of floats (its size is set by the uniform).
    VectorFloat,
    /// `UNIFORM_VECTOR_INT`: a vector of ints (its size is set by the uniform).
    VectorInt,
    /// `UNIFORM_UNKNOWN`.
    Unknown,
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
