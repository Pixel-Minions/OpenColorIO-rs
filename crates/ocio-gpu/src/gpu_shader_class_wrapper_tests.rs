// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the class wrappers. No upstream test covers them alone:
//! - upstream's Metal shader tests (`tests/cpu/GpuShader_tests.cpp` @ v2.5.2) need processors,
//!   so they are ported with that file; here, the wrapper writes its part of their expected
//!   texts;
//! - the OSL wrapper's text is in no upstream test. Both wrappers write their parts of the
//!   wheel's shaders, which the oracle extracts (`gpu_shader`).

use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

use super::*;

/// The class wrapper's part of a Metal shader that one of upstream's tests expects.
struct MetalCase {
    /// The upstream test.
    test: &'static str,
    /// The shader's function name. The resource prefix is the default, `ocio`
    /// (`GpuShaderDesc.cpp:53` @ v2.5.2); no Metal test sets another.
    function_name: &'static str,
    /// The expected text up to `\n// Declaration of all helper methods\n` or, without helper
    /// methods, `\n// Declaration of the OCIO shader function\n`: the wrapper's header, which
    /// ends with the declarations it read.
    header: &'static str,
    /// The expected text from `\n// Close class wrapper\n` on: what the wrapper adds after
    /// the function's footer.
    footer: &'static str,
}

