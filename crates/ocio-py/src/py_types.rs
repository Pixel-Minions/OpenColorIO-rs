// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyTypes` (src/bindings/python/PyTypes.cpp:14-985 @ v2.5.2): the module's classes,
//! declared without their members (each card that binds a class defines them later, as the
//! wheel's `bindPy*` functions do), its enums, its string-conversion functions and its string
//! constants.

use ocio::transforms::range_transform::{
    RangeStyle, range_style_from_string, range_style_to_string,
};
use ocio_gpu::open_color_types::{GpuLanguage, gpu_language_from_string, gpu_language_to_string};
use ocio_ops::open_color_types::{
    Allocation, BitDepth, CdlStyle, EnvironmentMode, FixedFunctionStyle, GradingStyle,
    Interpolation, LoggingLevel, NegativeStyle, TransformDirection, allocation_to_string,
    bit_depth_to_string, cdl_style_to_string, combine_transform_directions,
    fixed_function_style_from_string, fixed_function_style_to_string,
    get_inverse_transform_direction, grading_style_to_string, interpolation_to_string,
    logging_level_from_string, logging_level_to_string, negative_style_to_string,
    transform_direction_to_string,
};
use ocio_ops::parse_utils::{
    allocation_from_string, bit_depth_from_string, bit_depth_is_float, bit_depth_to_int,
    bool_from_string, bool_to_string, cdl_style_from_string, environment_mode_from_string,
    environment_mode_to_string, interpolation_from_string, negative_style_from_string,
    transform_direction_from_string,
};
use ocio_ops::utils::string_utils::{c_str, lower};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyString, PyType};

