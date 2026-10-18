// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of `tests/cpu/GpuShader_tests.cpp` @ v2.5.2 that extract the shaders of
//! processors of configs and LUT files: `MetalLutTest2`, `MetalSupport3` to `MetalSupport6` and
//! `VulkanSupport`. `generic_shader` is in `crates/ocio-gpu/src/gpu_shader_tests.rs`;
//! `MetalLutTest` waits for the legacy GPU processor (Phase 7), `MetalSupport7` for
//! `ExposureContrastTransform` and `MetalSupport8` and `MetalSupport9` for
//! `GradingRGBCurveTransform` (Phase 5).

use ocio::{
    ColorSpaceTransform, Config, DisplayViewTransform, FileTransform, Interpolation,
    OptimizationFlags, Transform,
};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::utils::pystring::os_path;
use ocio_testkit::assert_text_eq;

/// The directory of upstream's test files.
///
/// Port of `GetTestFilesDir` (tests/cpu/UnitTestUtils.cpp:20-36 @ v2.5.2).
fn get_test_files_dir() -> String {
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    let dir = dir.to_str().expect("a UTF-8 path").replace('\\', "/");
    dir.trim_start_matches("//?/").to_owned()
}

/// The shader text of `desc`.
fn text(desc: &GpuShaderDesc) -> &str {
    std::str::from_utf8(desc.shader_text()).expect("UTF-8")
}

/// Port of `OCIO_ADD_TEST(GpuShader, MetalLutTest2)` @ v2.5.2.
#[test]
fn metal_lut_test2() {
    const S_FROM_SPACE: &str = "ACEScg";
    const S_DIPLAY: &str = "AdobeRGB";
    const S_VIEW: &str = "raw";

    const CONFIG: &str = r#"ocio_profile_version: 2
environment: {}
search_path: "./"
roles:
  data: Raw
  default: Raw
  scene_linear: ACEScg

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  AdobeRGB:
    - !<View> {name: Raw, colorspace: Raw}

colorspaces:
  - !<ColorSpace>
    name: ACEScg
    to_reference: !<MatrixTransform> {matrix: [ 0.695452241357, 0.140678696470, 0.163869062172, 0, 0.044794563372, 0.859671118456, 0.095534318172, 0, -0.005525882558, 0.004025210306, 1.001500672252, 0, 0, 0, 0, 1 ]}
  - !<ColorSpace>
    name: Raw
    isdata: true"#;

    {
        let m_ocio_cfg = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        m_ocio_cfg.validate().unwrap();

        let mut transform = DisplayViewTransform::new();
        transform.set_src(S_FROM_SPACE);
        transform.set_display(S_DIPLAY);
        transform.set_view(S_VIEW);

        let processor = m_ocio_cfg.processor(&Transform::from(transform)).unwrap();

        let gpu_processor = processor.default_gpu_processor().unwrap();

        let mut shader_desc = GpuShaderDesc::default();

        shader_desc.set_function_name("Display");
        shader_desc.set_language(GpuLanguage::Msl2_0);

        gpu_processor
            .extract_gpu_shader_info(&mut shader_desc)
            .unwrap();
        const EXPECTED: &str = r#"
// Declaration of class wrapper

struct ocioDisplay
{
ocioDisplay(
)
{
}



// Declaration of the OCIO shader function

float4 Display(float4 inPixel)
{
  float4 outColor = inPixel;

  return outColor;
}

// Close class wrapper


};
float4 Display(
  float4 inPixel)
{
  return ocioDisplay(
  ).Display(inPixel);
}
"#;

        assert_text_eq("MetalLutTest2", EXPECTED, text(&shader_desc));
    }
}