/// Verbatim parts of the expected text of every Metal test in `tests/cpu/GpuShader_tests.cpp`
/// @ v2.5.2 (each comment gives the lines of the whole text).
const METAL_CASES: [MetalCase; 9] = [
    // `MetalLutTest`: the expected text at lines 309-340.
    MetalCase {
        test: "MetalLutTest",
        function_name: "Display",
        header: r#"
// Declaration of class wrapper

struct ocioDisplay
{
ocioDisplay(
)
{
}


"#,
        footer: r#"
// Close class wrapper


};
float4 Display(
  float4 inPixel)
{
  return ocioDisplay(
  ).Display(inPixel);
}
"#,
    },
    // `MetalLutTest2`: the expected text at lines 399-430.
    MetalCase {
        test: "MetalLutTest2",
        function_name: "Display",
        header: r#"
// Declaration of class wrapper

struct ocioDisplay
{
ocioDisplay(
)
{
}


"#,
        footer: r#"
// Close class wrapper


};
float4 Display(
  float4 inPixel)
{
  return ocioDisplay(
  ).Display(inPixel);
}
"#,
    },
    // `MetalSupport3`: the expected text at lines 486-539.
    MetalCase {
        test: "MetalSupport3",
        function_name: "MyMethodName",
        header: r#"
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
"#,
        footer: r#"
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
"#,
    },
    // `MetalSupport4`: the expected text at lines 595-644.
    MetalCase {
        test: "MetalSupport4",
        function_name: "MyMethodName",
        header: r#"
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
"#,
        footer: r#"
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
"#,
    },
    // `MetalSupport5`: the expected text at lines 698-763.
    MetalCase {
        test: "MetalSupport5",
        function_name: "OCIOMain",
        header: r#"
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
"#,
        footer: r#"
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
"#,
    },
    // `MetalSupport6`: the expected text at lines 820-919.
    MetalCase {
        test: "MetalSupport6",
        function_name: "OCIOMain",
        header: r#"
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
"#,
        footer: r#"
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
"#,
    },
    // `MetalSupport7`: the expected text at lines 972-1029.
    MetalCase {
        test: "MetalSupport7",
        function_name: "OCIOMain",
        header: r#"
// Declaration of class wrapper

struct ocioOCIOMain
{
ocioOCIOMain(
  float ocio_exposure_contrast_exposureVal
  , float ocio_exposure_contrast_gammaVal
)
{
  this->ocio_exposure_contrast_exposureVal = ocio_exposure_contrast_exposureVal;
  this->ocio_exposure_contrast_gammaVal = ocio_exposure_contrast_gammaVal;
}


// Declaration of all variables

float ocio_exposure_contrast_exposureVal;
float ocio_exposure_contrast_gammaVal;

"#,
        footer: r#"
// Close class wrapper


};
float4 OCIOMain(
  float ocio_exposure_contrast_exposureVal
  , float ocio_exposure_contrast_gammaVal
  , float4 inPixel)
{
  return ocioOCIOMain(
    ocio_exposure_contrast_exposureVal
    , ocio_exposure_contrast_gammaVal
  ).OCIOMain(inPixel);
}
"#,
    },
    // `MetalSupport8`: the expected text at lines 1083-1179.
    MetalCase {
        test: "MetalSupport8",
        function_name: "OCIOMain",
        header: r#"
// Declaration of class wrapper

struct ocioOCIOMain
{
ocioOCIOMain(
)
{
}


"#,
        footer: r#"
// Close class wrapper


};
float4 OCIOMain(
  float4 inPixel)
{
  return ocioOCIOMain(
  ).OCIOMain(inPixel);
}
"#,
    },
    // `MetalSupport9`: the expected text at lines 1201-1363.
    MetalCase {
        test: "MetalSupport9",
        function_name: "OCIOMain",
        header: r#"
// Declaration of class wrapper

struct ocioOCIOMain
{
ocioOCIOMain(
  constant int ocio_grading_rgbcurve_knotsOffsets[8]
  , int ocio_grading_rgbcurve_knotsOffsets_count
  , constant float ocio_grading_rgbcurve_knots[120]
  , int ocio_grading_rgbcurve_knots_count
  , constant int ocio_grading_rgbcurve_coefsOffsets[8]
  , int ocio_grading_rgbcurve_coefsOffsets_count
  , constant float ocio_grading_rgbcurve_coefs[360]
  , int ocio_grading_rgbcurve_coefs_count
  , bool ocio_grading_rgbcurve_localBypass
)
{
  for(int i = 0; i < ocio_grading_rgbcurve_knotsOffsets_count; ++i)
  {
    this->ocio_grading_rgbcurve_knotsOffsets[i] = ocio_grading_rgbcurve_knotsOffsets[i];
  }
  for(int i = ocio_grading_rgbcurve_knotsOffsets_count; i < 8; ++i)
  {
    this->ocio_grading_rgbcurve_knotsOffsets[i] = 0;
  }
  for(int i = 0; i < ocio_grading_rgbcurve_knots_count; ++i)
  {
    this->ocio_grading_rgbcurve_knots[i] = ocio_grading_rgbcurve_knots[i];
  }
  for(int i = ocio_grading_rgbcurve_knots_count; i < 120; ++i)
  {
    this->ocio_grading_rgbcurve_knots[i] = 0;
  }
  for(int i = 0; i < ocio_grading_rgbcurve_coefsOffsets_count; ++i)
  {
    this->ocio_grading_rgbcurve_coefsOffsets[i] = ocio_grading_rgbcurve_coefsOffsets[i];
  }
  for(int i = ocio_grading_rgbcurve_coefsOffsets_count; i < 8; ++i)
  {
    this->ocio_grading_rgbcurve_coefsOffsets[i] = 0;
  }
  for(int i = 0; i < ocio_grading_rgbcurve_coefs_count; ++i)
  {
    this->ocio_grading_rgbcurve_coefs[i] = ocio_grading_rgbcurve_coefs[i];
  }
  for(int i = ocio_grading_rgbcurve_coefs_count; i < 360; ++i)
  {
    this->ocio_grading_rgbcurve_coefs[i] = 0;
  }
  this->ocio_grading_rgbcurve_localBypass = ocio_grading_rgbcurve_localBypass;
}


// Declaration of all variables

int ocio_grading_rgbcurve_knotsOffsets[8];
float ocio_grading_rgbcurve_knots[120];
int ocio_grading_rgbcurve_coefsOffsets[8];
float ocio_grading_rgbcurve_coefs[360];
bool ocio_grading_rgbcurve_localBypass;

"#,
        footer: r#"
// Close class wrapper


};
float4 OCIOMain(
  constant int ocio_grading_rgbcurve_knotsOffsets[8]
  , int ocio_grading_rgbcurve_knotsOffsets_count
  , constant float ocio_grading_rgbcurve_knots[120]
  , int ocio_grading_rgbcurve_knots_count
  , constant int ocio_grading_rgbcurve_coefsOffsets[8]
  , int ocio_grading_rgbcurve_coefsOffsets_count
  , constant float ocio_grading_rgbcurve_coefs[360]
  , int ocio_grading_rgbcurve_coefs_count
  , bool ocio_grading_rgbcurve_localBypass
  , float4 inPixel)
{
  return ocioOCIOMain(
    ocio_grading_rgbcurve_knotsOffsets
    , ocio_grading_rgbcurve_knotsOffsets_count
    , ocio_grading_rgbcurve_knots
    , ocio_grading_rgbcurve_knots_count
    , ocio_grading_rgbcurve_coefsOffsets
    , ocio_grading_rgbcurve_coefsOffsets_count
    , ocio_grading_rgbcurve_coefs
    , ocio_grading_rgbcurve_coefs_count
    , ocio_grading_rgbcurve_localBypass
  ).OCIOMain(inPixel);
}
"#,
    },
];