use crate::arg;
use crate::dispatch::{CharPtr, Function, Overload, Param};
use crate::enums::{EnumDef, Scalar, add_enum, conduit, enum_class, enum_value, to_py};
use crate::py_open_color_io::{function, py_result, to_py_err};
use crate::pybind11;

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
    // py::enum_<LoggingLevel> (PyTypes.cpp:292-306 @ v2.5.2), no fixed type.
    EnumDef { name: "LoggingLevel", scalar: Scalar::Unfixed, arithmetic: true, value_docs: true, entries: &[("LOGGING_LEVEL_NONE", 0), ("LOGGING_LEVEL_WARNING", 1), ("LOGGING_LEVEL_INFO", 2), ("LOGGING_LEVEL_DEBUG", 3), ("LOGGING_LEVEL_UNKNOWN", 255)] },
    // py::enum_<ReferenceSpaceType> (PyTypes.cpp:308-316 @ v2.5.2), no fixed type.
    EnumDef { name: "ReferenceSpaceType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("REFERENCE_SPACE_SCENE", 0), ("REFERENCE_SPACE_DISPLAY", 1)] },
    // py::enum_<SearchReferenceSpaceType> (PyTypes.cpp:318-328 @ v2.5.2), no fixed type.
    EnumDef { name: "SearchReferenceSpaceType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("SEARCH_REFERENCE_SPACE_SCENE", 0), ("SEARCH_REFERENCE_SPACE_DISPLAY", 1), ("SEARCH_REFERENCE_SPACE_ALL", 2)] },
    // py::enum_<NamedTransformVisibility> (PyTypes.cpp:330-340 @ v2.5.2), no fixed type.
    EnumDef { name: "NamedTransformVisibility", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("NAMEDTRANSFORM_ACTIVE", 0), ("NAMEDTRANSFORM_INACTIVE", 1), ("NAMEDTRANSFORM_ALL", 2)] },
    // py::enum_<ColorSpaceVisibility> (PyTypes.cpp:342-352 @ v2.5.2), no fixed type.
    EnumDef { name: "ColorSpaceVisibility", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("COLORSPACE_ACTIVE", 0), ("COLORSPACE_INACTIVE", 1), ("COLORSPACE_ALL", 2)] },
    // py::enum_<ViewType> (PyTypes.cpp:354-362 @ v2.5.2), no fixed type.
    EnumDef { name: "ViewType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("VIEW_SHARED", 0), ("VIEW_DISPLAY_DEFINED", 1)] },
    // py::enum_<ColorSpaceDirection> (PyTypes.cpp:364-372 @ v2.5.2), no fixed type.
    EnumDef { name: "ColorSpaceDirection", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("COLORSPACE_DIR_TO_REFERENCE", 0), ("COLORSPACE_DIR_FROM_REFERENCE", 1)] },
    // py::enum_<ViewTransformDirection> (PyTypes.cpp:374-382 @ v2.5.2), no fixed type.
    EnumDef { name: "ViewTransformDirection", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("VIEWTRANSFORM_DIR_TO_REFERENCE", 0), ("VIEWTRANSFORM_DIR_FROM_REFERENCE", 1)] },
    // py::enum_<TransformDirection> (PyTypes.cpp:384-392 @ v2.5.2), no fixed type.
    EnumDef { name: "TransformDirection", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("TRANSFORM_DIR_FORWARD", 0), ("TRANSFORM_DIR_INVERSE", 1)] },
    // py::enum_<TransformType> (PyTypes.cpp:394-444 @ v2.5.2), no fixed type.
    EnumDef { name: "TransformType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("TRANSFORM_TYPE_ALLOCATION", 0), ("TRANSFORM_TYPE_BUILTIN", 1), ("TRANSFORM_TYPE_CDL", 2), ("TRANSFORM_TYPE_COLORSPACE", 3), ("TRANSFORM_TYPE_DISPLAY_VIEW", 4), ("TRANSFORM_TYPE_EXPONENT", 5), ("TRANSFORM_TYPE_EXPONENT_WITH_LINEAR", 6), ("TRANSFORM_TYPE_EXPOSURE_CONTRAST", 7), ("TRANSFORM_TYPE_FILE", 8), ("TRANSFORM_TYPE_FIXED_FUNCTION", 9), ("TRANSFORM_TYPE_GRADING_PRIMARY", 11), ("TRANSFORM_TYPE_GRADING_RGB_CURVE", 12), ("TRANSFORM_TYPE_GRADING_HUE_CURVE", 10), ("TRANSFORM_TYPE_GRADING_TONE", 13), ("TRANSFORM_TYPE_GROUP", 14), ("TRANSFORM_TYPE_LOG_AFFINE", 15), ("TRANSFORM_TYPE_LOG_CAMERA", 16), ("TRANSFORM_TYPE_LOG", 17), ("TRANSFORM_TYPE_LOOK", 18), ("TRANSFORM_TYPE_LUT1D", 19), ("TRANSFORM_TYPE_LUT3D", 20), ("TRANSFORM_TYPE_MATRIX", 21), ("TRANSFORM_TYPE_RANGE", 22)] },
    // py::enum_<Interpolation> (PyTypes.cpp:446-464 @ v2.5.2), no fixed type.
    EnumDef { name: "Interpolation", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("INTERP_UNKNOWN", 0), ("INTERP_NEAREST", 1), ("INTERP_LINEAR", 2), ("INTERP_TETRAHEDRAL", 3), ("INTERP_CUBIC", 4), ("INTERP_DEFAULT", 254), ("INTERP_BEST", 255)] },
    // py::enum_<BitDepth> (PyTypes.cpp:466-488 @ v2.5.2), no fixed type.
    EnumDef { name: "BitDepth", scalar: Scalar::Unfixed, arithmetic: true, value_docs: true, entries: &[("BIT_DEPTH_UNKNOWN", 0), ("BIT_DEPTH_UINT8", 1), ("BIT_DEPTH_UINT10", 2), ("BIT_DEPTH_UINT12", 3), ("BIT_DEPTH_UINT14", 4), ("BIT_DEPTH_UINT16", 5), ("BIT_DEPTH_UINT32", 6), ("BIT_DEPTH_F16", 7), ("BIT_DEPTH_F32", 8)] },
    // py::enum_<Lut1DHueAdjust> (PyTypes.cpp:490-500 @ v2.5.2), no fixed type.
    EnumDef { name: "Lut1DHueAdjust", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("HUE_NONE", 0), ("HUE_DW3", 1), ("HUE_WYPN", 2)] },
    // py::enum_<ChannelOrdering> (PyTypes.cpp:502-516 @ v2.5.2), no fixed type.
    EnumDef { name: "ChannelOrdering", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("CHANNEL_ORDERING_RGBA", 0), ("CHANNEL_ORDERING_BGRA", 1), ("CHANNEL_ORDERING_ABGR", 2), ("CHANNEL_ORDERING_RGB", 3), ("CHANNEL_ORDERING_BGR", 4)] },
    // py::enum_<Allocation> (PyTypes.cpp:518-528 @ v2.5.2), no fixed type.
    EnumDef { name: "Allocation", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("ALLOCATION_UNKNOWN", 0), ("ALLOCATION_UNIFORM", 1), ("ALLOCATION_LG2", 2)] },
    // py::enum_<GpuLanguage> (PyTypes.cpp:530-554 @ v2.5.2), no fixed type.
    EnumDef { name: "GpuLanguage", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("GPU_LANGUAGE_CG", 0), ("GPU_LANGUAGE_GLSL_1_2", 1), ("GPU_LANGUAGE_GLSL_1_3", 2), ("GPU_LANGUAGE_GLSL_4_0", 3), ("GPU_LANGUAGE_GLSL_VK_4_6", 4), ("GPU_LANGUAGE_GLSL_ES_1_0", 7), ("GPU_LANGUAGE_GLSL_ES_3_0", 8), ("GPU_LANGUAGE_HLSL_DX11", 5), ("GPU_LANGUAGE_MSL_2_0", 9), ("LANGUAGE_OSL_1", 6)] },
    // py::enum_<EnvironmentMode> (PyTypes.cpp:556-566 @ v2.5.2), no fixed type.
    EnumDef { name: "EnvironmentMode", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("ENV_ENVIRONMENT_UNKNOWN", 0), ("ENV_ENVIRONMENT_LOAD_PREDEFINED", 1), ("ENV_ENVIRONMENT_LOAD_ALL", 2)] },
    // py::enum_<RangeStyle> (PyTypes.cpp:568-576 @ v2.5.2), no fixed type.
    EnumDef { name: "RangeStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("RANGE_NO_CLAMP", 0), ("RANGE_CLAMP", 1)] },
    // py::enum_<FixedFunctionStyle> (PyTypes.cpp:578-628 @ v2.5.2), no fixed type.
    EnumDef { name: "FixedFunctionStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("FIXED_FUNCTION_ACES_RED_MOD_03", 0), ("FIXED_FUNCTION_ACES_RED_MOD_10", 1), ("FIXED_FUNCTION_ACES_GLOW_03", 2), ("FIXED_FUNCTION_ACES_GLOW_10", 3), ("FIXED_FUNCTION_ACES_DARK_TO_DIM_10", 4), ("FIXED_FUNCTION_REC2100_SURROUND", 5), ("FIXED_FUNCTION_RGB_TO_HSV", 6), ("FIXED_FUNCTION_XYZ_TO_xyY", 7), ("FIXED_FUNCTION_XYZ_TO_uvY", 8), ("FIXED_FUNCTION_XYZ_TO_LUV", 9), ("FIXED_FUNCTION_ACES_GAMUTMAP_02", 10), ("FIXED_FUNCTION_ACES_GAMUTMAP_07", 11), ("FIXED_FUNCTION_ACES_GAMUT_COMP_13", 12), ("FIXED_FUNCTION_LIN_TO_PQ", 13), ("FIXED_FUNCTION_LIN_TO_GAMMA_LOG", 14), ("FIXED_FUNCTION_LIN_TO_DOUBLE_LOG", 15), ("FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20", 16), ("FIXED_FUNCTION_ACES_RGB_TO_JMH_20", 17), ("FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20", 18), ("FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20", 19), ("FIXED_FUNCTION_RGB_TO_HSY_LIN", 20), ("FIXED_FUNCTION_RGB_TO_HSY_LOG", 21), ("FIXED_FUNCTION_RGB_TO_HSY_VID", 22)] },
    // py::enum_<ExposureContrastStyle> (PyTypes.cpp:630-640 @ v2.5.2), no fixed type.
    EnumDef { name: "ExposureContrastStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("EXPOSURE_CONTRAST_LINEAR", 0), ("EXPOSURE_CONTRAST_VIDEO", 1), ("EXPOSURE_CONTRAST_LOGARITHMIC", 2)] },
    // py::enum_<CDLStyle> (PyTypes.cpp:642-652 @ v2.5.2), no fixed type.
    EnumDef { name: "CDLStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("CDL_ASC", 0), ("CDL_NO_CLAMP", 1), ("CDL_TRANSFORM_DEFAULT", 1)] },
    // py::enum_<NegativeStyle> (PyTypes.cpp:654-666 @ v2.5.2), no fixed type.
    EnumDef { name: "NegativeStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("NEGATIVE_CLAMP", 0), ("NEGATIVE_MIRROR", 1), ("NEGATIVE_PASS_THRU", 2), ("NEGATIVE_LINEAR", 3)] },
    // py::enum_<GradingStyle> (PyTypes.cpp:668-678 @ v2.5.2), no fixed type.
    EnumDef { name: "GradingStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("GRADING_LOG", 0), ("GRADING_LIN", 1), ("GRADING_VIDEO", 2)] },
    // py::enum_<DynamicPropertyType> (PyTypes.cpp:680-698 @ v2.5.2), no fixed type.
    EnumDef { name: "DynamicPropertyType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("DYNAMIC_PROPERTY_EXPOSURE", 0), ("DYNAMIC_PROPERTY_CONTRAST", 1), ("DYNAMIC_PROPERTY_GAMMA", 2), ("DYNAMIC_PROPERTY_GRADING_PRIMARY", 3), ("DYNAMIC_PROPERTY_GRADING_RGBCURVE", 4), ("DYNAMIC_PROPERTY_GRADING_HUECURVE", 6), ("DYNAMIC_PROPERTY_GRADING_TONE", 5)] },
    // py::enum_<RGBCurveType> (PyTypes.cpp:700-714 @ v2.5.2), no fixed type.
    EnumDef { name: "RGBCurveType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("RGB_RED", 0), ("RGB_GREEN", 1), ("RGB_BLUE", 2), ("RGB_MASTER", 3), ("RGB_NUM_CURVES", 4)] },
    // py::enum_<HueCurveType> (PyTypes.cpp:716-736 @ v2.5.2), no fixed type.
    EnumDef { name: "HueCurveType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("HUE_HUE", 0), ("HUE_SAT", 1), ("HUE_LUM", 2), ("LUM_SAT", 3), ("SAT_SAT", 4), ("LUM_LUM", 5), ("SAT_LUM", 6), ("HUE_FX", 7)] },
    // py::enum_<HSYTransformStyle> (PyTypes.cpp:738-746 @ v2.5.2), no fixed type.
    EnumDef { name: "HSYTransformStyle", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("HSY_TRANSFORM_NONE", 0), ("HSY_TRANSFORM_1", 1)] },
    // py::enum_<BSplineType> (PyTypes.cpp:748-764 @ v2.5.2), no fixed type.
    EnumDef { name: "BSplineType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("B_SPLINE", 0), ("DIAGONAL_B_SPLINE", 1), ("HUE_HUE_B_SPLINE", 2), ("PERIODIC_1_B_SPLINE", 3), ("PERIODIC_0_B_SPLINE", 4), ("HORIZONTAL1_B_SPLINE", 5)] },
    // py::enum_<UniformDataType> (PyTypes.cpp:766-782 @ v2.5.2), no fixed type.
    EnumDef { name: "UniformDataType", scalar: Scalar::Unfixed, arithmetic: false, value_docs: true, entries: &[("UNIFORM_DOUBLE", 0), ("UNIFORM_BOOL", 1), ("UNIFORM_FLOAT3", 2), ("UNIFORM_VECTOR_FLOAT", 3), ("UNIFORM_VECTOR_INT", 4), ("UNIFORM_UNKNOWN", 5)] },
    // py::enum_<OptimizationFlags> (PyTypes.cpp:784-846 @ v2.5.2), underlying unsigned long.
    EnumDef { name: "OptimizationFlags", scalar: Scalar::ULong, arithmetic: true, value_docs: true, entries: &[("OPTIMIZATION_NONE", 0), ("OPTIMIZATION_IDENTITY", 1), ("OPTIMIZATION_IDENTITY_GAMMA", 2), ("OPTIMIZATION_PAIR_IDENTITY_CDL", 64), ("OPTIMIZATION_PAIR_IDENTITY_EXPOSURE_CONTRAST", 128), ("OPTIMIZATION_PAIR_IDENTITY_FIXED_FUNCTION", 256), ("OPTIMIZATION_PAIR_IDENTITY_GAMMA", 512), ("OPTIMIZATION_PAIR_IDENTITY_LUT1D", 1024), ("OPTIMIZATION_PAIR_IDENTITY_LUT3D", 2048), ("OPTIMIZATION_PAIR_IDENTITY_LOG", 4096), ("OPTIMIZATION_PAIR_IDENTITY_GRADING", 8192), ("OPTIMIZATION_COMP_EXPONENT", 262144), ("OPTIMIZATION_COMP_GAMMA", 524288), ("OPTIMIZATION_COMP_MATRIX", 1048576), ("OPTIMIZATION_COMP_LUT1D", 2097152), ("OPTIMIZATION_COMP_LUT3D", 4194304), ("OPTIMIZATION_COMP_RANGE", 8388608), ("OPTIMIZATION_COMP_SEPARABLE_PREFIX", 16777216), ("OPTIMIZATION_LUT_INV_FAST", 33554432), ("OPTIMIZATION_FAST_LOG_EXP_POW", 67108864), ("OPTIMIZATION_SIMPLIFY_OPS", 134217728), ("OPTIMIZATION_NO_DYNAMIC_PROPERTIES", 268435456), ("OPTIMIZATION_ALL", 4294967295), ("OPTIMIZATION_LOSSLESS", 144457667), ("OPTIMIZATION_VERY_GOOD", 263995331), ("OPTIMIZATION_GOOD", 268189635), ("OPTIMIZATION_DRAFT", 4294967295), ("OPTIMIZATION_DEFAULT", 263995331)] },
    // py::enum_<ProcessorCacheFlags> (PyTypes.cpp:848-860 @ v2.5.2), underlying unsigned int.
    EnumDef { name: "ProcessorCacheFlags", scalar: Scalar::UInt, arithmetic: false, value_docs: true, entries: &[("PROCESSOR_CACHE_OFF", 0), ("PROCESSOR_CACHE_ENABLED", 1), ("PROCESSOR_CACHE_SHARE_DYN_PROPERTIES", 2), ("PROCESSOR_CACHE_DEFAULT", 3)] },
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
    let cls = pybind11::new_class(py, name, name, &bases, &[])?;
    cls.setattr("_pybind11_conduit_v1_", conduit(py)?)?;
    m.setattr(name, &cls)?;
    Ok(cls)
}

/// Port of `bindPyTypes` (src/bindings/python/PyTypes.cpp:14-985 @ v2.5.2).
pub(crate) fn bind_py_types(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // The classes (17-290).
    for &(name, base) in CLASSES {
        declare_class(m, name, base)?;
    }
    // The enums (292-857).
    for def in ENUMS {
        add_enum(m.as_any(), None, def)?;
    }
    // The string-conversion functions (862-940).
    for f in &CONVERSIONS {
        pybind11::def_function(m, f)?;
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

// The string-conversion functions (862-940).
//
// Python can hold an enum value outside its enumerators (`OCIO.TransformDirection(5)`, I-223):
// each function then gives what upstream's gives for it (its last `return` or `throw`), which
// the Rust enums can't hold, so the value is matched to a variant here first.

/// The qualified name of the module's class `$name`, as pybind11 writes a parameter's type.
macro_rules! q {
    ($name:literal) => {
        concat!("PyOpenColorIO.PyOpenColorIO.", $name)
    };
}

/// A module function of one overload (`m.def(name, &f, "arg"_a...)`).
macro_rules! def {
    ($name:literal, [$($param:expr),*], $ret:expr, $call:expr) => {
        function(
            $name,
            &[Overload {
                params: &[$($param),*],
                ret: $ret,
                call: $call,
                pos_only: 0,
            }],
        )
    };
}

/// The value of the enum argument `$a[$i]` of the class `$class`, or the next overload.
macro_rules! enum_arg {
    ($a:expr, $i:expr, $class:expr) => {
        match enum_value(&$a[$i], $class)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// The variant of `$v` among `$variant`s (whose discriminants are upstream's values), if any.
macro_rules! variant {
    ($v:expr, $($variant:path),+ $(,)?) => {{
        let v: i64 = $v;
        [$($variant),+].into_iter().find(|e| *e as i64 == v)
    }};
}

type Out = PyResult<Option<Py<PyAny>>>;

fn text(py: Python<'_>, s: &str) -> Out {
    Ok(Some(PyString::new(py, s).into_any().unbind()))
}

fn boolean(py: Python<'_>, b: bool) -> Out {
    Ok(Some(PyBool::new(py, b).to_owned().into_any().unbind()))
}

/// A value of the enum class `class`.
fn enum_out(py: Python<'_>, class: &str, value: i64) -> Out {
    Ok(Some(to_py(&enum_class(py, class)?, value)?.unbind()))
}

/// `throw Exception(message)`.
fn raise(py: Python<'_>, message: &str) -> PyErr {
    to_py_err(py, &ocio::Exception::new(message))
}

/// A `const char *` argument, a null pointer as upstream's `s ? s : ""`.
fn c_string(s: &CharPtr) -> &[u8] {
    s.0.as_deref().unwrap_or_default()
}

const STR: Param = Param::new("str", "str");

const LOGGING_LEVEL: &str = q!("LoggingLevel");
const TRANSFORM_DIRECTION: &str = q!("TransformDirection");
const BIT_DEPTH: &str = q!("BitDepth");
const ALLOCATION: &str = q!("Allocation");
const INTERPOLATION: &str = q!("Interpolation");
const GPU_LANGUAGE: &str = q!("GpuLanguage");
const ENVIRONMENT_MODE: &str = q!("EnvironmentMode");
const CDL_STYLE: &str = q!("CDLStyle");
const RANGE_STYLE: &str = q!("RangeStyle");
const FIXED_FUNCTION_STYLE: &str = q!("FixedFunctionStyle");
const GRADING_STYLE: &str = q!("GradingStyle");
const EXPOSURE_CONTRAST_STYLE: &str = q!("ExposureContrastStyle");
const NEGATIVE_STYLE: &str = q!("NegativeStyle");

fn transform_direction(v: i64) -> Option<TransformDirection> {
    variant!(v, TransformDirection::Forward, TransformDirection::Inverse)
}

fn bit_depth(v: i64) -> Option<BitDepth> {
    use BitDepth::*;
    variant!(
        v, Unknown, Uint8, Uint10, Uint12, Uint14, Uint16, Uint32, F16, F32
    )
}

/// Port of `GradingStyleFromString` (src/OpenColorIO/ParseUtils.cpp:443-456 @ v2.5.2).
fn grading_style_from_string(style: &[u8]) -> ocio::Result<GradingStyle> {
    let p = c_str(style);
    match lower(p).as_slice() {
        b"linear" => Ok(GradingStyle::Lin),
        b"log" => Ok(GradingStyle::Log),
        b"video" => Ok(GradingStyle::Video),
        _ => Err(ocio::Exception::new(
            [b"Unknown grading style: '".as_slice(), p, b"'."].concat(),
        )),
    }
}

/// Port of `ExposureContrastStyleToString` (src/OpenColorIO/ParseUtils.cpp:465-476 @ v2.5.2),
/// on the enum's value (`EXPOSURE_CONTRAST_LINEAR` 0, `_VIDEO` 1, `_LOGARITHMIC` 2,
/// include/OpenColorIO/OpenColorTypes.h): the library has no Rust enum for it yet.
fn exposure_contrast_style_to_string(style: i64) -> ocio::Result<&'static str> {
    match style {
        1 => Ok("video"),
        2 => Ok("log"),
        0 => Ok("linear"),
        _ => Err(ocio::Exception::new("Unknown exposure contrast style")),
    }
}

/// Port of `ExposureContrastStyleFromString` (src/OpenColorIO/ParseUtils.cpp:478-491 @
/// v2.5.2), as [`exposure_contrast_style_to_string`]'s value.
fn exposure_contrast_style_from_string(style: &[u8]) -> ocio::Result<i64> {
    let p = c_str(style);
    match lower(p).as_slice() {
        b"linear" => Ok(0),
        b"video" => Ok(1),
        b"log" => Ok(2),
        _ => Err(ocio::Exception::new(
            [b"Unknown exposure contrast style: '".as_slice(), p, b"'."].concat(),
        )),
    }
}

/// The functions `bindPyTypes` defines (PyTypes.cpp:862-940 @ v2.5.2), in its order; each
/// calls the library's (ParseUtils.cpp @ v2.5.2, whose lines the comments give for a value
/// outside the enum).
static CONVERSIONS: [Function; 32] = [
    def!(
        "BoolToString",
        [Param::new("value", "bool")],
        "str",
        |py, _, a, convert| {
            let value = arg!(a, 0, convert, bool);
            text(py, bool_to_string(value))
        }
    ),
    def!("BoolFromString", [STR], "bool", |py, _, a, convert| {
        let s = arg!(a, 0, convert, CharPtr);
        boolean(py, bool_from_string(s.0.as_deref()))
    }),
    def!(
        "LoggingLevelToString",
        [Param::new("level", LOGGING_LEVEL)],
        "str",
        |py, _, a, _| {
            let level = enum_arg!(a, 0, LOGGING_LEVEL);
            let level = variant!(
                level,
                LoggingLevel::None,
                LoggingLevel::Warning,
                LoggingLevel::Info,
                LoggingLevel::Debug,
            )
            // ParseUtils.cpp:119.
            .unwrap_or(LoggingLevel::Unknown);
            text(py, logging_level_to_string(level))
        }
    ),
    def!(
        "LoggingLevelFromString",
        [STR],
        LOGGING_LEVEL,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            enum_out(
                py,
                LOGGING_LEVEL,
                logging_level_from_string(s.0.as_deref()) as i64,
            )
        }
    ),
    def!(
        "TransformDirectionToString",
        [Param::new("direction", TRANSFORM_DIRECTION)],
        "str",
        |py, _, a, _| {
            let dir = enum_arg!(a, 0, TRANSFORM_DIRECTION);
            // ParseUtils.cpp:136-137.
            let dir = transform_direction(dir).unwrap_or(TransformDirection::Inverse);
            text(py, transform_direction_to_string(dir))
        }
    ),
    def!(
        "TransformDirectionFromString",
        [STR],
        TRANSFORM_DIRECTION,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let dir = py_result(py, transform_direction_from_string(s.0.as_deref()))?;
            enum_out(py, TRANSFORM_DIRECTION, dir as i64)
        }
    ),
    def!(
        "GetInverseTransformDirection",
        [Param::new("direction", TRANSFORM_DIRECTION)],
        TRANSFORM_DIRECTION,
        |py, _, a, _| {
            let dir = enum_arg!(a, 0, TRANSFORM_DIRECTION);
            // ParseUtils.cpp:166-167.
            let dir = transform_direction(dir).unwrap_or(TransformDirection::Inverse);
            enum_out(
                py,
                TRANSFORM_DIRECTION,
                get_inverse_transform_direction(dir) as i64,
            )
        }
    ),
    def!(
        "CombineTransformDirections",
        [
            Param::new("direction1", TRANSFORM_DIRECTION),
            Param::new("direction2", TRANSFORM_DIRECTION)
        ],
        TRANSFORM_DIRECTION,
        |py, _, a, _| {
            let d1 = enum_arg!(a, 0, TRANSFORM_DIRECTION);
            let d2 = enum_arg!(a, 1, TRANSFORM_DIRECTION);
            let combined = match (transform_direction(d1), transform_direction(d2)) {
                (Some(d1), Some(d2)) => combine_transform_directions(d1, d2),
                // ParseUtils.cpp:161.
                _ => TransformDirection::Inverse,
            };
            enum_out(py, TRANSFORM_DIRECTION, combined as i64)
        }
    ),
    def!(
        "BitDepthToString",
        [Param::new("bitDepth", BIT_DEPTH)],
        "str",
        |py, _, a, _| {
            let bd = enum_arg!(a, 0, BIT_DEPTH);
            // ParseUtils.cpp:181.
            text(py, bit_depth(bd).map_or("unknown", bit_depth_to_string))
        }
    ),
    def!(
        "BitDepthFromString",
        [STR],
        BIT_DEPTH,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            enum_out(py, BIT_DEPTH, bit_depth_from_string(s.0.as_deref()) as i64)
        }
    ),
    def!(
        "BitDepthIsFloat",
        [Param::new("bitDepth", BIT_DEPTH)],
        "bool",
        |py, _, a, _| {
            let bd = enum_arg!(a, 0, BIT_DEPTH);
            // ParseUtils.cpp:203.
            boolean(py, bit_depth(bd).is_some_and(bit_depth_is_float))
        }
    ),
    def!(
        "BitDepthToInt",
        [Param::new("bitDepth", BIT_DEPTH)],
        "int",
        |py, _, a, _| {
            let bd = enum_arg!(a, 0, BIT_DEPTH);
            // ParseUtils.cpp:215.
            let bits = bit_depth(bd).map_or(0, bit_depth_to_int);
            Ok(Some(PyInt::new(py, bits).into_any().unbind()))
        }
    ),
    def!(
        "AllocationToString",
        [Param::new("allocation", ALLOCATION)],
        "str",
        |py, _, a, _| {
            use Allocation::*;
            let alloc = enum_arg!(a, 0, ALLOCATION);
            // ParseUtils.cpp:222.
            let alloc = variant!(alloc, Unknown, Uniform, Lg2).unwrap_or(Unknown);
            text(py, allocation_to_string(alloc))
        }
    ),
    def!(
        "AllocationFromString",
        [STR],
        ALLOCATION,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            enum_out(
                py,
                ALLOCATION,
                allocation_from_string(s.0.as_deref()) as i64,
            )
        }
    ),
    def!(
        "InterpolationToString",
        [Param::new("interpolation", INTERPOLATION)],
        "str",
        |py, _, a, _| {
            use Interpolation::*;
            let interp = enum_arg!(a, 0, INTERPOLATION);
            // ParseUtils.cpp:243.
            let interp = variant!(
                interp,
                Unknown,
                Nearest,
                Linear,
                Tetrahedral,
                Cubic,
                Default,
                Best
            )
            .unwrap_or(Unknown);
            text(py, interpolation_to_string(interp))
        }
    ),
    def!(
        "InterpolationFromString",
        [STR],
        INTERPOLATION,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            enum_out(
                py,
                INTERPOLATION,
                interpolation_from_string(s.0.as_deref()) as i64,
            )
        }
    ),
    def!(
        "GpuLanguageToString",
        [Param::new("language", GPU_LANGUAGE)],
        "str",
        |py, _, a, _| {
            let language = enum_arg!(a, 0, GPU_LANGUAGE);
            match GpuLanguage::ALL.into_iter().find(|l| *l as i64 == language) {
                Some(language) => text(py, gpu_language_to_string(language)),
                // ParseUtils.cpp:274.
                None => Err(raise(py, "Unsupported GPU shader language.")),
            }
        }
    ),
    def!(
        "GpuLanguageFromString",
        [STR],
        GPU_LANGUAGE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let language = py_result(py, gpu_language_from_string(s.0.as_deref()))?;
            enum_out(py, GPU_LANGUAGE, language as i64)
        }
    ),
    def!(
        "EnvironmentModeToString",
        [Param::new("mode", ENVIRONMENT_MODE)],
        "str",
        |py, _, a, _| {
            use EnvironmentMode::*;
            let mode = enum_arg!(a, 0, ENVIRONMENT_MODE);
            // ParseUtils.cpp:302.
            let mode = variant!(mode, Unknown, LoadPredefined, LoadAll).unwrap_or(Unknown);
            text(py, environment_mode_to_string(mode))
        }
    ),
    def!(
        "EnvironmentModeFromString",
        [STR],
        ENVIRONMENT_MODE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            enum_out(
                py,
                ENVIRONMENT_MODE,
                environment_mode_from_string(s.0.as_deref()) as i64,
            )
        }
    ),
    def!(
        "CDLStyleToString",
        [Param::new("style", CDL_STYLE)],
        "str",
        |py, _, a, _| {
            let style = enum_arg!(a, 0, CDL_STYLE);
            // ParseUtils.cpp:318.
            let style = variant!(style, CdlStyle::Asc, CdlStyle::NoClamp).unwrap_or(CdlStyle::Asc);
            text(py, cdl_style_to_string(style))
        }
    ),
    def!(
        "CDLStyleFromString",
        [STR],
        CDL_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, cdl_style_from_string(s.0.as_deref()))?;
            enum_out(py, CDL_STYLE, style as i64)
        }
    ),
    def!(
        "RangeStyleToString",
        [Param::new("style", RANGE_STYLE)],
        "str",
        |py, _, a, _| {
            let style = enum_arg!(a, 0, RANGE_STYLE);
            // ParseUtils.cpp:338.
            let style = variant!(style, RangeStyle::NoClamp, RangeStyle::Clamp)
                .unwrap_or(RangeStyle::Clamp);
            text(py, range_style_to_string(style))
        }
    ),
    def!(
        "RangeStyleFromString",
        [STR],
        RANGE_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, range_style_from_string(s.0.as_deref()))?;
            enum_out(py, RANGE_STYLE, style as i64)
        }
    ),
    def!(
        "FixedFunctionStyleToString",
        [Param::new("style", FIXED_FUNCTION_STYLE)],
        "str",
        |py, _, a, _| {
            use FixedFunctionStyle::*;
            let style = enum_arg!(a, 0, FIXED_FUNCTION_STYLE);
            let style = variant!(
                style,
                AcesRedMod03,
                AcesRedMod10,
                AcesGlow03,
                AcesGlow10,
                AcesDarkToDim10,
                Rec2100Surround,
                RgbToHsv,
                XyzToXyy,
                XyzToUvy,
                XyzToLuv,
                AcesGamutMap02,
                AcesGamutMap07,
                AcesGamutComp13,
                LinToPq,
                LinToGammaLog,
                LinToDoubleLog,
                AcesOutputTransform20,
                AcesRgbToJmh20,
                AcesTonescaleCompress20,
                AcesGamutCompress20,
                RgbToHsyLin,
                RgbToHsyLog,
                RgbToHsyVid,
            );
            match style {
                Some(style) => text(py, py_result(py, fixed_function_style_to_string(style))?),
                // ParseUtils.cpp:386-387.
                None => Err(raise(py, "Unknown Fixed FunctionOp style")),
            }
        }
    ),
    def!(
        "FixedFunctionStyleFromString",
        [STR],
        FIXED_FUNCTION_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, fixed_function_style_from_string(s.0.as_deref()))?;
            enum_out(py, FIXED_FUNCTION_STYLE, style as i64)
        }
    ),
    def!(
        "GradingStyleToString",
        [Param::new("style", GRADING_STYLE)],
        "str",
        |py, _, a, _| {
            use GradingStyle::*;
            let style = enum_arg!(a, 0, GRADING_STYLE);
            match variant!(style, Log, Lin, Video) {
                Some(style) => text(py, grading_style_to_string(style)),
                // ParseUtils.cpp:439-440.
                None => Err(raise(py, "Unknown grading style")),
            }
        }
    ),
    def!(
        "GradingStyleFromString",
        [STR],
        GRADING_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, grading_style_from_string(c_string(&s)))?;
            enum_out(py, GRADING_STYLE, style as i64)
        }
    ),
    def!(
        "ExposureContrastStyleToString",
        [Param::new("style", EXPOSURE_CONTRAST_STYLE)],
        "str",
        |py, _, a, _| {
            let style = enum_arg!(a, 0, EXPOSURE_CONTRAST_STYLE);
            text(py, py_result(py, exposure_contrast_style_to_string(style))?)
        }
    ),
    def!(
        "ExposureContrastStyleFromString",
        [STR],
        EXPOSURE_CONTRAST_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, exposure_contrast_style_from_string(c_string(&s)))?;
            enum_out(py, EXPOSURE_CONTRAST_STYLE, style)
        }
    ),
    def!(
        "NegativeStyleToString",
        [Param::new("style", NEGATIVE_STYLE)],
        "str",
        |py, _, a, _| {
            use NegativeStyle::*;
            let style = enum_arg!(a, 0, NEGATIVE_STYLE);
            match variant!(style, Clamp, Mirror, PassThru, Linear) {
                Some(style) => text(py, negative_style_to_string(style)),
                // ParseUtils.cpp:512.
                None => Err(raise(py, "Unknown exponent style")),
            }
        }
    ),
    def!(
        "NegativeStyleFromString",
        [STR],
        NEGATIVE_STYLE,
        |py, _, a, convert| {
            let s = arg!(a, 0, convert, CharPtr);
            let style = py_result(py, negative_style_from_string(s.0.as_deref()))?;
            enum_out(py, NEGATIVE_STYLE, style as i64)
        }
    ),
];