/// Port of `OCIO_ADD_TEST(GpuShader, MetalSupport3)` @ v2.5.2.
#[test]
fn metal_support3() {
    // The unit test validates a single 1D LUT.

    let config = ["ocio_profile_version: 2\n\nenvironment: {ENV1: ", &get_test_files_dir(), "}\n\nsearch_path: $ENV1\n\nroles:\n  default: cs1\n  reference: cs1\n\ndisplays:\n  disp1:\n    - !<View> {name: view1, colorspace: cs2}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {src: lut1d_green.ctf}\n"].concat();

    {
        let m_ocio_cfg = Config::create_from_stream(config.as_bytes()).unwrap();
        m_ocio_cfg.validate().unwrap();

        let mut transform = ColorSpaceTransform::new();
        transform.set_src("cs1");
        transform.set_dst("cs2");

        let processor = m_ocio_cfg.processor(&Transform::from(transform)).unwrap();
        let gpu_processor = processor
            .optimized_gpu_processor(OptimizationFlags::NONE)
            .unwrap();

        let mut shader_desc = GpuShaderDesc::default();
        shader_desc.set_language(GpuLanguage::Msl2_0);
        shader_desc.set_function_name("MyMethodName");
        shader_desc.set_pixel_name("myPixelName");

        gpu_processor
            .extract_gpu_shader_info(&mut shader_desc)
            .unwrap();

        const EXPECTED: &str = r#"
// Declaration of class wrapper

struct ocioMyMethodName
{
ocioMyMethodName(
  texture1d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
)
{
  this->ocio_lut1d_0 = ocio_lut1d_0;
  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;
}



// Declaration of all textures

texture1d<float> ocio_lut1d_0;
sampler ocio_lut1d_0Sampler;

// Declaration of the OCIO shader function

float4 MyMethodName(float4 inPixel)
{
  float4 myPixelName = inPixel;
  
  // Add LUT 1D processing for ocio_lut1d_0
  
  {
    float3 ocio_lut1d_0_coords = (myPixelName.rgb * float3(31., 31., 31.) + float3(0.5, 0.5, 0.5) ) / float3(32., 32., 32.);
    myPixelName.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;
    myPixelName.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).g;
    myPixelName.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).b;
  }

  return myPixelName;
}

// Close class wrapper


};
float4 MyMethodName(
  texture1d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
  , float4 inPixel)
{
  return ocioMyMethodName(
    ocio_lut1d_0
    , ocio_lut1d_0Sampler
  ).MyMethodName(inPixel);
}
"#;

        assert_text_eq("MetalSupport3", EXPECTED, text(&shader_desc));
    }
}

/// Port of `OCIO_ADD_TEST(GpuShader, MetalSupport4)` @ v2.5.2.
#[test]
fn metal_support4() {
    // The unit test validates a single 3D LUT.

    let config = ["ocio_profile_version: 2\n\nenvironment: {ENV1: ", &get_test_files_dir(), "}\n\nsearch_path: $ENV1\n\nroles:\n  default: cs1\n  reference: cs1\n\ndisplays:\n  disp1:\n    - !<View> {name: view1, colorspace: cs2}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {src: lut3d_example_Inv.ctf}\n"].concat();

    {
        let m_ocio_cfg = Config::create_from_stream(config.as_bytes()).unwrap();
        m_ocio_cfg.validate().unwrap();

        let mut transform = ColorSpaceTransform::new();
        transform.set_src("cs1");
        transform.set_dst("cs2");

        let processor = m_ocio_cfg.processor(&Transform::from(transform)).unwrap();
        let gpu_processor = processor
            .optimized_gpu_processor(OptimizationFlags::NONE)
            .unwrap();

        let mut shader_desc = GpuShaderDesc::default();
        shader_desc.set_language(GpuLanguage::Msl2_0);
        shader_desc.set_function_name("MyMethodName");
        shader_desc.set_pixel_name("myPixelName");

        gpu_processor
            .extract_gpu_shader_info(&mut shader_desc)
            .unwrap();

        const EXPECTED: &str = r#"
// Declaration of class wrapper

struct ocioMyMethodName
{
ocioMyMethodName(
  texture3d<float> ocio_lut3d_0
  , sampler ocio_lut3d_0Sampler
)
{
  this->ocio_lut3d_0 = ocio_lut3d_0;
  this->ocio_lut3d_0Sampler = ocio_lut3d_0Sampler;
}



// Declaration of all textures

texture3d<float> ocio_lut3d_0;
sampler ocio_lut3d_0Sampler;

// Declaration of the OCIO shader function

float4 MyMethodName(float4 inPixel)
{
  float4 myPixelName = inPixel;
  
  // Add LUT 3D processing for ocio_lut3d_0
  
  float3 ocio_lut3d_0_coords = (myPixelName.zyx * float3(47., 47., 47.) + float3(0.5, 0.5, 0.5)) / float3(48., 48., 48.);
  myPixelName.rgb = ocio_lut3d_0.sample(ocio_lut3d_0Sampler, ocio_lut3d_0_coords).rgb;

  return myPixelName;
}

// Close class wrapper


};
float4 MyMethodName(
  texture3d<float> ocio_lut3d_0
  , sampler ocio_lut3d_0Sampler
  , float4 inPixel)
{
  return ocioMyMethodName(
    ocio_lut3d_0
    , ocio_lut3d_0Sampler
  ).MyMethodName(inPixel);
}
"#;

        assert_text_eq("MetalSupport4", EXPECTED, text(&shader_desc));
    }
}

