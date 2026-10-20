// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyTypes` (src/bindings/python/PyTypes.cpp:14-985 @ v2.5.2): the module's classes,
//! declared without their members (each card that binds a class defines them later, as the
//! wheel's `bindPy*` functions do), its enums, and its string constants. The string-conversion
//! functions come with chunk 6.2b's second part.

use pyo3::prelude::*;
use pyo3::types::{PyString, PyType};

use crate::enums::{EnumDef, Scalar, add_enum, conduit};
use crate::pybind11::{self, MODULE};

/// Port of `OCIO_USER_CATEGORIES_ENVVAR` (src/OpenColorIO/Config.cpp:52 @ v2.5.2), which the
/// library itself has not needed yet.
const OCIO_USER_CATEGORIES_ENVVAR: &str = "OCIO_USER_CATEGORIES";

/// The classes `bindPyTypes` declares, in its order, with their base (`None`: pybind11's
/// `pybind11_object`).
const CLASSES: &[(&str, Option<&str>)] = &[
    // py::class_<Baker> (PyTypes.cpp:17 @ v2.5.2).
    ("Baker", None),
    // py::class_<PyBuiltinConfigRegistry> (PyTypes.cpp:21 @ v2.5.2).
    ("BuiltinConfigRegistry", None),
    // py::class_<ColorSpace> (PyTypes.cpp:25 @ v2.5.2).
    ("ColorSpace", None),
    // py::class_<ColorSpaceSet> (PyTypes.cpp:29 @ v2.5.2).
    ("ColorSpaceSet", None),
    // py::class_<Config> (PyTypes.cpp:33 @ v2.5.2).
    ("Config", None),
    // py::class_<Context> (PyTypes.cpp:37 @ v2.5.2).
    ("Context", None),
    // py::class_<CPUProcessor> (PyTypes.cpp:41 @ v2.5.2).
    ("CPUProcessor", None),
    // py::class_<FileRules> (PyTypes.cpp:45 @ v2.5.2).
    ("FileRules", None),
    // py::class_<GPUProcessor> (PyTypes.cpp:49 @ v2.5.2).
    ("GPUProcessor", None),
    // py::class_<GpuShaderCreator> (PyTypes.cpp:53 @ v2.5.2).
    ("GpuShaderCreator", None),
    // py::class_<GpuShaderDesc> (PyTypes.cpp:57 @ v2.5.2).
    ("GpuShaderDesc", Some("GpuShaderCreator")),
    // py::class_<PyImageDesc> (PyTypes.cpp:61 @ v2.5.2).
    ("ImageDesc", None),
    // py::class_<PyPackedImageDesc> (PyTypes.cpp:65 @ v2.5.2).
    ("PackedImageDesc", Some("ImageDesc")),
    // py::class_<PyPlanarImageDesc> (PyTypes.cpp:69 @ v2.5.2).
    ("PlanarImageDesc", Some("ImageDesc")),
    // py::class_<Look> (PyTypes.cpp:73 @ v2.5.2).
    ("Look", None),
    // py::class_<NamedTransform> (PyTypes.cpp:77 @ v2.5.2).
    ("NamedTransform", None),
    // py::class_<Processor> (PyTypes.cpp:81 @ v2.5.2).
    ("Processor", None),
    // py::class_<ProcessorMetadata> (PyTypes.cpp:85 @ v2.5.2).
    ("ProcessorMetadata", None),
    // py::class_<PySystemMonitors> (PyTypes.cpp:89 @ v2.5.2).
    ("SystemMonitors", None),
    // py::class_<ViewingRules> (PyTypes.cpp:93 @ v2.5.2).
    ("ViewingRules", None),
    // py::class_<ViewTransform> (PyTypes.cpp:97 @ v2.5.2).
    ("ViewTransform", None),
    // py::class_<PyBuiltinTransformRegistry> (PyTypes.cpp:102 @ v2.5.2).
    ("BuiltinTransformRegistry", None),
    // py::class_<PyDynamicProperty> (PyTypes.cpp:106 @ v2.5.2).
    ("DynamicProperty", None),
    // py::class_<FormatMetadata> (PyTypes.cpp:110 @ v2.5.2).
    ("FormatMetadata", None),
    // py::class_<GradingRGBM> (PyTypes.cpp:114 @ v2.5.2).
    ("GradingRGBM", None),
    // py::class_<GradingPrimary> (PyTypes.cpp:118 @ v2.5.2).
    ("GradingPrimary", None),
    // py::class_<GradingRGBMSW> (PyTypes.cpp:122 @ v2.5.2).
    ("GradingRGBMSW", None),
    // py::class_<GradingTone> (PyTypes.cpp:126 @ v2.5.2).
    ("GradingTone", None),
    // py::class_<GradingControlPoint> (PyTypes.cpp:130 @ v2.5.2).
    ("GradingControlPoint", None),
    // py::class_<GradingBSplineCurve> (PyTypes.cpp:134 @ v2.5.2).
    ("GradingBSplineCurve", None),
    // py::class_<GradingRGBCurve> (PyTypes.cpp:138 @ v2.5.2).
    ("GradingRGBCurve", None),
    // py::class_<GradingHueCurve> (PyTypes.cpp:142 @ v2.5.2).
    ("GradingHueCurve", None),
    // py::class_<Transform> (PyTypes.cpp:146 @ v2.5.2).
    ("Transform", None),
    // py::class_<AllocationTransform> (PyTypes.cpp:150 @ v2.5.2).
    ("AllocationTransform", Some("Transform")),
    // py::class_<BuiltinTransform> (PyTypes.cpp:156 @ v2.5.2).
    ("BuiltinTransform", Some("Transform")),
    // py::class_<CDLTransform> (PyTypes.cpp:160 @ v2.5.2).
    ("CDLTransform", Some("Transform")),
    // py::class_<ColorSpaceTransform> (PyTypes.cpp:164 @ v2.5.2).
    ("ColorSpaceTransform", Some("Transform")),
    // py::class_<DisplayViewTransform> (PyTypes.cpp:170 @ v2.5.2).
    ("DisplayViewTransform", Some("Transform")),
    // py::class_<ExponentTransform> (PyTypes.cpp:176 @ v2.5.2).
    ("ExponentTransform", Some("Transform")),
    // py::class_<ExponentWithLinearTransform> (PyTypes.cpp:180 @ v2.5.2).
    ("ExponentWithLinearTransform", Some("Transform")),
    // py::class_<ExposureContrastTransform> (PyTypes.cpp:186 @ v2.5.2).
    ("ExposureContrastTransform", Some("Transform")),
    // py::class_<FileTransform> (PyTypes.cpp:192 @ v2.5.2).
    ("FileTransform", Some("Transform")),
    // py::class_<FixedFunctionTransform> (PyTypes.cpp:196 @ v2.5.2).
    ("FixedFunctionTransform", Some("Transform")),
    // py::class_<GradingPrimaryTransform> (PyTypes.cpp:202 @ v2.5.2).
    ("GradingPrimaryTransform", Some("Transform")),
    // py::class_<GradingRGBCurveTransform> (PyTypes.cpp:208 @ v2.5.2).
    ("GradingRGBCurveTransform", Some("Transform")),
    // py::class_<GradingHueCurveTransform> (PyTypes.cpp:214 @ v2.5.2).
    ("GradingHueCurveTransform", Some("Transform")),
    // py::class_<GradingToneTransform> (PyTypes.cpp:220 @ v2.5.2).
    ("GradingToneTransform", Some("Transform")),
    // py::class_<GroupTransform> (PyTypes.cpp:226 @ v2.5.2).
    ("GroupTransform", Some("Transform")),
    // py::class_<LogAffineTransform> (PyTypes.cpp:230 @ v2.5.2).
    ("LogAffineTransform", Some("Transform")),
    // py::class_<LogCameraTransform> (PyTypes.cpp:234 @ v2.5.2).
    ("LogCameraTransform", Some("Transform")),
    // py::class_<LogTransform> (PyTypes.cpp:238 @ v2.5.2).
    ("LogTransform", Some("Transform")),
    // py::class_<LookTransform> (PyTypes.cpp:242 @ v2.5.2).
    ("LookTransform", Some("Transform")),
    // py::class_<Lut1DTransform> (PyTypes.cpp:246 @ v2.5.2).
    ("Lut1DTransform", Some("Transform")),
    // py::class_<Lut3DTransform> (PyTypes.cpp:250 @ v2.5.2).
    ("Lut3DTransform", Some("Transform")),
    // py::class_<MatrixTransform> (PyTypes.cpp:254 @ v2.5.2).
    ("MatrixTransform", Some("Transform")),
    // py::class_<RangeTransform> (PyTypes.cpp:258 @ v2.5.2).
    ("RangeTransform", Some("Transform")),
    // py::class_<ColorSpaceMenuParameters> (PyTypes.cpp:263 @ v2.5.2).
    ("ColorSpaceMenuParameters", None),
    // py::class_<ColorSpaceMenuHelper> (PyTypes.cpp:267 @ v2.5.2).
    ("ColorSpaceMenuHelper", None),
    // py::class_<LegacyViewingPipeline> (PyTypes.cpp:271 @ v2.5.2).
    ("LegacyViewingPipeline", None),
    // py::class_<MixingSlider> (PyTypes.cpp:275 @ v2.5.2).
    ("MixingSlider", None),
    // py::class_<MixingColorSpaceManager> (PyTypes.cpp:279 @ v2.5.2).
    ("MixingColorSpaceManager", None),
    // py::class_<ConfigMergingParameters> (PyTypes.cpp:283 @ v2.5.2).
    ("ConfigMergingParameters", None),
    // py::class_<ConfigMerger> (PyTypes.cpp:287 @ v2.5.2).
    ("ConfigMerger", None),
];