/// The Metal wrapper writes the class of each of upstream's Metal shaders, from the
/// declarations those shaders hold: no parameter, a 1D, 2D or 3D LUT's texture and sampler,
/// three LUTs (the 3D one first), float uniforms, and uniform arrays with a bool. A copy
/// writes the same.
#[test]
fn metal_wrappers_write_the_classes_of_upstreams_shaders() {
    for case in &METAL_CASES {
        // The declarations are what the header ends with, after the constructor's closing
        // brace and the empty line the wrapper writes after it.
        let at = case.header.find("\n}\n\n").expect(case.test) + 4;
        let declarations = &case.header.as_bytes()[at..];
        let function_footer = "\n  return outColor;\n}\n";
        let expected_footer = format!("{function_footer}{}", case.footer);

        let mut wrapper = GpuShaderClassWrapper::create_class_wrapper(GpuLanguage::Msl2_0);
        wrapper
            .prepare_class_wrapper(b"ocio", case.function_name.as_bytes(), declarations)
            .unwrap();

        for wrapper in [&wrapper, &wrapper.clone_wrapper()] {
            let header = wrapper.get_class_wrapper_header(declarations).unwrap();
            assert_text_eq(
                case.test,
                case.header,
                std::str::from_utf8(&header).unwrap(),
            );

            let footer = wrapper
                .get_class_wrapper_footer(function_footer.as_bytes())
                .unwrap();
            assert_text_eq(
                case.test,
                &expected_footer,
                std::str::from_utf8(&footer).unwrap(),
            );
        }
    }
}

/// U-10 (docs/improvements.md): where upstream would read past the end of the line after a
/// texture's declaration, which has no `sampler` and is shorter than 6 bytes, the port returns
/// an error. That line can be the empty one after the last line, or a piece of a name cut by a
/// line feed. A line of 6 bytes is read up to its terminating NUL, which is defined; a texture
/// on the last line without a line feed reads that line again (`getline` at the end of the
/// stream keeps it). Neither is an error.
#[test]
fn a_texture_without_a_sampler_after_it_is_an_error_where_upstream_reads_past_the_line() {
    let refused: [(&[u8], &str); 3] = [
        (
            b"\n// Declaration of all textures\n\ntexture2d<float> t;\n",
            "The MSL class wrapper found no sampler after texture 't': the next line is ''.",
        ),
        (
            b"texture3d<float> a\nb\nc_lut3d_0;\nsampler a\nb\nc_lut3d_0Sampler;\n",
            "The MSL class wrapper found no sampler after texture 'a': the next line is 'b'.",
        ),
        (
            b"texture1d<float> t;\n12345\n",
            "The MSL class wrapper found no sampler after texture 't': the next line is \
             '12345'.",
        ),
    ];
    for (declarations, message) in refused {
        let mut wrapper = GpuShaderClassWrapper::create_class_wrapper(GpuLanguage::Msl2_0);
        let error = wrapper
            .prepare_class_wrapper(b"ocio", b"OCIOMain", declarations)
            .unwrap_err();
        assert_eq!(error.message(), message);
    }

    let accepted: [&[u8]; 2] = [b"texture1d<float> t;\n123456\n", b"texture1d<float> t;"];
    for declarations in accepted {
        let mut wrapper = GpuShaderClassWrapper::create_class_wrapper(GpuLanguage::Msl2_0);
        assert!(
            wrapper
                .prepare_class_wrapper(b"ocio", b"OCIOMain", declarations)
                .is_ok()
        );
    }
}

/// The start of the section after the declarations: the helper methods
/// (`GpuShaderDesc.cpp:302-309` @ v2.5.2) or, without them, the OCIO function
/// (`GPUProcessor.cpp:26-35` @ v2.5.2).
const AFTER_DECLARATIONS: [&str; 2] = [
    "\n// Declaration of all helper methods\n",
    "\n// Declaration of the OCIO shader function\n",
];

/// Where the declarations' section of a shader text ends.
fn declarations_end(text: &str) -> usize {
    AFTER_DECLARATIONS
        .iter()
        .filter_map(|marker| text.find(marker))
        .min()
        .expect("a section after the declarations")
}

/// A 1D LUT of `n` entries whose channels differ, set entry by entry.
fn lut1d(n: usize) -> Value {
    let calls: Vec<Value> = (0..n)
        .map(|i| {
            let x = i as f64 / (n - 1) as f64;
            json!(["setValue", i, x, 0.5 * x, 0.25 + 0.5 * x])
        })
        .collect();
    json!({"class": "Lut1DTransform", "args": {"length": n}, "calls": calls})
}