/// Port of `OCIO_ADD_TEST(GpuShader, MetalSupport5)` @ v2.5.2.
#[test]
fn metal_support5() {
    // The unit test validates a single 1D LUT needing an helper method.

    let config = ["ocio_profile_version: 2\n\nenvironment: {ENV1: ", &get_test_files_dir(), "}\n\nsearch_path: $ENV1\n\nroles:\n  default: cs1\n  reference: cs1\n\ndisplays:\n  disp1:\n    - !<View> {name: view1, colorspace: cs2}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<FileTransform> {src: clf/lut1d_long.clf}\n"].concat();

    {
        let m_ocio_cfg = Config::create_from_stream(config.as_bytes()).unwrap();
        m_ocio_cfg.validate().unwrap();

        let mut transform = ColorSpaceTransform::new();
        transform.set_src("cs1");
        transform.set_dst("cs2");

        let processor = m_ocio_cfg.processor(&Transform::from(transform)).unwrap();
        let gpu_processor = processor
            .optimized_gpu_processor(OptimizationFlags::NONE)
            .unwrap();

        let mut shader_desc = GpuShaderDesc::default();
        shader_desc.set_language(GpuLanguage::Msl2_0);

        gpu_processor
            .extract_gpu_shader_info(&mut shader_desc)
            .unwrap();

        const EXPECTED: &str = r#"
// Declaration of class wrapper

struct ocioOCIOMain
{
ocioOCIOMain(
  texture2d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
)
{
  this->ocio_lut1d_0 = ocio_lut1d_0;
  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;
}



// Declaration of all textures

texture2d<float> ocio_lut1d_0;
sampler ocio_lut1d_0Sampler;

// Declaration of all helper methods

float2 ocio_lut1d_0_computePos(float f)
{
  float dep = clamp(f, 0.0, 1.0) * 131071.;
  float2 retVal;
  retVal.y = floor(dep / 4095.);
  retVal.x = dep - retVal.y * 4095.;
  retVal.x = (retVal.x + 0.5) / 4096.;
  retVal.y = (retVal.y + 0.5) / 33.;
  return retVal;
}

// Declaration of the OCIO shader function

float4 OCIOMain(float4 inPixel)
{
  float4 outColor = inPixel;
  
  // Add LUT 1D processing for ocio_lut1d_0
  
  {
    outColor.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.r)).r;
    outColor.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.g)).r;
    outColor.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_computePos(outColor.b)).r;
  }

  return outColor;
}

// Close class wrapper


};
float4 OCIOMain(
  texture2d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
  , float4 inPixel)
{
  return ocioOCIOMain(
    ocio_lut1d_0
    , ocio_lut1d_0Sampler
  ).OCIOMain(inPixel);
}
"#;

        assert_text_eq("MetalSupport5", EXPECTED, text(&shader_desc));
    }
}