/// The enums `bindPyTypes` binds, in its order: their names, C++ underlying types and values
/// (include/OpenColorIO/OpenColorTypes.h @ v2.5.2).
#[rustfmt::skip]
const ENUMS: &[EnumDef] = &[
    // py::enum_<LoggingLevel> (PyTypes.cpp:292-306 @ v2.5.2), underlying int.
    EnumDef { name: "LoggingLevel", scalar: Scalar::Int, arithmetic: true, entries: &[("LOGGING_LEVEL_NONE", 0), ("LOGGING_LEVEL_WARNING", 1), ("LOGGING_LEVEL_INFO", 2), ("LOGGING_LEVEL_DEBUG", 3), ("LOGGING_LEVEL_UNKNOWN", 255)] },
    // py::enum_<ReferenceSpaceType> (PyTypes.cpp:308-316 @ v2.5.2), underlying int.
    EnumDef { name: "ReferenceSpaceType", scalar: Scalar::Int, arithmetic: false, entries: &[("REFERENCE_SPACE_SCENE", 0), ("REFERENCE_SPACE_DISPLAY", 1)] },
    // py::enum_<SearchReferenceSpaceType> (PyTypes.cpp:318-328 @ v2.5.2), underlying int.
    EnumDef { name: "SearchReferenceSpaceType", scalar: Scalar::Int, arithmetic: false, entries: &[("SEARCH_REFERENCE_SPACE_SCENE", 0), ("SEARCH_REFERENCE_SPACE_DISPLAY", 1), ("SEARCH_REFERENCE_SPACE_ALL", 2)] },
    // py::enum_<NamedTransformVisibility> (PyTypes.cpp:330-340 @ v2.5.2), underlying int.
    EnumDef { name: "NamedTransformVisibility", scalar: Scalar::Int, arithmetic: false, entries: &[("NAMEDTRANSFORM_ACTIVE", 0), ("NAMEDTRANSFORM_INACTIVE", 1), ("NAMEDTRANSFORM_ALL", 2)] },
    // py::enum_<ColorSpaceVisibility> (PyTypes.cpp:342-352 @ v2.5.2), underlying int.
    EnumDef { name: "ColorSpaceVisibility", scalar: Scalar::Int, arithmetic: false, entries: &[("COLORSPACE_ACTIVE", 0), ("COLORSPACE_INACTIVE", 1), ("COLORSPACE_ALL", 2)] },
    // py::enum_<ViewType> (PyTypes.cpp:354-362 @ v2.5.2), underlying int.
    EnumDef { name: "ViewType", scalar: Scalar::Int, arithmetic: false, entries: &[("VIEW_SHARED", 0), ("VIEW_DISPLAY_DEFINED", 1)] },
    // py::enum_<ColorSpaceDirection> (PyTypes.cpp:364-372 @ v2.5.2), underlying int.
    EnumDef { name: "ColorSpaceDirection", scalar: Scalar::Int, arithmetic: false, entries: &[("COLORSPACE_DIR_TO_REFERENCE", 0), ("COLORSPACE_DIR_FROM_REFERENCE", 1)] },
    // py::enum_<ViewTransformDirection> (PyTypes.cpp:374-382 @ v2.5.2), underlying int.
    EnumDef { name: "ViewTransformDirection", scalar: Scalar::Int, arithmetic: false, entries: &[("VIEWTRANSFORM_DIR_TO_REFERENCE", 0), ("VIEWTRANSFORM_DIR_FROM_REFERENCE", 1)] },
    // py::enum_<TransformDirection> (PyTypes.cpp:384-392 @ v2.5.2), underlying int.
    EnumDef { name: "TransformDirection", scalar: Scalar::Int, arithmetic: false, entries: &[("TRANSFORM_DIR_FORWARD", 0), ("TRANSFORM_DIR_INVERSE", 1)] },
    // py::enum_<TransformType> (PyTypes.cpp:394-444 @ v2.5.2), underlying int.
    EnumDef { name: "TransformType", scalar: Scalar::Int, arithmetic: false, entries: &[("TRANSFORM_TYPE_ALLOCATION", 0), ("TRANSFORM_TYPE_BUILTIN", 1), ("TRANSFORM_TYPE_CDL", 2), ("TRANSFORM_TYPE_COLORSPACE", 3), ("TRANSFORM_TYPE_DISPLAY_VIEW", 4), ("TRANSFORM_TYPE_EXPONENT", 5), ("TRANSFORM_TYPE_EXPONENT_WITH_LINEAR", 6), ("TRANSFORM_TYPE_EXPOSURE_CONTRAST", 7), ("TRANSFORM_TYPE_FILE", 8), ("TRANSFORM_TYPE_FIXED_FUNCTION", 9), ("TRANSFORM_TYPE_GRADING_PRIMARY", 11), ("TRANSFORM_TYPE_GRADING_RGB_CURVE", 12), ("TRANSFORM_TYPE_GRADING_HUE_CURVE", 10), ("TRANSFORM_TYPE_GRADING_TONE", 13), ("TRANSFORM_TYPE_GROUP", 14), ("TRANSFORM_TYPE_LOG_AFFINE", 15), ("TRANSFORM_TYPE_LOG_CAMERA", 16), ("TRANSFORM_TYPE_LOG", 17), ("TRANSFORM_TYPE_LOOK", 18), ("TRANSFORM_TYPE_LUT1D", 19), ("TRANSFORM_TYPE_LUT3D", 20), ("TRANSFORM_TYPE_MATRIX", 21), ("TRANSFORM_TYPE_RANGE", 22)] },
    // py::enum_<Interpolation> (PyTypes.cpp:446-464 @ v2.5.2), underlying int.
    EnumDef { name: "Interpolation", scalar: Scalar::Int, arithmetic: false, entries: &[("INTERP_UNKNOWN", 0), ("INTERP_NEAREST", 1), ("INTERP_LINEAR", 2), ("INTERP_TETRAHEDRAL", 3), ("INTERP_CUBIC", 4), ("INTERP_DEFAULT", 254), ("INTERP_BEST", 255)] },
    // py::enum_<BitDepth> (PyTypes.cpp:466-488 @ v2.5.2), underlying int.
    EnumDef { name: "BitDepth", scalar: Scalar::Int, arithmetic: true, entries: &[("BIT_DEPTH_UNKNOWN", 0), ("BIT_DEPTH_UINT8", 1), ("BIT_DEPTH_UINT10", 2), ("BIT_DEPTH_UINT12", 3), ("BIT_DEPTH_UINT14", 4), ("BIT_DEPTH_UINT16", 5), ("BIT_DEPTH_UINT32", 6), ("BIT_DEPTH_F16", 7), ("BIT_DEPTH_F32", 8)] },
    // py::enum_<Lut1DHueAdjust> (PyTypes.cpp:490-500 @ v2.5.2), underlying int.
    EnumDef { name: "Lut1DHueAdjust", scalar: Scalar::Int, arithmetic: false, entries: &[("HUE_NONE", 0), ("HUE_DW3", 1), ("HUE_WYPN", 2)] },
    // py::enum_<ChannelOrdering> (PyTypes.cpp:502-516 @ v2.5.2), underlying int.
    EnumDef { name: "ChannelOrdering", scalar: Scalar::Int, arithmetic: false, entries: &[("CHANNEL_ORDERING_RGBA", 0), ("CHANNEL_ORDERING_BGRA", 1), ("CHANNEL_ORDERING_ABGR", 2), ("CHANNEL_ORDERING_RGB", 3), ("CHANNEL_ORDERING_BGR", 4)] },
    // py::enum_<Allocation> (PyTypes.cpp:518-528 @ v2.5.2), underlying int.
    EnumDef { name: "Allocation", scalar: Scalar::Int, arithmetic: false, entries: &[("ALLOCATION_UNKNOWN", 0), ("ALLOCATION_UNIFORM", 1), ("ALLOCATION_LG2", 2)] },
    // py::enum_<GpuLanguage> (PyTypes.cpp:530-554 @ v2.5.2), underlying int.
    EnumDef { name: "GpuLanguage", scalar: Scalar::Int, arithmetic: false, entries: &[("GPU_LANGUAGE_CG", 0), ("GPU_LANGUAGE_GLSL_1_2", 1), ("GPU_LANGUAGE_GLSL_1_3", 2), ("GPU_LANGUAGE_GLSL_4_0", 3), ("GPU_LANGUAGE_GLSL_VK_4_6", 4), ("GPU_LANGUAGE_GLSL_ES_1_0", 7), ("GPU_LANGUAGE_GLSL_ES_3_0", 8), ("GPU_LANGUAGE_HLSL_DX11", 5), ("GPU_LANGUAGE_MSL_2_0", 9), ("LANGUAGE_OSL_1", 6)] },
    // py::enum_<EnvironmentMode> (PyTypes.cpp:556-566 @ v2.5.2), underlying int.
    EnumDef { name: "EnvironmentMode", scalar: Scalar::Int, arithmetic: false, entries: &[("ENV_ENVIRONMENT_UNKNOWN", 0), ("ENV_ENVIRONMENT_LOAD_PREDEFINED", 1), ("ENV_ENVIRONMENT_LOAD_ALL", 2)] },
    // py::enum_<RangeStyle> (PyTypes.cpp:568-576 @ v2.5.2), underlying int.
    EnumDef { name: "RangeStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("RANGE_NO_CLAMP", 0), ("RANGE_CLAMP", 1)] },
    // py::enum_<FixedFunctionStyle> (PyTypes.cpp:578-628 @ v2.5.2), underlying int.
    EnumDef { name: "FixedFunctionStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("FIXED_FUNCTION_ACES_RED_MOD_03", 0), ("FIXED_FUNCTION_ACES_RED_MOD_10", 1), ("FIXED_FUNCTION_ACES_GLOW_03", 2), ("FIXED_FUNCTION_ACES_GLOW_10", 3), ("FIXED_FUNCTION_ACES_DARK_TO_DIM_10", 4), ("FIXED_FUNCTION_REC2100_SURROUND", 5), ("FIXED_FUNCTION_RGB_TO_HSV", 6), ("FIXED_FUNCTION_XYZ_TO_xyY", 7), ("FIXED_FUNCTION_XYZ_TO_uvY", 8), ("FIXED_FUNCTION_XYZ_TO_LUV", 9), ("FIXED_FUNCTION_ACES_GAMUTMAP_02", 10), ("FIXED_FUNCTION_ACES_GAMUTMAP_07", 11), ("FIXED_FUNCTION_ACES_GAMUT_COMP_13", 12), ("FIXED_FUNCTION_LIN_TO_PQ", 13), ("FIXED_FUNCTION_LIN_TO_GAMMA_LOG", 14), ("FIXED_FUNCTION_LIN_TO_DOUBLE_LOG", 15), ("FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20", 16), ("FIXED_FUNCTION_ACES_RGB_TO_JMH_20", 17), ("FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20", 18), ("FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20", 19), ("FIXED_FUNCTION_RGB_TO_HSY_LIN", 20), ("FIXED_FUNCTION_RGB_TO_HSY_LOG", 21), ("FIXED_FUNCTION_RGB_TO_HSY_VID", 22)] },
    // py::enum_<ExposureContrastStyle> (PyTypes.cpp:630-640 @ v2.5.2), underlying int.
    EnumDef { name: "ExposureContrastStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("EXPOSURE_CONTRAST_LINEAR", 0), ("EXPOSURE_CONTRAST_VIDEO", 1), ("EXPOSURE_CONTRAST_LOGARITHMIC", 2)] },
    // py::enum_<CDLStyle> (PyTypes.cpp:642-652 @ v2.5.2), underlying int.
    EnumDef { name: "CDLStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("CDL_ASC", 0), ("CDL_NO_CLAMP", 1), ("CDL_TRANSFORM_DEFAULT", 1)] },
    // py::enum_<NegativeStyle> (PyTypes.cpp:654-666 @ v2.5.2), underlying int.
    EnumDef { name: "NegativeStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("NEGATIVE_CLAMP", 0), ("NEGATIVE_MIRROR", 1), ("NEGATIVE_PASS_THRU", 2), ("NEGATIVE_LINEAR", 3)] },
    // py::enum_<GradingStyle> (PyTypes.cpp:668-678 @ v2.5.2), underlying int.
    EnumDef { name: "GradingStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("GRADING_LOG", 0), ("GRADING_LIN", 1), ("GRADING_VIDEO", 2)] },
    // py::enum_<DynamicPropertyType> (PyTypes.cpp:680-698 @ v2.5.2), underlying int.
    EnumDef { name: "DynamicPropertyType", scalar: Scalar::Int, arithmetic: false, entries: &[("DYNAMIC_PROPERTY_EXPOSURE", 0), ("DYNAMIC_PROPERTY_CONTRAST", 1), ("DYNAMIC_PROPERTY_GAMMA", 2), ("DYNAMIC_PROPERTY_GRADING_PRIMARY", 3), ("DYNAMIC_PROPERTY_GRADING_RGBCURVE", 4), ("DYNAMIC_PROPERTY_GRADING_HUECURVE", 6), ("DYNAMIC_PROPERTY_GRADING_TONE", 5)] },
    // py::enum_<RGBCurveType> (PyTypes.cpp:700-714 @ v2.5.2), underlying int.
    EnumDef { name: "RGBCurveType", scalar: Scalar::Int, arithmetic: false, entries: &[("RGB_RED", 0), ("RGB_GREEN", 1), ("RGB_BLUE", 2), ("RGB_MASTER", 3), ("RGB_NUM_CURVES", 4)] },
    // py::enum_<HueCurveType> (PyTypes.cpp:716-736 @ v2.5.2), underlying int.
    EnumDef { name: "HueCurveType", scalar: Scalar::Int, arithmetic: false, entries: &[("HUE_HUE", 0), ("HUE_SAT", 1), ("HUE_LUM", 2), ("LUM_SAT", 3), ("SAT_SAT", 4), ("LUM_LUM", 5), ("SAT_LUM", 6), ("HUE_FX", 7)] },
    // py::enum_<HSYTransformStyle> (PyTypes.cpp:738-746 @ v2.5.2), underlying int.
    EnumDef { name: "HSYTransformStyle", scalar: Scalar::Int, arithmetic: false, entries: &[("HSY_TRANSFORM_NONE", 0), ("HSY_TRANSFORM_1", 1)] },
    // py::enum_<BSplineType> (PyTypes.cpp:748-764 @ v2.5.2), underlying int.
    EnumDef { name: "BSplineType", scalar: Scalar::Int, arithmetic: false, entries: &[("B_SPLINE", 0), ("DIAGONAL_B_SPLINE", 1), ("HUE_HUE_B_SPLINE", 2), ("PERIODIC_1_B_SPLINE", 3), ("PERIODIC_0_B_SPLINE", 4), ("HORIZONTAL1_B_SPLINE", 5)] },
    // py::enum_<UniformDataType> (PyTypes.cpp:766-782 @ v2.5.2), underlying int.
    EnumDef { name: "UniformDataType", scalar: Scalar::Int, arithmetic: false, entries: &[("UNIFORM_DOUBLE", 0), ("UNIFORM_BOOL", 1), ("UNIFORM_FLOAT3", 2), ("UNIFORM_VECTOR_FLOAT", 3), ("UNIFORM_VECTOR_INT", 4), ("UNIFORM_UNKNOWN", 5)] },
    // py::enum_<OptimizationFlags> (PyTypes.cpp:784-846 @ v2.5.2), underlying unsigned long.
    EnumDef { name: "OptimizationFlags", scalar: Scalar::ULong, arithmetic: true, entries: &[("OPTIMIZATION_NONE", 0), ("OPTIMIZATION_IDENTITY", 1), ("OPTIMIZATION_IDENTITY_GAMMA", 2), ("OPTIMIZATION_PAIR_IDENTITY_CDL", 64), ("OPTIMIZATION_PAIR_IDENTITY_EXPOSURE_CONTRAST", 128), ("OPTIMIZATION_PAIR_IDENTITY_FIXED_FUNCTION", 256), ("OPTIMIZATION_PAIR_IDENTITY_GAMMA", 512), ("OPTIMIZATION_PAIR_IDENTITY_LUT1D", 1024), ("OPTIMIZATION_PAIR_IDENTITY_LUT3D", 2048), ("OPTIMIZATION_PAIR_IDENTITY_LOG", 4096), ("OPTIMIZATION_PAIR_IDENTITY_GRADING", 8192), ("OPTIMIZATION_COMP_EXPONENT", 262144), ("OPTIMIZATION_COMP_GAMMA", 524288), ("OPTIMIZATION_COMP_MATRIX", 1048576), ("OPTIMIZATION_COMP_LUT1D", 2097152), ("OPTIMIZATION_COMP_LUT3D", 4194304), ("OPTIMIZATION_COMP_RANGE", 8388608), ("OPTIMIZATION_COMP_SEPARABLE_PREFIX", 16777216), ("OPTIMIZATION_LUT_INV_FAST", 33554432), ("OPTIMIZATION_FAST_LOG_EXP_POW", 67108864), ("OPTIMIZATION_SIMPLIFY_OPS", 134217728), ("OPTIMIZATION_NO_DYNAMIC_PROPERTIES", 268435456), ("OPTIMIZATION_ALL", 4294967295), ("OPTIMIZATION_LOSSLESS", 144457667), ("OPTIMIZATION_VERY_GOOD", 263995331), ("OPTIMIZATION_GOOD", 268189635), ("OPTIMIZATION_DRAFT", 4294967295), ("OPTIMIZATION_DEFAULT", 263995331)] },
    // py::enum_<ProcessorCacheFlags> (PyTypes.cpp:848-860 @ v2.5.2), underlying unsigned int.
    EnumDef { name: "ProcessorCacheFlags", scalar: Scalar::UInt, arithmetic: false, entries: &[("PROCESSOR_CACHE_OFF", 0), ("PROCESSOR_CACHE_ENABLED", 1), ("PROCESSOR_CACHE_SHARE_DYN_PROPERTIES", 2), ("PROCESSOR_CACHE_DEFAULT", 3)] },
];

/// Declares a class as `py::class_<T>(m, name, DOC(T))` does (the wheel's docstrings are
/// empty): `__init__` the slot wrapper that raises "No constructor defined!" until a
/// constructor is bound, `__doc__`, `__module__`, then `_pybind11_conduit_v1_`.
fn declare_class<'py>(
    m: &Bound<'py, PyModule>,
    name: &str,
    base: Option<&str>,
) -> PyResult<Bound<'py, PyType>> {
    let py = m.py();
    let bases = match base {
        Some(base) => vec![m.getattr(base)?.cast_into::<PyType>()?],
        None => vec![],
    };
    let cls = pybind11::new_class(
        py,
        name,
        name,
        &bases,
        &[
            ("__init__", pybind11::no_constructor(py)?),
            ("__doc__", PyString::new(py, "").into_any()),
            ("__module__", PyString::new(py, MODULE).into_any()),
        ],
    )?;
    cls.setattr("_pybind11_conduit_v1_", conduit(py)?)?;
    m.setattr(name, &cls)?;
    Ok(cls)
}

/// Port of `bindPyTypes` (src/bindings/python/PyTypes.cpp:14-985 @ v2.5.2), but for the
/// string-conversion functions (862-940).
pub(crate) fn bind_py_types(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // The classes (17-290).
    for &(name, base) in CLASSES {
        declare_class(m, name, base)?;
    }
    // The enums (292-857).
    for def in ENUMS {
        add_enum(m.as_any(), None, def)?;
    }
    // Env. variables (942-948).
    m.setattr("OCIO_CONFIG_ENVVAR", ocio::config::OCIO_CONFIG_ENVVAR)?;
    m.setattr(
        "OCIO_ACTIVE_DISPLAYS_ENVVAR",
        ocio::config::OCIO_ACTIVE_DISPLAYS_ENVVAR,
    )?;
    m.setattr(
        "OCIO_ACTIVE_VIEWS_ENVVAR",
        ocio::config::OCIO_ACTIVE_VIEWS_ENVVAR,
    )?;
    m.setattr(
        "OCIO_INACTIVE_COLORSPACES_ENVVAR",
        ocio::config::OCIO_INACTIVE_COLORSPACES_ENVVAR,
    )?;
    m.setattr(
        "OCIO_OPTIMIZATION_FLAGS_ENVVAR",
        ocio::processor::OCIO_OPTIMIZATION_FLAGS_ENVVAR,
    )?;
    m.setattr("OCIO_USER_CATEGORIES_ENVVAR", OCIO_USER_CATEGORIES_ENVVAR)?;
    // Roles (950-962).
    {
        use ocio_ops::parse_utils::*;
        m.setattr("ROLE_DEFAULT", ROLE_DEFAULT)?;
        m.setattr("ROLE_REFERENCE", ROLE_REFERENCE)?;
        m.setattr("ROLE_DATA", ROLE_DATA)?;
        m.setattr("ROLE_COLOR_PICKING", ROLE_COLOR_PICKING)?;
        m.setattr("ROLE_SCENE_LINEAR", ROLE_SCENE_LINEAR)?;
        m.setattr("ROLE_COMPOSITING_LOG", ROLE_COMPOSITING_LOG)?;
        m.setattr("ROLE_COLOR_TIMING", ROLE_COLOR_TIMING)?;
        m.setattr("ROLE_TEXTURE_PAINT", ROLE_TEXTURE_PAINT)?;
        m.setattr("ROLE_MATTE_PAINT", ROLE_MATTE_PAINT)?;
        m.setattr("ROLE_RENDERING", ROLE_RENDERING)?;
        m.setattr("ROLE_INTERCHANGE_SCENE", ROLE_INTERCHANGE_SCENE)?;
        m.setattr("ROLE_INTERCHANGE_DISPLAY", ROLE_INTERCHANGE_DISPLAY)?;
    }
    // Shared View (964-965).
    m.setattr(
        "OCIO_VIEW_USE_DISPLAY_NAME",
        ocio::OCIO_VIEW_USE_DISPLAY_NAME,
    )?;
    // FormatMetadata (967-973): ASCII names, held as bytes by the port.
    {
        use ocio_ops::format_metadata::*;
        for (name, value) in [
            ("METADATA_DESCRIPTION", METADATA_DESCRIPTION),
            ("METADATA_INFO", METADATA_INFO),
            ("METADATA_INPUT_DESCRIPTOR", METADATA_INPUT_DESCRIPTOR),
            ("METADATA_OUTPUT_DESCRIPTOR", METADATA_OUTPUT_DESCRIPTOR),
            ("METADATA_NAME", METADATA_NAME),
            ("METADATA_ID", METADATA_ID),
        ] {
            let text = std::str::from_utf8(value)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            m.setattr(name, text)?;
        }
    }
    // Caches (975-978).
    m.setattr(
        "OCIO_DISABLE_ALL_CACHES",
        ocio::caching::OCIO_DISABLE_ALL_CACHES,
    )?;
    m.setattr(
        "OCIO_DISABLE_PROCESSOR_CACHES",
        ocio::caching::OCIO_DISABLE_PROCESSOR_CACHES,
    )?;
    m.setattr(
        "OCIO_DISABLE_CACHE_FALLBACK",
        ocio::caching::OCIO_DISABLE_CACHE_FALLBACK,
    )?;
    // 980-984.
    m.setattr(
        "OCIO_CONFIG_DEFAULT_NAME",
        ocio::config::OCIO_CONFIG_DEFAULT_NAME,
    )?;
    m.setattr(
        "OCIO_CONFIG_DEFAULT_FILE_EXT",
        ocio::config::OCIO_CONFIG_DEFAULT_FILE_EXT,
    )?;
    m.setattr(
        "OCIO_CONFIG_ARCHIVE_FILE_EXT",
        ocio::config::OCIO_CONFIG_ARCHIVE_FILE_EXT,
    )?;
    m.setattr("OCIO_BUILTIN_URI_PREFIX", ocio::OCIO_BUILTIN_URI_PREFIX)?;
    Ok(())
}