/// A 3D LUT of 2 entries a side that isn't the identity.
fn lut3d() -> Value {
    let calls: Vec<Value> = (0..8u32)
        .map(|i| {
            let (r, g, b) = (i / 4, (i / 2) % 2, i % 2);
            json!([
                "setValue",
                r,
                g,
                b,
                0.1 + 0.3 * f64::from(r),
                0.2 + 0.25 * f64::from(g),
                0.05 + 0.5 * f64::from(b)
            ])
        })
        .collect();
    json!({"class": "Lut3DTransform", "args": {"gridSize": 2}, "calls": calls})
}

/// A request's case: what it holds, and the names the wrapper takes.
struct WrapperCase {
    label: String,
    language: GpuLanguage,
    resource_prefix: String,
    function_name: String,
    request: GpuShaderRequest,
}

/// The wrappers write what the wheel's shaders hold of them, in MSL and OSL, for processors
/// with no declaration, 1D, 2D and 3D textures, three textures (the 3D one first in the
/// class), float uniforms, and uniform arrays with a bool, and for names the wrappers treat
/// apart: an empty prefix (`OCIO_` then), an empty function name, a prefix starting with a
/// non-ASCII byte or with white space then a digit, line feeds the wheel reads safely (U-6),
/// control characters, double underscores. The test finds the wrapper's parts in each shader
/// text, and gives the port's wrapper the declarations the text holds, with the names the
/// description's getters give:
/// - its header is the text up to the helper methods or the OCIO function;
/// - its footer, after everything before it, is the whole text.
///
/// In MSL, the declarations follow the class's constructor and the empty line after it; in
/// OSL, the shader's opening brace. A class name starting with a digit raises the wheel's
/// error (that name has no double underscore, so the request's names are the getters').
#[test]
fn wrappers_write_the_wheels_shaders() {
    let none =
        |transform: Value| json!({"transform": transform, "optimization": "OPTIMIZATION_NONE"});
    let exposure_contrast = json!({"transform": {
        "class": "ExposureContrastTransform",
        "args": {"exposure": 0.5, "contrast": 1.25, "gamma": 1.5},
        "calls": [["makeExposureDynamic"], ["makeContrastDynamic"], ["makeGammaDynamic"]]}});
    let no_op = json!({"transform": {"class": "MatrixTransform"}});
    // (label, processor, texture width limit)
    let msl_processors: Vec<(&str, Value, Option<u32>)> = vec![
        ("no op", no_op.clone(), None),
        ("a 1D LUT", none(lut1d(5)), None),
        ("a 1D LUT in a 2D texture", none(lut1d(9)), Some(4)),
        ("a 3D LUT", none(lut3d()), None),
        (
            "three LUTs",
            none(json!({"class": "GroupTransform", "children": [lut1d(3), lut3d(), lut1d(9)]})),
            Some(4),
        ),
        ("float uniforms", exposure_contrast.clone(), None),
        (
            "uniform arrays and a bool",
            json!({"transform": {"class": "GradingRGBCurveTransform",
                "args": {"style": {"enum": "GRADING_LOG"}, "dynamic": true}}}),
            None,
        ),
    ];
    let osl_processors: Vec<(&str, Value, Option<u32>)> = vec![
        ("no op", no_op, None),
        ("float uniforms", exposure_contrast, None),
        (
            "a matrix",
            json!({"transform": {"class": "MatrixTransform", "args": {"matrix":
                [1.0, 0.25, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]}}}),
            None,
        ),
    ];
    // (label, resource prefix, function name, pixel name); `None` keeps the default.
    type Names = (
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        Option<&'static str>,
    );
    let msl_names: Vec<Names> = vec![
        ("the defaults", None, None, None),
        ("an empty prefix", Some(""), None, None),
        ("other names", Some("R3"), Some("F1"), Some("P2")),
        ("an empty function name", None, Some(""), None),
        (
            "a prefix starting with a non-ASCII byte",
            Some("\u{e9}t\u{e9}"),
            None,
            None,
        ),
        (
            "white space, then a non-ASCII byte",
            Some(" \t\u{3c0}"),
            None,
            None,
        ),
        ("white space, then a digit", Some(" 9x"), None, None),
        ("a line feed in the prefix", Some("a\nb"), None, None),
        (
            "a tab before a space after a line feed",
            Some("a\nb\tc d"),
            None,
            None,
        ),
        (
            "6 bytes between line feeds",
            Some("x\n123456\nz"),
            None,
            None,
        ),
        (
            "a short segment after 6 bytes",
            Some("x\n123456\n1\nz"),
            None,
            None,
        ),
        (
            "a no-break space before texture",
            Some("a\n\u{a0}texture"),
            None,
            None,
        ),
        ("textur after a line feed", Some("a\ntextur"), None, None),
        ("control characters", Some("a\tb\rc\u{7f}d"), None, None),
        ("double underscores", Some("a___b"), Some("F__1"), None),
        (
            "a non-ASCII function name and a line feed in the pixel name",
            None,
            Some("f\u{e9}"),
            Some("p\nx"),
        ),
        ("a prefix starting with a digit", Some("9x"), None, None),
    ];
    let osl_names: Vec<Names> = vec![
        ("the defaults", None, None, None),
        ("other names", Some("R3"), Some("F1"), Some("P2")),
        ("an empty function name", None, Some(""), None),
        ("a non-ASCII function name", None, Some("f\u{e9}"), None),
    ];

    let mut cases = Vec::new();
    for (language, oracle_language, processors, names) in [
        (
            GpuLanguage::Msl2_0,
            oracle_gpu::GpuLanguage::Msl20,
            &msl_processors,
            &msl_names,
        ),
        (
            GpuLanguage::Osl1,
            oracle_gpu::GpuLanguage::Osl1,
            &osl_processors,
            &osl_names,
        ),
    ] {
        for (processor_label, processor, width) in processors {
            for (names_label, prefix, function_name, pixel_name) in names {
                cases.push(WrapperCase {
                    label: format!("{processor_label}, {names_label}, in {language:?}"),
                    language,
                    resource_prefix: prefix.unwrap_or("ocio").to_string(),
                    function_name: function_name.unwrap_or("OCIOMain").to_string(),
                    request: GpuShaderRequest::new(
                        processor.clone(),
                        ShaderSettings {
                            language: Some(oracle_language),
                            function_name: function_name.map(String::from),
                            pixel_name: pixel_name.map(String::from),
                            resource_prefix: prefix.map(String::from),
                            texture_max_width: *width,
                            ..ShaderSettings::default()
                        },
                    ),
                });
            }
        }
    }

    let calls: Vec<_> = cases.iter().map(|case| case.request.call()).collect();
    let responses = Oracle::get().batch(&calls, true);
    for (case, response) in cases.iter().zip(responses) {
        let label = case.label.as_str();
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{label}: the oracle failed: {e}")),
        );
        let mut wrapper = GpuShaderClassWrapper::create_class_wrapper(case.language);

        if let Some(raised) = reply.raised() {
            // The class name: the wheel raises before writing anything.
            assert_eq!(
                (raised.kind.as_str(), raised.stage.as_str()),
                ("Exception", "extract"),
                "{label}"
            );
            wrapper
                .prepare_class_wrapper(
                    case.resource_prefix.as_bytes(),
                    case.function_name.as_bytes(),
                    b"",
                )
                .unwrap();
            let header = wrapper.get_class_wrapper_header(b"").unwrap_err();
            assert_eq!(header.message(), raised.message, "{label}");
            let footer = wrapper.get_class_wrapper_footer(b"").unwrap_err();
            assert_eq!(footer.message(), raised.message, "{label}");
            continue;
        }

        // The description passes the wrapper its names as its getters give them (the setters
        // make `__` `_`).
        let shader = reply.shader();
        let getter = |key: &str| shader.getters[key].as_str().expect(label).as_bytes();
        let (resource_prefix, function_name) = (getter("resource_prefix"), getter("function_name"));
        let text = shader.text.as_str();
        let header = &text[..declarations_end(text)];
        let (declarations, footer_start) = match case.language {
            GpuLanguage::Msl2_0 => (
                &header[header.find("\n}\n\n").expect(label) + 4..],
                text.find("\n// Close class wrapper\n").expect(label),
            ),
            _ => (
                &header[header.rfind("\n{\n").expect(label) + 3..],
                text.rfind("\noutColor = ").expect(label),
            ),
        };

        wrapper
            .prepare_class_wrapper(resource_prefix, function_name, declarations.as_bytes())
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        let written = wrapper
            .get_class_wrapper_header(declarations.as_bytes())
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_text_eq(label, header, std::str::from_utf8(&written).unwrap());
        let written = wrapper
            .get_class_wrapper_footer(&text.as_bytes()[..footer_start])
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_text_eq(label, text, std::str::from_utf8(&written).unwrap());
    }
}