/// Port of `OCIO_ADD_TEST(GpuShader, MetalSupport6)` @ v2.5.2.
#[test]
fn metal_support6() {
    // The unit test validates several arbitrary luts.

    let config = ["ocio_profile_version: 2\n\nenvironment: {ENV1: ", &get_test_files_dir(), "}\n\nsearch_path: $ENV1\n\nroles:\n  default: cs1\n  reference: cs1\n\ndisplays:\n  disp1:\n    - !<View> {name: view1, colorspace: cs2}\n\ncolorspaces:\n  - !<ColorSpace>\n    name: cs1\n\n  - !<ColorSpace>\n    name: cs2\n    from_scene_reference: !<GroupTransform>\n      children:\n        - !<FileTransform> {src: lut1d_green.ctf}\n        - !<FileTransform> {src: lut3d_example_Inv.ctf}\n        - !<FileTransform> {src: clf/lut1d_long.clf}\n"].concat();

    {
        let m_ocio_cfg = Config::create_from_stream(config.as_bytes()).unwrap();
        m_ocio_cfg.validate().unwrap();

        let mut transform = ColorSpaceTransform::new();
        transform.set_src("cs1");
        transform.set_dst("cs2");

        let processor = m_ocio_cfg.processor(&Transform::from(transform)).unwrap();
        let gpu_processor = processor
            .optimized_gpu_processor(OptimizationFlags::NONE)
            .unwrap();

        let mut shader_desc = GpuShaderDesc::default();
        shader_desc.set_language(GpuLanguage::Msl2_0);

        gpu_processor
            .extract_gpu_shader_info(&mut shader_desc)
            .unwrap();

        const EXPECTED: &str = r#"
// Declaration of class wrapper

struct ocioOCIOMain
{
ocioOCIOMain(
  texture3d<float> ocio_lut3d_1
  , sampler ocio_lut3d_1Sampler
  , texture1d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
  , texture2d<float> ocio_lut1d_2
  , sampler ocio_lut1d_2Sampler
)
{
  this->ocio_lut3d_1 = ocio_lut3d_1;
  this->ocio_lut3d_1Sampler = ocio_lut3d_1Sampler;
  this->ocio_lut1d_0 = ocio_lut1d_0;
  this->ocio_lut1d_0Sampler = ocio_lut1d_0Sampler;
  this->ocio_lut1d_2 = ocio_lut1d_2;
  this->ocio_lut1d_2Sampler = ocio_lut1d_2Sampler;
}



// Declaration of all textures

texture1d<float> ocio_lut1d_0;
sampler ocio_lut1d_0Sampler;
texture3d<float> ocio_lut3d_1;
sampler ocio_lut3d_1Sampler;
texture2d<float> ocio_lut1d_2;
sampler ocio_lut1d_2Sampler;

// Declaration of all helper methods

float2 ocio_lut1d_2_computePos(float f)
{
  float dep = clamp(f, 0.0, 1.0) * 131071.;
  float2 retVal;
  retVal.y = floor(dep / 4095.);
  retVal.x = dep - retVal.y * 4095.;
  retVal.x = (retVal.x + 0.5) / 4096.;
  retVal.y = (retVal.y + 0.5) / 33.;
  return retVal;
}

// Declaration of the OCIO shader function

float4 OCIOMain(float4 inPixel)
{
  float4 outColor = inPixel;
  
  // Add LUT 1D processing for ocio_lut1d_0
  
  {
    float3 ocio_lut1d_0_coords = (outColor.rgb * float3(31., 31., 31.) + float3(0.5, 0.5, 0.5) ) / float3(32., 32., 32.);
    outColor.r = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;
    outColor.g = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).g;
    outColor.b = ocio_lut1d_0.sample(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).b;
  }
  
  // Add LUT 3D processing for ocio_lut3d_1
  
  float3 ocio_lut3d_1_coords = (outColor.zyx * float3(47., 47., 47.) + float3(0.5, 0.5, 0.5)) / float3(48., 48., 48.);
  outColor.rgb = ocio_lut3d_1.sample(ocio_lut3d_1Sampler, ocio_lut3d_1_coords).rgb;
  
  // Add LUT 1D processing for ocio_lut1d_2
  
  {
    outColor.r = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.r)).r;
    outColor.g = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.g)).r;
    outColor.b = ocio_lut1d_2.sample(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.b)).r;
  }

  return outColor;
}

// Close class wrapper


};
float4 OCIOMain(
  texture3d<float> ocio_lut3d_1
  , sampler ocio_lut3d_1Sampler
  , texture1d<float> ocio_lut1d_0
  , sampler ocio_lut1d_0Sampler
  , texture2d<float> ocio_lut1d_2
  , sampler ocio_lut1d_2Sampler
  , float4 inPixel)
{
  return ocioOCIOMain(
    ocio_lut3d_1
    , ocio_lut3d_1Sampler
    , ocio_lut1d_0
    , ocio_lut1d_0Sampler
    , ocio_lut1d_2
    , ocio_lut1d_2Sampler
  ).OCIOMain(inPixel);
}
"#;

        assert_text_eq("MetalSupport6", EXPECTED, text(&shader_desc));
    }
}

/// Port of `OCIO_ADD_TEST(GpuShader, VulkanSupport)` @ v2.5.2.
#[test]
fn vulkan_support() {
    let config = Config::new().unwrap();
    let mut ft = FileTransform::new();
    let paths = [
        get_test_files_dir().into_bytes(),
        b"clf".to_vec(),
        b"lut1d_lut3d_lut1d.clf".to_vec(),
    ];
    let file_path = os_path::normpath(&if cfg!(windows) {
        os_path::join_nt_all(&paths)
    } else {
        os_path::join_posix_all(&paths)
    });
    ft.set_src(&file_path);

    let processor = config.processor(&Transform::from(ft)).unwrap();
    let gpu_processor = processor
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();

    let mut shader_desc = GpuShaderDesc::default();
    shader_desc.set_language(GpuLanguage::GlslVk4_6);
    shader_desc.set_descriptor_set_index(2, 10).unwrap();

    gpu_processor
        .extract_gpu_shader_info(&mut shader_desc)
        .unwrap();

    assert_eq!(shader_desc.descriptor_set_index(), 2);
    assert_eq!(shader_desc.num_textures(), 2);
    assert_eq!(shader_desc.num_textures_3d(), 1);

    // Check Lut1D textures.
    {
        let index = 0;
        let t = shader_desc.texture(index).unwrap();

        assert_eq!(t.texture_name(), b"ocio_lut1d_0");
        assert_eq!(t.sampler_name(), b"ocio_lut1d_0Sampler");
        assert_eq!(t.width(), 65);
        assert_eq!(t.height(), 1);
        assert_eq!(TextureType::RedChannel, t.channel());
        assert_eq!(TextureDimensions::D1, t.dimensions());
        assert_eq!(Interpolation::Linear, t.interpolation());

        assert_eq!(shader_desc.texture_shader_binding_index(index).unwrap(), 10);
    }
    {
        let index = 1;
        let t = shader_desc.texture(index).unwrap();

        assert_eq!(t.texture_name(), b"ocio_lut1d_2");
        assert_eq!(t.sampler_name(), b"ocio_lut1d_2Sampler");
        assert_eq!(t.width(), 4096);
        assert_eq!(t.height(), 17);
        assert_eq!(TextureType::RedChannel, t.channel());
        assert_eq!(TextureDimensions::D2, t.dimensions());
        assert_eq!(Interpolation::Linear, t.interpolation());

        assert_eq!(shader_desc.texture_shader_binding_index(index).unwrap(), 12);
    }
    // Check Lut3D textures.
    {
        let index = 0;
        let t = shader_desc.texture_3d(index).unwrap();

        assert_eq!(t.texture_name(), b"ocio_lut3d_1");
        assert_eq!(t.sampler_name(), b"ocio_lut3d_1Sampler");
        assert_eq!(t.edge_len(), 3);
        assert_eq!(Interpolation::Linear, t.interpolation());

        assert_eq!(
            shader_desc.texture_3d_shader_binding_index(index).unwrap(),
            11
        );
    }

    // Note that the binding index increments properly for the three LUTs.

    const EXPECTED: &str = r#"
// Declaration of all textures

layout(set=2, binding = 10) uniform sampler1D ocio_lut1d_0Sampler; 
layout(set=2, binding = 11) uniform sampler3D ocio_lut3d_1Sampler; 
layout(set=2, binding = 12) uniform sampler2D ocio_lut1d_2Sampler; 

// Declaration of all helper methods

vec2 ocio_lut1d_2_computePos(float f)
{
  float dep;
  float abs_f = abs(f);
  if (abs_f > 6.10351562e-05)
  {
    vec3 fComp = vec3(15., 15., 15.);
    float absarr = min( abs_f, 65504.);
    fComp.x = floor( log2( absarr ) );
    float lower = pow( 2.0, fComp.x );
    fComp.y = ( absarr - lower ) / lower;
    vec3 scale = vec3(1024., 1024., 1024.);
    dep = dot( fComp, scale );
  }
  else
  {
    dep = abs_f * 16777216.;
  }
  dep += (f < 0.) ? 32768.0 : 0.0;
  vec2 retVal;
  retVal.y = floor(dep / 4095.);
  retVal.x = dep - retVal.y * 4095.;
  retVal.x = (retVal.x + 0.5) / 4096.;
  retVal.y = (retVal.y + 0.5) / 17.;
  return retVal;
}

// Declaration of the OCIO shader function

vec4 OCIOMain(vec4 inPixel)
{
  vec4 outColor = inPixel;
  
  // Add LUT 1D processing for ocio_lut1d_0
  
  {
    vec3 ocio_lut1d_0_coords = (outColor.rgb * vec3(64., 64., 64.) + vec3(0.5, 0.5, 0.5) ) / vec3(65., 65., 65.);
    outColor.r = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;
    outColor.g = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).r;
    outColor.b = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).r;
  }
  
  // Add LUT 3D processing for ocio_lut3d_1
  
  vec3 ocio_lut3d_1_coords = (outColor.zyx * vec3(2., 2., 2.) + vec3(0.5, 0.5, 0.5)) / vec3(3., 3., 3.);
  outColor.rgb = texture(ocio_lut3d_1Sampler, ocio_lut3d_1_coords).rgb;
  
  // Add LUT 1D processing for ocio_lut1d_2
  
  {
    outColor.r = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.r)).r;
    outColor.g = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.g)).r;
    outColor.b = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.b)).r;
  }

  return outColor;
}
"#;

    assert_text_eq("VulkanSupport", EXPECTED, text(&shader_desc));
}
