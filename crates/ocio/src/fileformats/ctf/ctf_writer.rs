// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF writer: the ProcessList and its operators as XML.
//!
//! Port of the writers of `src/OpenColorIO/fileformats/ctf/CTFTransform.cpp` and
//! `CTFTransform.h` (@ v2.5.2): the lowest version that fits the ops (`GetOpMinimumVersion`,
//! `GetMinimumVersion`), the value formatting (`WriteValue`, `SetOStream`, `WriteValues`),
//! `OpWriter` and its bit depths, the operators' writers, and `TransformWriter`. Upstream's
//! writers hold their formatter; here each `write` is given it. The operators' writers came
//! chunk by chunk: the Matrix (WP 4.6a); the CDL, Gamma (and Exponent), Log and Range (WP
//! 4.6b); the LUTs (WP 4.6c); the FixedFunction and RGB curves (WP 4.6d). The ExposureContrast
//! and other grading writers (WP 4.6f) come with their op data (Phase 5).

use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    METADATA_SAT_DESCRIPTION, METADATA_SOP_DESCRIPTION,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    ATTR_ID, ATTR_NAME, TAG_DESCRIPTION, TAG_OFFSET, TAG_POWER, TAG_SATNODE, TAG_SATURATION,
    TAG_SLOPE, TAG_SOPNODE,
};
use ocio_formats::fileformats::xmlutils::xml_writer_utils::{
    Attributes, XmlElementWriter, XmlFormatter, XmlScopeIndent,
};
use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::{
    FormatMetadataImpl, METADATA_DESCRIPTION, METADATA_INPUT_DESCRIPTOR, METADATA_OUTPUT_DESCRIPTOR,
};
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::imath_half::float_to_half;
use ocio_ops::op_data::{OpData, OpDataRcPtr, OpDataType, get_type_name};
use ocio_ops::open_color_types::{
    BitDepth, GradingStyle, Lut1DHueAdjust, RgbCurveType, TransformDirection,
};
use ocio_ops::ops::cdl::cdl_op_data::CdlOpData;
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::ExposureContrastOpData;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpStyle;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::GradingBSplineCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::{default_curve, default_lin_curve};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::ops::log::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LINEAR_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
};
use ocio_ops::ops::log::log_utils::{
    ANTI_LOG2_STR, ANTI_LOG10_STR, CAMERA_LIN_TO_LOG_STR, CAMERA_LOG_TO_LIN_STR, LIN_TO_LOG_STR,
    LOG_TO_LIN_STR, LOG2_STR, LOG10_STR,
};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut1DOpData;
use ocio_ops::ops::lut3d::lut3d_op_data::Lut3DOpData;
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::{
    ATTR_BASE, ATTR_BITDEPTH_IN, ATTR_BITDEPTH_OUT, ATTR_BYPASS_LIN_TO_LOG, ATTR_CHAN,
    ATTR_COMP_CLF_VERSION, ATTR_DIMENSION, ATTR_EXPONENT, ATTR_GAMMA, ATTR_HALF_DOMAIN,
    ATTR_HUE_ADJUST, ATTR_INTERPOLATION, ATTR_INVERSE_OF, ATTR_LINEARSLOPE, ATTR_LINSIDEBREAK,
    ATTR_LINSIDEOFFSET, ATTR_LINSIDESLOPE, ATTR_LOGSIDEOFFSET, ATTR_LOGSIDESLOPE, ATTR_OFFSET,
    ATTR_PARAM, ATTR_PARAMS, ATTR_RAW_HALFS, ATTR_STYLE, ATTR_VERSION, TAG_ARRAY, TAG_CDL,
    TAG_CURVE_CTRL_PNTS, TAG_CURVE_SLOPES, TAG_DYN_PROP_RGBCURVE, TAG_DYNAMIC_PARAMETER,
    TAG_EXPONENT, TAG_EXPONENT_PARAMS, TAG_FIXED_FUNCTION, TAG_GAMMA, TAG_GAMMA_PARAMS,
    TAG_INVLUT1D, TAG_INVLUT3D, TAG_LOG, TAG_LOG_PARAMS, TAG_LUT1D, TAG_LUT3D, TAG_MATRIX,
    TAG_MAX_IN_VALUE, TAG_MAX_OUT_VALUE, TAG_MIN_IN_VALUE, TAG_MIN_OUT_VALUE, TAG_PROCESS_LIST,
    TAG_RANGE, TAG_RGB_CURVE, TAG_RGB_CURVE_BLUE, TAG_RGB_CURVE_GREEN, TAG_RGB_CURVE_MASTER,
    TAG_RGB_CURVE_RED, convert_grading_style_and_dir_to_string, get_interpolation_1d_name,
    get_interpolation_3d_name,
};
use crate::transforms::cdl_transform::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};

use super::ctf_transform::{
    CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_4, CTF_PROCESS_LIST_VERSION_1_5,
    CTF_PROCESS_LIST_VERSION_1_6, CTF_PROCESS_LIST_VERSION_1_7, CTF_PROCESS_LIST_VERSION_2_0,
    CTF_PROCESS_LIST_VERSION_2_1, CTF_PROCESS_LIST_VERSION_2_4, CTF_PROCESS_LIST_VERSION_2_5,
    CtfReaderTransform, CtfVersion, get_elements_values,
};

/// The precision of the doubles the writer formats (CTFTransform.cpp:39 @ v2.5.2).
const DOUBLE_PRECISION: i64 = 15;

/// The lowest CTF version that can hold `op`.
///
/// Port of `GetOpMinimumVersion` (CTFTransform.cpp:225-384 @ v2.5.2). Upstream's default,
/// `CTF_PROCESS_LIST_VERSION`, is for none of the types the port has.
fn get_op_minimum_version(op: &OpData) -> Result<CtfVersion> {
    use FixedFunctionOpStyle as F;

    let min_version = match op {
        OpData::Cdl(_) => CTF_PROCESS_LIST_VERSION_1_7,
        // CTFTransform.cpp:237-248.
        OpData::ExposureContrast(ec) => {
            if ec.get_log_exposure_step() != ExposureContrastOpData::LOGEXPOSURESTEP_DEFAULT
                || ec.get_log_mid_gray() != ExposureContrastOpData::LOGMIDGRAY_DEFAULT
            {
                CTF_PROCESS_LIST_VERSION_2_0
            } else {
                CTF_PROCESS_LIST_VERSION_1_3
            }
        }
        OpData::FixedFunction(ff) => match ff.style() {
            F::AcesGamutComp13Fwd | F::AcesGamutComp13Inv => CTF_PROCESS_LIST_VERSION_2_1,
            F::LinToPq
            | F::PqToLin
            | F::LinToGammaLog
            | F::GammaLogToLin
            | F::LinToDoubleLog
            | F::DoubleLogToLin
            | F::AcesOutputTransform20Fwd
            | F::AcesOutputTransform20Inv
            | F::AcesRgbToJmh20
            | F::AcesJmhToRgb20
            | F::AcesTonescaleCompress20Fwd
            | F::AcesTonescaleCompress20Inv
            | F::AcesGamutCompress20Fwd
            | F::AcesGamutCompress20Inv => CTF_PROCESS_LIST_VERSION_2_4,
            F::RgbToHsyLog
            | F::HsyLogToRgb
            | F::RgbToHsyLin
            | F::HsyLinToRgb
            | F::RgbToHsyVid
            | F::HsyVidToRgb => CTF_PROCESS_LIST_VERSION_2_5,
            _ => CTF_PROCESS_LIST_VERSION_2_0,
        },
        OpData::GradingPrimary(_)
        | OpData::GradingRgbCurve(_)
        | OpData::GradingTone(_)
        | OpData::Log(_) => CTF_PROCESS_LIST_VERSION_2_0,
        // CTFTransform.cpp:296-300.
        OpData::GradingHueCurve(_) => CTF_PROCESS_LIST_VERSION_2_5,
        OpData::Exponent(exp) => {
            if exp.exp4[3] == 1. {
                CTF_PROCESS_LIST_VERSION_1_3
            } else {
                CTF_PROCESS_LIST_VERSION_1_5
            }
        }
        OpData::Gamma(gamma) => match gamma.style() {
            GammaStyle::BasicFwd
            | GammaStyle::BasicRev
            | GammaStyle::MoncurveFwd
            | GammaStyle::MoncurveRev => {
                if gamma.is_alpha_component_identity() {
                    CTF_PROCESS_LIST_VERSION_1_3
                } else {
                    CTF_PROCESS_LIST_VERSION_1_5
                }
            }
            GammaStyle::BasicMirrorFwd
            | GammaStyle::BasicMirrorRev
            | GammaStyle::BasicPassThruFwd
            | GammaStyle::BasicPassThruRev
            | GammaStyle::MoncurveMirrorFwd
            | GammaStyle::MoncurveMirrorRev => CTF_PROCESS_LIST_VERSION_2_0,
        },
        OpData::Lut1D(lut) => match lut.get_direction() {
            TransformDirection::Forward => {
                if lut.get_hue_adjust() != Lut1DHueAdjust::None {
                    CTF_PROCESS_LIST_VERSION_1_4
                } else {
                    CTF_PROCESS_LIST_VERSION_1_3
                }
            }
            TransformDirection::Inverse => {
                if lut.get_hue_adjust() != Lut1DHueAdjust::None || lut.is_input_half_domain() {
                    CTF_PROCESS_LIST_VERSION_1_6
                } else {
                    CTF_PROCESS_LIST_VERSION_1_3
                }
            }
        },
        OpData::Lut3D(lut) => match lut.get_direction() {
            TransformDirection::Forward => CTF_PROCESS_LIST_VERSION_1_3,
            TransformDirection::Inverse => CTF_PROCESS_LIST_VERSION_1_6,
        },
        OpData::Matrix(_) | OpData::Range(_) => CTF_PROCESS_LIST_VERSION_1_3,
        OpData::Reference(_) => {
            return Err(Exception::new(
                "Reference ops should have been replaced by their content.",
            ));
        }
        OpData::NoOp(_) => CTF_PROCESS_LIST_VERSION_1_3,
    };
    Ok(min_version)
}

/// The lowest CTF version that can hold every op of `transform`, 1.3 at least.
///
/// Port of `GetMinimumVersion` (CTFTransform.cpp:386-403 @ v2.5.2).
fn get_minimum_version(transform: &CtfReaderTransform) -> Result<CtfVersion> {
    let op_list = transform.get_ops();

    // Need to specify the minimum version here. Some test transforms have no ops.
    let mut minimum_version = CTF_PROCESS_LIST_VERSION_1_3;

    for op in op_list {
        let version = get_op_minimum_version(op)?;
        if version > minimum_version {
            minimum_version = version;
        }
    }

    Ok(minimum_version)
}

/// Writes each description as a `tag` element.
///
/// Port of `WriteDescriptions` (CTFTransform.cpp:511-517 @ v2.5.2).
fn write_descriptions(fmt: &mut XmlFormatter<'_>, tag: &[u8], descriptions: &[Vec<u8>]) {
    for it in descriptions {
        fmt.write_content_tag(tag, it);
    }
}

/// A floating-point value the writer formats: `float` or `double`.
pub(crate) trait WriteNum: Copy {
    /// Whether it is a `double` (`SetOStream`'s specialization).
    const IS_DOUBLE: bool;
    /// `value * scale` in the value's type.
    fn scaled(self, scale: Self) -> Self;
    /// `os << value`.
    fn put(self, os: &mut OStringStream);
    /// `IsNan(value)`.
    fn is_nan_value(self) -> bool;
    /// `value == std::numeric_limits<T>::infinity()`.
    fn is_pos_inf(self) -> bool;
    /// `value == -std::numeric_limits<T>::infinity()`.
    fn is_neg_inf(self) -> bool;
}

impl WriteNum for f32 {
    const IS_DOUBLE: bool = false;
    fn scaled(self, scale: f32) -> f32 {
        self * scale
    }
    fn put(self, os: &mut OStringStream) {
        os.put_f32(self);
    }
    fn is_nan_value(self) -> bool {
        self.is_nan()
    }
    fn is_pos_inf(self) -> bool {
        self == f32::INFINITY
    }
    fn is_neg_inf(self) -> bool {
        self == f32::NEG_INFINITY
    }
}

impl WriteNum for f64 {
    const IS_DOUBLE: bool = true;
    fn scaled(self, scale: f64) -> f64 {
        self * scale
    }
    fn put(self, os: &mut OStringStream) {
        os.put_f64(self);
    }
    fn is_nan_value(self) -> bool {
        self.is_nan()
    }
    fn is_pos_inf(self) -> bool {
        self == f64::INFINITY
    }
    fn is_neg_inf(self) -> bool {
        self == f64::NEG_INFINITY
    }
}

/// Writes a floating-point value: `nan`, `inf` and `-inf` for the special values.
///
/// Port of `WriteValue` for floating-point types (CTFTransform.cpp:519-539 @ v2.5.2).
fn write_value<T: WriteNum>(value: T, stream: &mut OStringStream) {
    if value.is_nan_value() {
        stream.put_str("nan");
    } else if value.is_pos_inf() {
        stream.put_str("inf");
    } else if value.is_neg_inf() {
        stream.put_str("-inf");
    } else {
        value.put(stream);
    }
}

/// The width and precision of the values of an `F32` array: 11 and 8 for a `float`, 19 and
/// [`DOUBLE_PRECISION`] for a `double`.
///
/// Port of `SetOStream` (CTFTransform.cpp:547-559 @ v2.5.2).
fn set_ostream<T: WriteNum>(xml: &mut OStringStream) {
    if T::IS_DOUBLE {
        xml.width = 19;
        xml.precision = DOUBLE_PRECISION;
    } else {
        xml.width = 11;
        xml.precision = 8;
    }
}

/// Writes the values `values[0]`, `values[iter_step]`, ... times `scale`, `values_per_line` a
/// line (counting every value, stepped over or not), formatted for `bit_depth`: each padded
/// to the width of the value before it, the first to the bit depth's width.
///
/// Port of `WriteValues` (CTFTransform.cpp:561-673 @ v2.5.2). Upstream's iterator range is a
/// slice. An `std::ostream`'s width resets after every value it writes, so `value.length() >
/// oss.width()` always holds, and the width becomes the length of the value just written.
pub(crate) fn write_values<T: WriteNum>(
    formatter: &mut XmlFormatter<'_>,
    values: &[T],
    values_per_line: usize,
    bit_depth: BitDepth,
    iter_step: usize,
    scale: T,
) -> Result<()> {
    // Method used to write an array of values of the same type.

    let mut oss = OStringStream::new(Crt::NATIVE);

    // The numbers in a CLF/CTF file may always contain fractional values, regardless of the
    // bit-depth attributes. E.g., even if the bit-depth is 8i, the array could contain values
    // such as [-0.1, 8, 100.234, 305]. However, we do use the bit-depth to initialize the most
    // likely formatting to use when printing arrays with large numbers of values such as LUTs.
    // And if the array really does only have integers, it is nicer to print those without
    // decimal points.

    match bit_depth {
        BitDepth::Uint8 => oss.width = 3,
        BitDepth::Uint10 => oss.width = 4,
        BitDepth::Uint12 => oss.width = 4,
        BitDepth::Uint16 => oss.width = 5,
        BitDepth::F16 => {
            oss.width = 11;
            oss.precision = 5;
        }
        BitDepth::F32 => set_ostream::<T>(&mut oss),
        BitDepth::Uint14 | BitDepth::Uint32 => {
            return Err(Exception::new("Unsupported bitdepth."));
        }
        BitDepth::Unknown => return Err(Exception::new("Unknown bitdepth.")),
    }

    let float_values = bit_depth == BitDepth::F16 || bit_depth == BitDepth::F32;

    let mut index = 0;
    while index < values.len() {
        oss.clear_str();

        let value = values[index].scaled(scale);
        if float_values {
            write_value(value, &mut oss);
        } else {
            value.put(&mut oss);
        }

        let value = oss.str().to_vec();
        if value.len() as i64 > oss.width {
            // The imposed precision requires more characters so the code recomputes the width
            // to better align the values for the next lines.
            oss.width = value.len() as i64;
        }

        let xml = formatter.get_stream();
        xml.put_bytes(&value);

        if index % values_per_line == values_per_line - 1 {
            // std::endln writes a newline and flushes the output buffer where '\n' only
            // writes a newline. Flushing the buffer for all floats of large LUTs introduces a
            // huge performance hit so only use '\n'.
            xml.put_str("\n");
        } else {
            xml.put_str(" ");
        }
        index += iter_step;
    }
    Ok(())
}

/// The CLF name of a bit depth the writer validated.
///
/// Port of `BitDepthToCLFString` (CTFTransform.cpp:743-753 @ v2.5.2).
fn bit_depth_to_clf_string(bit_depth: BitDepth) -> Result<&'static [u8]> {
    Ok(match bit_depth {
        BitDepth::Uint8 => b"8i",
        BitDepth::Uint10 => b"10i",
        BitDepth::Uint12 => b"12i",
        BitDepth::Uint16 => b"16i",
        BitDepth::F16 => b"16f",
        BitDepth::F32 => b"32f",
        _ => {
            return Err(Exception::new(
                "Bitdepth has not been validated before calling this.",
            ));
        }
    })
}

/// The file bit depth to write for an op of `op_type` that prefers `bd`: `F32` for none, an
/// error for the depths CLF/CTF don't have.
///
/// Port of `GetValidatedFileBitDepth` (CTFTransform.cpp:755-779 @ v2.5.2). The message
/// streams the bit depth as its enum's integer.
pub(crate) fn get_validated_file_bit_depth(bd: BitDepth, op_type: OpDataType) -> Result<BitDepth> {
    // If we get BIT_DEPTH_UNKNOWN here, it means the client has not requested any specific
    // bit-depth to write to the file, so we will use a default of F32.
    if bd == BitDepth::Unknown {
        return Ok(BitDepth::F32);
    }
    if matches!(
        bd,
        BitDepth::Uint8
            | BitDepth::Uint10
            | BitDepth::Uint12
            | BitDepth::Uint16
            | BitDepth::F16
            | BitDepth::F32
    ) {
        return Ok(bd);
    }

    let type_name = get_type_name(op_type)?;
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.put_str("Op ");
    oss.put_str(type_name);
    oss.put_str(". Bit-depth: ");
    oss.put_i32(bd as i32);
    oss.put_str(" is not supported for writing to CLF/CTF.");
    Err(Exception::new(oss.into_bytes()))
}

/// The op's id and name when not empty, and its bit depths: `OpWriter::getAttributes`,
/// which the writers that override it call first.
///
/// Port of `OpWriter::getAttributes` (CTFTransform.cpp:781-811 @ v2.5.2).
fn op_writer_attributes<W: OpWriter + ?Sized>(
    writer: &W,
    attributes: &mut Attributes,
) -> Result<()> {
    let op = writer.get_op();
    let id = op.get_id();
    if !id.is_empty() {
        attributes.push((ATTR_ID.to_vec(), id.to_vec()));
    }

    let name = op.get_name();
    if !name.is_empty() {
        attributes.push((ATTR_NAME.to_vec(), name.to_vec()));
    }

    let in_bit_depth_name = bit_depth_to_clf_string(writer.in_bit_depth())?;
    attributes.push((ATTR_BITDEPTH_IN.to_vec(), in_bit_depth_name.to_vec()));

    let out_bit_depth_name = bit_depth_to_clf_string(writer.out_bit_depth())?;
    attributes.push((ATTR_BITDEPTH_OUT.to_vec(), out_bit_depth_name.to_vec()));

    // TODO: bypass implementation
    Ok(())
}

/// The writer of one operator: its element, with the op's id, name and bit depths, its
/// descriptions, then its content.
///
/// Port of `OpWriter` (CTFTransform.cpp:675-811 @ v2.5.2). Each operator's writer implements
/// the virtual functions; [`write`](Self::write) is upstream's `OpWriter::write`.
trait OpWriter {
    /// `getOp`.
    fn get_op(&self) -> &OpData;
    /// `getTagName`.
    fn get_tag_name(&self) -> &[u8];
    /// `m_inBitDepth`.
    fn in_bit_depth(&self) -> BitDepth;
    /// `m_outBitDepth`.
    fn out_bit_depth(&self) -> BitDepth;
    /// `writeContent`.
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()>;

    /// The op's id and name when not empty, and its bit depths.
    ///
    /// Port of `OpWriter::getAttributes` (CTFTransform.cpp:781-811 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)
    }

    /// The op's descriptions.
    ///
    /// Port of `OpWriter::writeFormatMetadata` (CTFTransform.cpp:734-741 @ v2.5.2).
    fn write_format_metadata(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let op = self.get_op();
        let mut desc = Vec::new();
        get_elements_values(
            op.get_format_metadata().get_children_elements(),
            TAG_DESCRIPTION,
            &mut desc,
        );
        write_descriptions(formatter, TAG_DESCRIPTION, &desc);
        Ok(())
    }

    /// Port of `OpWriter::write` (CTFTransform.cpp:710-732 @ v2.5.2).
    fn write(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let mut attributes = Attributes::new();
        self.get_attributes(&mut attributes)?;

        let tag_name = self.get_tag_name().to_vec();
        formatter.write_start_tag_with(&tag_name, &attributes);
        {
            let mut scope_indent = XmlScopeIndent::new(formatter);
            self.write_format_metadata(&mut scope_indent)?;

            // TODO: Bypass Dynamic Property converts to Look Dynamic Property in ctf format.

            self.write_content(&mut scope_indent)?;
        }
        formatter.write_end_tag(&tag_name);
        Ok(())
    }
}

/// `os << value` of a `double` at [`DOUBLE_PRECISION`], in a new stream.
fn double_string(value: f64) -> Vec<u8> {
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.precision = DOUBLE_PRECISION;
    oss.put_f64(value);
    oss.into_bytes()
}

/// The writer of an ASC_CDL: its style, its descriptions of each kind, and its SOPNode and
/// SatNode.
///
/// Port of `CDLWriter` (CTFTransform.cpp:813-933 @ v2.5.2).
struct CdlWriter<'a> {
    /// `m_cdl`.
    cdl: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl CdlWriter<'_> {
    fn cdl(&self) -> &CdlOpData {
        let OpData::Cdl(cdl) = self.cdl else {
            unreachable!("a CDL writer writes a CDL");
        };
        cdl
    }
}

impl OpWriter for CdlWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.cdl
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_CDL
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `CDLWriter::getAttributes` (CTFTransform.cpp:856-861 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;
        let style = CdlOpData::get_style_name(self.cdl().get_style());
        attributes.push((ATTR_STYLE.to_vec(), style.as_bytes().to_vec()));
        Ok(())
    }

    /// Port of `CDLWriter::writeContent` (CTFTransform.cpp:863-916 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let attributes = Attributes::new();

        let op = self.get_op();
        let cdl = self.cdl();

        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.precision = DOUBLE_PRECISION;

        let triple = |oss: &mut OStringStream, params: [f64; 3]| {
            oss.clear_str();
            oss.put_f64(params[0]);
            oss.put_str(" ");
            oss.put_f64(params[1]);
            oss.put_str(" ");
            oss.put_f64(params[2]);
            oss.str().to_vec()
        };

        // SOPNode.
        formatter.write_start_tag_with(TAG_SOPNODE, &attributes);
        {
            let mut scope_indent = XmlScopeIndent::new(formatter);

            let mut desc = Vec::new();
            get_elements_values(
                op.get_format_metadata().get_children_elements(),
                METADATA_SOP_DESCRIPTION,
                &mut desc,
            );
            write_descriptions(&mut scope_indent, TAG_DESCRIPTION, &desc);

            let slope = triple(&mut oss, cdl.get_slope_params().get_rgb());
            scope_indent.write_content_tag(TAG_SLOPE, &slope);

            let offset = triple(&mut oss, cdl.get_offset_params().get_rgb());
            scope_indent.write_content_tag(TAG_OFFSET, &offset);

            let power = triple(&mut oss, cdl.get_power_params().get_rgb());
            scope_indent.write_content_tag(TAG_POWER, &power);
        }
        formatter.write_end_tag(TAG_SOPNODE);

        // SatNode.
        formatter.write_start_tag_with(TAG_SATNODE, &attributes);
        {
            let mut scope_indent = XmlScopeIndent::new(formatter);

            let mut desc = Vec::new();
            get_elements_values(
                op.get_format_metadata().get_children_elements(),
                METADATA_SAT_DESCRIPTION,
                &mut desc,
            );
            write_descriptions(&mut scope_indent, TAG_DESCRIPTION, &desc);

            oss.clear_str();
            oss.put_f64(cdl.get_saturation());
            let saturation = oss.str().to_vec();
            scope_indent.write_content_tag(TAG_SATURATION, &saturation);
        }
        formatter.write_end_tag(TAG_SATNODE);
        Ok(())
    }

    /// The descriptions, then the input and viewing descriptions.
    ///
    /// Port of `CDLWriter::writeFormatMetadata` (CTFTransform.cpp:918-933 @ v2.5.2).
    fn write_format_metadata(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let op = self.get_op();
        let elements = op.get_format_metadata().get_children_elements();
        let mut desc = Vec::new();
        get_elements_values(elements, METADATA_DESCRIPTION, &mut desc);
        write_descriptions(formatter, TAG_DESCRIPTION, &desc);
        desc.clear();
        get_elements_values(elements, METADATA_INPUT_DESCRIPTION, &mut desc);
        write_descriptions(formatter, METADATA_INPUT_DESCRIPTION, &desc);
        desc.clear();
        get_elements_values(elements, METADATA_VIEWING_DESCRIPTION, &mut desc);
        write_descriptions(formatter, METADATA_VIEWING_DESCRIPTION, &desc);
        Ok(())
    }
}

/// The writer of a FixedFunction (CTF only): its style and its parameters.
///
/// Port of `FixedFunctionWriter` (CTFTransform.cpp:1053-1119 @ v2.5.2).
struct FixedFunctionWriter<'a> {
    /// `m_ff`.
    ff: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl OpWriter for FixedFunctionWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.ff
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_FIXED_FUNCTION
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `FixedFunctionWriter::getAttributes` (CTFTransform.cpp:1094-1115 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;
        let OpData::FixedFunction(ff) = self.ff else {
            unreachable!("a FixedFunction writer writes a FixedFunction");
        };
        let style = ff.style().to_str(false);
        attributes.push((ATTR_STYLE.to_vec(), style.as_bytes().to_vec()));
        let params = ff.params();

        let num_params = params.len();
        if num_params != 0 {
            let mut i = 0;
            let mut ff_params = OStringStream::new(Crt::NATIVE);
            ff_params.precision = DOUBLE_PRECISION;
            write_value(params[i], &mut ff_params);
            i += 1;
            while i < num_params {
                ff_params.put_str(" ");
                write_value(params[i], &mut ff_params);
                i += 1;
            }
            attributes.push((ATTR_PARAMS.to_vec(), ff_params.into_bytes()));
        }
        Ok(())
    }

    /// Port of `FixedFunctionWriter::writeContent` (CTFTransform.cpp:1117-1119 @ v2.5.2).
    fn write_content(&self, _formatter: &mut XmlFormatter<'_>) -> Result<()> {
        Ok(())
    }
}

/// The writer of a GradingRGBCurve (CTF only): its style and direction, bypassLinToLog, the
/// curves that differ from the style's default (their control points, and their slopes unless
/// default), and whether it is dynamic.
///
/// Port of `GradingRGBCurveWriter` (CTFTransform.cpp:1502-1630 @ v2.5.2).
struct GradingRgbCurveWriter<'a> {
    /// `m_curves`.
    curves: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl GradingRgbCurveWriter<'_> {
    fn curves(&self) -> &GradingRgbCurveOpData {
        let OpData::GradingRgbCurve(curves) = self.curves else {
            unreachable!("a GradingRGBCurve writer writes a GradingRGBCurve");
        };
        curves
    }

    /// The element `tag` of `curve`: its control points, one "x y" a line, and its slopes
    /// unless default.
    ///
    /// Port of `GradingRGBCurveWriter::writeCurve` (CTFTransform.cpp:1559-1603 @ v2.5.2).
    fn write_curve(formatter: &mut XmlFormatter<'_>, tag: &[u8], curve: &GradingBSplineCurve) {
        formatter.write_start_tag_with(tag, &Attributes::new());
        {
            let mut si0 = XmlScopeIndent::new(formatter);
            si0.write_start_tag_with(TAG_CURVE_CTRL_PNTS, &Attributes::new());
            {
                let mut si1 = XmlScopeIndent::new(&mut si0);
                let num_pnts = curve.num_control_points();

                // Write 1 control point per line in the form of "X Y"
                for ct_pt in &curve.control_points()[..num_pnts] {
                    let mut oss = OStringStream::new(Crt::NATIVE);
                    set_ostream::<f32>(&mut oss);
                    oss.put_f32(ct_pt.x);
                    oss.put_str(" ");
                    oss.put_f32(ct_pt.y);
                    si1.write_content(oss.str());
                }
            }
            si0.write_end_tag(TAG_CURVE_CTRL_PNTS);

            if !curve.slopes_are_default() {
                si0.write_start_tag_with(TAG_CURVE_SLOPES, &Attributes::new());
                {
                    let mut si1 = XmlScopeIndent::new(&mut si0);
                    // (Number of slopes is always the same as control points.)
                    let num_slopes = curve.num_control_points();
                    let mut oss = OStringStream::new(Crt::NATIVE);
                    set_ostream::<f32>(&mut oss);
                    for &val in &curve.slopes()[..num_slopes] {
                        oss.put_f32(val);
                        oss.put_str(" ");
                    }
                    si1.write_content(oss.str());
                }
                si0.write_end_tag(TAG_CURVE_SLOPES);
            }
        }

        formatter.write_end_tag(tag);
    }
}

impl OpWriter for GradingRgbCurveWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.curves
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_RGB_CURVE
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `GradingRGBCurveWriter::getAttributes` (CTFTransform.cpp:1543-1557 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;

        let curves = self.curves();
        let style = curves.get_style();
        let dir = curves.get_direction();

        let style_str = convert_grading_style_and_dir_to_string(style, dir);
        attributes.push((ATTR_STYLE.to_vec(), style_str.to_vec()));

        if curves.get_bypass_lin_to_log() {
            attributes.push((ATTR_BYPASS_LIN_TO_LOG.to_vec(), b"true".to_vec()));
        }
        Ok(())
    }

    /// Port of `GradingRGBCurveWriter::writeContent` (CTFTransform.cpp:1605-1630 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let curves = self.curves();
        let vals = curves.get_value();

        let def_curve = if curves.get_style() == GradingStyle::Lin {
            default_lin_curve()
        } else {
            default_curve()
        };

        const CURVE_TAGS: [&[u8]; 4] = [
            TAG_RGB_CURVE_RED,
            TAG_RGB_CURVE_GREEN,
            TAG_RGB_CURVE_BLUE,
            TAG_RGB_CURVE_MASTER,
        ];
        for (c, tag) in [
            RgbCurveType::Red,
            RgbCurveType::Green,
            RgbCurveType::Blue,
            RgbCurveType::Master,
        ]
        .into_iter()
        .zip(CURVE_TAGS)
        {
            let curve = vals.curve(c)?;
            if *curve != def_curve || !curve.slopes_are_default() {
                Self::write_curve(formatter, tag, curve);
            }
        }
        if curves.is_dynamic() {
            let attributes: Attributes =
                vec![(ATTR_PARAM.to_vec(), TAG_DYN_PROP_RGBCURVE.to_vec())];
            formatter.write_empty_tag(TAG_DYNAMIC_PARAMETER, &attributes);
        }
        Ok(())
    }
}

/// The writer of a Gamma (CTF before 2.0) or an Exponent: its style and its parameters, for
/// all channels or each.
///
/// Port of `GammaWriter` (CTFTransform.cpp:1123-1276 @ v2.5.2).
struct GammaWriter<'a> {
    /// `m_version`.
    version: CtfVersion,
    /// `m_gamma`.
    gamma: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl GammaWriter<'_> {
    fn gamma(&self) -> &GammaOpData {
        let OpData::Gamma(gamma) = self.gamma else {
            unreachable!("a Gamma writer writes a Gamma");
        };
        gamma
    }
}

/// Adds the attributes of a GammaParams (or ExponentParams) element: the gamma (or exponent),
/// and the offset of the moncurve styles.
///
/// Port of `AddGammaParams` (CTFTransform.cpp:1181-1214 @ v2.5.2).
fn add_gamma_params(
    attributes: &mut Attributes,
    params: &[f64],
    style: GammaStyle,
    use_gamma: bool,
) {
    attributes.push((
        if use_gamma { ATTR_GAMMA } else { ATTR_EXPONENT }.to_vec(),
        double_string(params[0]),
    ));

    match style {
        GammaStyle::MoncurveFwd
        | GammaStyle::MoncurveRev
        | GammaStyle::MoncurveMirrorFwd
        | GammaStyle::MoncurveMirrorRev => {
            attributes.push((ATTR_OFFSET.to_vec(), double_string(params[1])));
        }
        GammaStyle::BasicFwd
        | GammaStyle::BasicRev
        | GammaStyle::BasicMirrorFwd
        | GammaStyle::BasicMirrorRev
        | GammaStyle::BasicPassThruFwd
        | GammaStyle::BasicPassThruRev => {}
    }
}

impl OpWriter for GammaWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.gamma
    }
    /// Port of `GammaWriter::getTagName` (CTFTransform.cpp:1162-1169 @ v2.5.2).
    fn get_tag_name(&self) -> &[u8] {
        if self.version < CTF_PROCESS_LIST_VERSION_2_0 {
            return TAG_GAMMA;
        }
        TAG_EXPONENT
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `GammaWriter::getAttributes` (CTFTransform.cpp:1171-1176 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;
        let style = GammaOpData::convert_style_to_string(self.gamma().style());
        attributes.push((ATTR_STYLE.to_vec(), style.as_bytes().to_vec()));
        Ok(())
    }

    /// Port of `GammaWriter::writeContent` (CTFTransform.cpp:1216-1276 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let use_gamma = self.version < CTF_PROCESS_LIST_VERSION_2_0;
        let params_tag = if use_gamma {
            TAG_GAMMA_PARAMS
        } else {
            TAG_EXPONENT_PARAMS
        };
        let gamma = self.gamma();
        if gamma.is_non_channel_dependent() {
            // RGB channels equal and A is identity, just write one element.
            let mut attributes = Attributes::new();

            add_gamma_params(
                &mut attributes,
                gamma.red_params(),
                gamma.style(),
                use_gamma,
            );

            formatter.write_empty_tag(params_tag, &attributes);
        } else {
            for (chan, params) in [
                (b"R", gamma.red_params()),
                (b"G", gamma.green_params()),
                (b"B", gamma.blue_params()),
            ] {
                let mut attributes = Attributes::new();
                attributes.push((ATTR_CHAN.to_vec(), chan.to_vec()));
                add_gamma_params(&mut attributes, params, gamma.style(), use_gamma);

                formatter.write_empty_tag(params_tag, &attributes);
            }

            if !gamma.is_alpha_component_identity() {
                // Alpha
                let mut attributes = Attributes::new();
                attributes.push((ATTR_CHAN.to_vec(), b"A".to_vec()));
                add_gamma_params(
                    &mut attributes,
                    gamma.alpha_params(),
                    gamma.style(),
                    use_gamma,
                );

                formatter.write_empty_tag(params_tag, &attributes);
            }
        }
        Ok(())
    }
}

/// The writer of a Log: its style, and its parameters but for log2 and log10, for all
/// channels or each.
///
/// Port of `LogWriter` (CTFTransform.cpp:1896-2037 @ v2.5.2).
struct LogWriter<'a> {
    /// `m_log`.
    log: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

/// Adds the attribute `attr_name` of `attr_value`.
///
/// Port of `AddLogParam` (CTFTransform.cpp:1967-1975 @ v2.5.2).
fn add_log_param(attributes: &mut Attributes, attr_name: &[u8], attr_value: f64) {
    attributes.push((attr_name.to_vec(), double_string(attr_value)));
}

/// Adds the base and the params of a LogParams element.
///
/// Port of `AddLogParams` (CTFTransform.cpp:1977-1995 @ v2.5.2).
fn add_log_params(attributes: &mut Attributes, params: &[f64], base: f64) {
    // LogOpData::validate ensure that params size is between 4 & 6.
    add_log_param(attributes, ATTR_BASE, base);
    add_log_param(attributes, ATTR_LINSIDESLOPE, params[LIN_SIDE_SLOPE]);
    add_log_param(attributes, ATTR_LINSIDEOFFSET, params[LIN_SIDE_OFFSET]);
    add_log_param(attributes, ATTR_LOGSIDESLOPE, params[LOG_SIDE_SLOPE]);
    add_log_param(attributes, ATTR_LOGSIDEOFFSET, params[LOG_SIDE_OFFSET]);
    if params.len() > 4 {
        add_log_param(attributes, ATTR_LINSIDEBREAK, params[LIN_SIDE_BREAK]);
    }
    if params.len() > 5 {
        add_log_param(attributes, ATTR_LINEARSLOPE, params[LINEAR_SLOPE]);
    }
}

impl LogWriter<'_> {
    fn log(&self) -> &ocio_ops::ops::log::log_op_data::LogOpData {
        let OpData::Log(log) = self.log else {
            unreachable!("a Log writer writes a Log");
        };
        log
    }
}

impl OpWriter for LogWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.log
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_LOG
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `LogWriter::getAttributes` (CTFTransform.cpp:1937-1962 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;

        let log = self.log();
        let fwd = log.direction() == TransformDirection::Forward;
        let style = if log.is_log2() {
            if fwd { LOG2_STR } else { ANTI_LOG2_STR }
        } else if log.is_log10() {
            if fwd { LOG10_STR } else { ANTI_LOG10_STR }
        } else if log.is_camera() {
            if fwd {
                CAMERA_LIN_TO_LOG_STR
            } else {
                CAMERA_LOG_TO_LIN_STR
            }
        } else if fwd {
            LIN_TO_LOG_STR
        } else {
            LOG_TO_LIN_STR
        };

        attributes.push((ATTR_STYLE.to_vec(), style.as_bytes().to_vec()));
        Ok(())
    }

    /// Port of `LogWriter::writeContent` (CTFTransform.cpp:1997-2037 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let log = self.log();
        if log.is_log2() || log.is_log10() {
            // No parameters to save.
            return Ok(());
        }

        if log.all_components_equal() {
            // All channels equal, just write one element.
            let mut attributes = Attributes::new();

            add_log_params(&mut attributes, log.red_params(), log.base());

            formatter.write_empty_tag(TAG_LOG_PARAMS, &attributes);
        } else {
            for (chan, params) in [
                (b"R", log.red_params()),
                (b"G", log.green_params()),
                (b"B", log.blue_params()),
            ] {
                let mut attributes = Attributes::new();
                attributes.push((ATTR_CHAN.to_vec(), chan.to_vec()));
                add_log_params(&mut attributes, params, log.base());

                formatter.write_empty_tag(TAG_LOG_PARAMS, &attributes);
            }
        }
        Ok(())
    }
}

/// The writer of a Range: its bounds, scaled to the file bit depths.
///
/// Port of `RangeWriter` (CTFTransform.cpp:2434-2512 @ v2.5.2).
struct RangeWriter<'a> {
    /// `m_range`.
    range: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

/// Writes `<tag> value </tag>`.
///
/// Port of `WriteTag` (CTFTransform.cpp:2474-2480 @ v2.5.2).
fn write_tag(fmt: &mut XmlFormatter<'_>, tag: &[u8], value: f64) {
    let content = [b" ".as_slice(), &double_string(value), b" "].concat();
    fmt.write_content_tag(tag, &content);
}

impl OpWriter for RangeWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.range
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_RANGE
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `RangeWriter::writeContent` (CTFTransform.cpp:2482-2512 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let OpData::Range(range) = self.range else {
            unreachable!("a Range writer writes a Range");
        };
        let forward;
        let range = if range.get_direction() == TransformDirection::Inverse {
            forward = range.get_as_forward()?;
            &forward
        } else {
            range
        };

        let out_scale = get_bit_depth_max_value(self.out_bit_depth)?;
        let in_scale = get_bit_depth_max_value(self.in_bit_depth)?;

        if !range.min_is_empty() {
            write_tag(
                formatter,
                TAG_MIN_IN_VALUE,
                range.get_min_in_value() * in_scale,
            );
        }

        if !range.max_is_empty() {
            write_tag(
                formatter,
                TAG_MAX_IN_VALUE,
                range.get_max_in_value() * in_scale,
            );
        }

        if !range.min_is_empty() {
            write_tag(
                formatter,
                TAG_MIN_OUT_VALUE,
                range.get_min_out_value() * out_scale,
            );
        }

        if !range.max_is_empty() {
            write_tag(
                formatter,
                TAG_MAX_OUT_VALUE,
                range.get_max_out_value() * out_scale,
            );
        }
        Ok(())
    }
}

/// The writer of a Lut1D or an InverseLUT1D: its interpolation, half domain, raw halfs and hue
/// adjust, and its array, scaled to the file bit depth of its values.
///
/// Port of `Lut1DWriter` (CTFTransform.cpp:2042-2179 @ v2.5.2).
struct Lut1DWriter<'a> {
    /// `m_lut`.
    lut: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl Lut1DWriter<'_> {
    fn lut(&self) -> &Lut1DOpData {
        let OpData::Lut1D(lut) = self.lut else {
            unreachable!("a Lut1D writer writes a Lut1D");
        };
        lut
    }
}

impl OpWriter for Lut1DWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.lut
    }

    /// Port of `Lut1DWriter::getTagName` (CTFTransform.cpp:2078-2088 @ v2.5.2).
    fn get_tag_name(&self) -> &[u8] {
        if self.lut().get_direction() == TransformDirection::Forward {
            TAG_LUT1D
        } else {
            TAG_INVLUT1D
        }
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `Lut1DWriter::getAttributes` (CTFTransform.cpp:2090-2120 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;
        let lut = self.lut();

        // If the client requests INTERP_LINEAR, we want to write it to the attribute (even
        // though linear is what CLF specifies as its default, some clients may want to lock
        // down that behavior). INTERP_DEFAULT means "do not write the attribute".
        let interpolation = lut.get_interpolation();
        if let Some(interpolation_name) = get_interpolation_1d_name(interpolation)
            && !interpolation_name.is_empty()
        {
            attributes.push((ATTR_INTERPOLATION.to_vec(), interpolation_name.to_vec()));
        }

        if lut.is_input_half_domain() {
            attributes.push((ATTR_HALF_DOMAIN.to_vec(), b"true".to_vec()));
        }

        if lut.is_output_raw_halfs() {
            attributes.push((ATTR_RAW_HALFS.to_vec(), b"true".to_vec()));
        }

        if lut.get_hue_adjust() == Lut1DHueAdjust::Dw3 {
            attributes.push((ATTR_HUE_ADJUST.to_vec(), b"dw3".to_vec()));
        }
        Ok(())
    }

    /// Port of `Lut1DWriter::writeContent` (CTFTransform.cpp:2122-2179 @ v2.5.2). The raw
    /// halfs, upstream's `unsigned` values times the `float` scale 1, are written as the
    /// `float`s of that product.
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        // Note: As of CTF v1.7 we support IndexMaps and that member of the LUT is populated,
        // however, since we convert it to a Range on reading, we do not want to write out the
        // IndexMap.

        let lut = self.lut();
        let array = lut.get_array();
        let num_components = array.get_num_color_components() as usize;
        let dimension = format!("{} {}", array.get_length(), num_components);

        let attributes: Attributes = vec![(ATTR_DIMENSION.to_vec(), dimension.into_bytes())];

        formatter.write_start_tag_with(TAG_ARRAY, &attributes);

        // To avoid needing to duplicate the const objects, we scale the values on-the-fly
        // while writing.
        let array_bit_depth = if lut.get_direction() == TransformDirection::Inverse {
            self.in_bit_depth
        } else {
            self.out_bit_depth
        };
        let scale = get_bit_depth_max_value(array_bit_depth)? as f32;
        let iter_step = if num_components == 1 { 3 } else { 1 };

        if lut.is_output_raw_halfs() {
            let values: Vec<f32> = array
                .get_values()
                .iter()
                .map(|&v| f32::from(float_to_half(v * scale)))
                .collect();

            write_values(
                formatter,
                &values,
                num_components,
                BitDepth::Uint16,
                iter_step,
                1.0f32,
            )?;
        } else {
            write_values(
                formatter,
                array.get_values(),
                num_components,
                array_bit_depth,
                iter_step,
                scale,
            )?;
        }

        formatter.write_end_tag(TAG_ARRAY);
        Ok(())
    }
}

/// The writer of a Lut3D or an InverseLUT3D: its interpolation, and its array, scaled to the
/// file bit depth of its values.
///
/// Port of `Lut3DWriter` (CTFTransform.cpp:2183-2280 @ v2.5.2).
struct Lut3DWriter<'a> {
    /// `m_lut`.
    lut: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl Lut3DWriter<'_> {
    fn lut(&self) -> &Lut3DOpData {
        let OpData::Lut3D(lut) = self.lut else {
            unreachable!("a Lut3D writer writes a Lut3D");
        };
        lut
    }
}

impl OpWriter for Lut3DWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.lut
    }

    /// Port of `Lut3DWriter::getTagName` (CTFTransform.cpp:2219-2229 @ v2.5.2).
    fn get_tag_name(&self) -> &[u8] {
        if self.lut().get_direction() == TransformDirection::Forward {
            TAG_LUT3D
        } else {
            TAG_INVLUT3D
        }
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `Lut3DWriter::getAttributes` (CTFTransform.cpp:2231-2244 @ v2.5.2).
    fn get_attributes(&self, attributes: &mut Attributes) -> Result<()> {
        op_writer_attributes(self, attributes)?;

        let interpolation = self.lut().get_interpolation();

        // Please see comment in Lut1DWriter.
        if let Some(interpolation_name) = get_interpolation_3d_name(interpolation)
            && !interpolation_name.is_empty()
        {
            attributes.push((ATTR_INTERPOLATION.to_vec(), interpolation_name.to_vec()));
        }
        Ok(())
    }

    /// Port of `Lut3DWriter::writeContent` (CTFTransform.cpp:2246-2280 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        // Note: As of CTF v1.7 we support IndexMaps and that member of the Lut is populated,
        // however, since we convert it to a Range on reading, we do not want to write out the
        // IndexMap.

        let lut = self.lut();
        let array = lut.get_array();
        let length = array.get_length();
        let dimension = format!(
            "{length} {length} {length} {}",
            array.get_num_color_components()
        );

        let attributes: Attributes = vec![(ATTR_DIMENSION.to_vec(), dimension.into_bytes())];

        formatter.write_start_tag_with(TAG_ARRAY, &attributes);

        // To avoid needing to duplicate the const objects, we scale the values on-the-fly
        // while writing.
        let array_bit_depth = if lut.get_direction() == TransformDirection::Inverse {
            self.in_bit_depth
        } else {
            self.out_bit_depth
        };
        let scale = get_bit_depth_max_value(array_bit_depth)? as f32;

        write_values(formatter, array.get_values(), 3, array_bit_depth, 1, scale)?;

        formatter.write_end_tag(TAG_ARRAY);
        Ok(())
    }
}

/// The writer of a Matrix: its array of 3 or 4 rows, with or without its offsets, in the
/// CTF 1.x `dim` of three numbers before 2.0.
///
/// Port of `MatrixWriter` (CTFTransform.cpp:2284-2430 @ v2.5.2).
struct MatrixWriter<'a> {
    /// `m_version`.
    version: CtfVersion,
    /// `m_matrix`.
    matrix: &'a OpData,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
}

impl OpWriter for MatrixWriter<'_> {
    fn get_op(&self) -> &OpData {
        self.matrix
    }
    fn get_tag_name(&self) -> &[u8] {
        TAG_MATRIX
    }
    fn in_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }
    fn out_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// Port of `MatrixWriter::writeContent` (CTFTransform.cpp:2326-2430 @ v2.5.2).
    fn write_content(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        // Matrix Op supports 4 xml formats:
        //   1) 4x5x4, matrix with alpha and offsets.
        //   2) 4x4x4, matrix with alpha and no offsets.
        //   3) 3x4x3, matrix only with offsets and no alpha.
        //   4) 3x3x3, matrix with no alpha and no offsets.

        let save_dim3 = self.version < CTF_PROCESS_LIST_VERSION_2_0;

        let OpData::Matrix(matrix) = self.matrix else {
            unreachable!("a Matrix writer writes a Matrix");
        };
        let forward;
        let matrix: &MatrixOpData = if matrix.get_direction() == TransformDirection::Inverse {
            forward = matrix.get_as_forward()?;
            &forward
        } else {
            matrix
        };

        let has_alpha = matrix.has_alpha()?;
        let dimension_attr: &[u8] = if has_alpha {
            if matrix.has_offsets() {
                if save_dim3 { b"4 5 4" } else { b"4 5" }
            } else if save_dim3 {
                b"4 4 4"
            } else {
                b"4 4"
            }
        } else if matrix.has_offsets() {
            if save_dim3 { b"3 4 3" } else { b"3 4" }
        } else if save_dim3 {
            b"3 3 3"
        } else {
            b"3 3"
        };

        let attributes: Attributes = vec![(ATTR_DIMENSION.to_vec(), dimension_attr.to_vec())];

        formatter.write_start_tag_with(TAG_ARRAY, &attributes);

        let values = matrix.get_array().get_values();
        let offsets = matrix.get_offsets();

        let out_scale = get_bit_depth_max_value(self.out_bit_depth)?;
        let in_out_scale = out_scale / get_bit_depth_max_value(self.in_bit_depth)?;

        if has_alpha {
            if matrix.has_offsets() {
                // Write in 4x5x4 mode.
                let v: [f64; 20] = [
                    values[0] * in_out_scale,
                    values[1] * in_out_scale,
                    values[2] * in_out_scale,
                    values[3] * in_out_scale,
                    offsets[0] * out_scale,
                    values[4] * in_out_scale,
                    values[5] * in_out_scale,
                    values[6] * in_out_scale,
                    values[7] * in_out_scale,
                    offsets[1] * out_scale,
                    values[8] * in_out_scale,
                    values[9] * in_out_scale,
                    values[10] * in_out_scale,
                    values[11] * in_out_scale,
                    offsets[2] * out_scale,
                    values[12] * in_out_scale,
                    values[13] * in_out_scale,
                    values[14] * in_out_scale,
                    values[15] * in_out_scale,
                    offsets[3] * out_scale,
                ];

                write_values(formatter, &v, 5, BitDepth::F32, 1, 1.0)?;
            } else {
                // Write in 4x4x4 compact mode.
                let v: [f64; 16] = std::array::from_fn(|i| values[i] * in_out_scale);

                write_values(formatter, &v, 4, BitDepth::F32, 1, 1.0)?;
            }
        } else if matrix.has_offsets() {
            // Write in 3x4x3 compact mode.
            let v: [f64; 12] = [
                values[0] * in_out_scale,
                values[1] * in_out_scale,
                values[2] * in_out_scale,
                offsets[0] * out_scale,
                values[4] * in_out_scale,
                values[5] * in_out_scale,
                values[6] * in_out_scale,
                offsets[1] * out_scale,
                values[8] * in_out_scale,
                values[9] * in_out_scale,
                values[10] * in_out_scale,
                offsets[2] * out_scale,
            ];

            write_values(formatter, &v, 4, BitDepth::F32, 1, 1.0)?;
        } else {
            // Write in 3x3x3 compact mode.
            let v: [f64; 9] = [
                values[0] * in_out_scale,
                values[1] * in_out_scale,
                values[2] * in_out_scale,
                values[4] * in_out_scale,
                values[5] * in_out_scale,
                values[6] * in_out_scale,
                values[8] * in_out_scale,
                values[9] * in_out_scale,
                values[10] * in_out_scale,
            ];

            write_values(formatter, &v, 3, BitDepth::F32, 1, 1.0)?;
        }

        formatter.write_end_tag(TAG_ARRAY);
        Ok(())
    }
}

/// The error of an op CLF can't hold.
///
/// Port of `ThrowWriteOp` (CTFTransform.cpp:2645-2651 @ v2.5.2).
fn throw_write_op(op_type: &str) -> Exception {
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.put_str("Transform uses the '");
    oss.put_str(op_type);
    oss.put_str("' op which cannot be written as CLF.  Use CTF format or Bake the transform.");
    Exception::new(oss.into_bytes())
}

/// The file input bit depth an op prefers: a Matrix's and a Range's, an inverse LUT's file
/// output depth (which scales its array), `F32` for the others.
///
/// Port of `GetInputFileBD` (CTFTransform.cpp:2653-2693 @ v2.5.2).
fn get_input_file_bd(op: &OpData) -> Result<BitDepth> {
    let op_type = op.get_type();
    match op {
        OpData::Matrix(mat) => {
            let bd = mat.get_file_input_bit_depth();
            get_validated_file_bit_depth(bd, op_type)
        }
        OpData::Range(range) => {
            let bd = range.get_file_input_bit_depth();
            get_validated_file_bit_depth(bd, op_type)
        }
        // For an InverseLut1D, the file "out" depth, which determines the array scaling, must
        // actually be set as the in-depth.
        OpData::Lut1D(lut) if lut.get_direction() == TransformDirection::Inverse => {
            let bd = lut.get_file_output_bit_depth();
            get_validated_file_bit_depth(bd, op_type)
        }
        // For an InverseLut3D, the file "out" depth, which determines the array scaling, must
        // actually be set as the in-depth.
        OpData::Lut3D(lut) if lut.get_direction() == TransformDirection::Inverse => {
            let bd = lut.get_file_output_bit_depth();
            get_validated_file_bit_depth(bd, op_type)
        }
        _ => Ok(BitDepth::F32),
    }
}

/// The writer of a ProcessList: its version (CLF 3, or the lowest CTF version that fits), id
/// (the ops' cache IDs hashed when it has none), name, inverseOf, descriptions, descriptors
/// and Info, then its ops (an identity matrix when it has none).
///
/// Port of `TransformWriter` (CTFTransform.h:266-289, CTFTransform.cpp:2518-3004 @ v2.5.2).
#[derive(Debug)]
pub struct TransformWriter<'t> {
    /// `m_transform`.
    transform: &'t CtfReaderTransform,
    /// `m_isCLF`.
    is_clf: bool,
}

impl<'t> TransformWriter<'t> {
    /// Port of `TransformWriter::TransformWriter` (CTFTransform.cpp:2518-2525 @ v2.5.2).
    pub fn new(transform: &'t CtfReaderTransform, is_clf: bool) -> TransformWriter<'t> {
        TransformWriter { transform, is_clf }
    }

    /// The Info metadata `m` and its children.
    ///
    /// Port of `TransformWriter::writeProcessListMetadata` (CTFTransform.cpp:2612-2641 @
    /// v2.5.2).
    fn write_process_list_metadata(formatter: &mut XmlFormatter<'_>, m: &FormatMetadataImpl) {
        if m.get_children_elements().is_empty() {
            let info_value = m.get_element_value();
            if m.get_num_attributes() != 0 || !info_value.is_empty() {
                formatter.write_content_tag_with(
                    m.get_element_name(),
                    m.get_attributes(),
                    m.get_element_value(),
                );
            }
        } else {
            formatter.write_start_tag_with(m.get_element_name(), m.get_attributes());
            let value = m.get_element_value();
            if !value.is_empty() {
                formatter.write_content(m.get_element_value());
            }

            let items = m.get_children_elements();
            for it in items {
                let mut scope_indent = XmlScopeIndent::new(formatter);
                Self::write_process_list_metadata(&mut scope_indent, it);
            }

            formatter.write_end_tag(m.get_element_name());
        }
    }

    /// The ops, each with the file bit depths that keep its preferred ones where they can.
    ///
    /// Port of `TransformWriter::writeOps` (CTFTransform.cpp:2695-3004 @ v2.5.2).
    fn write_ops(&self, formatter: &mut XmlFormatter<'_>, version: CtfVersion) -> Result<()> {
        // If the ops have a specific file input/output bit-depth that was set (e.g., if the
        // ops were originally read in from a CTF/CLF file), then we try to preserve those
        // values on write. Otherwise, default to 32f.
        let mut in_bd;
        let mut out_bd = BitDepth::F32;

        let ops: &[OpDataRcPtr] = self.transform.get_ops();
        let num_ops = ops.len();
        let mut num_saved_ops = 0;
        if num_ops != 0 {
            // Initialize the input bit-depth from the first op. Thereafter, the input depth
            // will be determined from the output depth of the previous op that was written.
            in_bd = get_input_file_bd(&ops[0])?;

            for i in 0..num_ops {
                let op = &ops[i];

                // Each op is allowed to set its preferred output depth, so that is always
                // respected. However, we try to honour preferred input depth too, where
                // possible. If the next op has a preferred file input bit-depth, use that as
                // the default out-depth for the current op. However, if the current op has a
                // preferred file out depth, that will take precedence below. If an op's
                // preferred file depth cannot be honoured, it still results in a perfectly
                // valid CTF file, it's only that the scaling of array values may be adjusted
                // relative to the original CTF file it was created from (if any).
                if i + 1 < num_ops {
                    let next_op = &ops[i + 1];
                    // Return file input bit-depth for ops with a preference, F32 for others.
                    out_bd = get_input_file_bd(next_op)?;
                }

                let op_type = op.get_type();

                if op_type != OpDataType::NoOp {
                    op.validate()?;
                    num_saved_ops += 1;
                }

                match &**op {
                    OpData::Cdl(_) => {
                        let op_writer = CdlWriter {
                            cdl: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Exponent(exp) => {
                        let param_r = vec![exp.exp4[0]];
                        let param_g = vec![exp.exp4[1]];
                        let param_b = vec![exp.exp4[2]];
                        let param_a = vec![exp.exp4[3]];

                        let mut gamma_data = GammaOpData::new(
                            GammaStyle::BasicFwd,
                            param_r,
                            param_g,
                            param_b,
                            param_a,
                        );
                        *gamma_data.get_format_metadata_mut() = exp.get_format_metadata().clone();

                        if self.is_clf && !gamma_data.is_alpha_component_identity() {
                            return Err(throw_write_op("Exponent with alpha"));
                        }

                        let gamma = OpData::Gamma(gamma_data);
                        let op_writer = GammaWriter {
                            version,
                            gamma: &gamma,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::FixedFunction(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("FixedFunction"));
                        }

                        let op_writer = FixedFunctionWriter {
                            ff: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Gamma(gamma) => {
                        if self.is_clf && !gamma.is_alpha_component_identity() {
                            return Err(throw_write_op("Gamma with alpha component"));
                        }

                        let op_writer = GammaWriter {
                            version,
                            gamma: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    // CTFTransform.cpp:2776-2789: `ExposureContrastWriter` comes with WP 4.6f.
                    OpData::ExposureContrast(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("ExposureContrast"));
                        }
                        return Err(Exception::new(
                            "ExposureContrastWriter is not ported yet (WP 4.6f).",
                        ));
                    }
                    // CTFTransform.cpp:2821-2834: `GradingPrimaryWriter` comes with WP 4.6f.
                    OpData::GradingPrimary(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("GradingPrimary"));
                        }
                        return Err(Exception::new(
                            "GradingPrimaryWriter is not ported yet (WP 4.6f).",
                        ));
                    }
                    OpData::GradingRgbCurve(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("GradingRGBCurve"));
                        }

                        let op_writer = GradingRgbCurveWriter {
                            curves: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    // CTFTransform.cpp:2849-2862: `GradingHueCurveWriter` comes with WP 4.6f.
                    OpData::GradingHueCurve(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("GradingHueCurve"));
                        }
                        return Err(Exception::new(
                            "GradingHueCurveWriter is not ported yet (WP 4.6f).",
                        ));
                    }
                    // CTFTransform.cpp:2863-2876: `GradingToneWriter` comes with WP 4.6f.
                    OpData::GradingTone(_) => {
                        if self.is_clf {
                            return Err(throw_write_op("GradingTone"));
                        }
                        return Err(Exception::new(
                            "GradingToneWriter is not ported yet (WP 4.6f).",
                        ));
                    }
                    OpData::Log(_) => {
                        let op_writer = LogWriter {
                            log: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Lut1D(lut) => {
                        if self.is_clf && lut.get_direction() != TransformDirection::Forward {
                            return Err(throw_write_op("InverseLUT1D"));
                        }
                        // Avoid copying LUT, write will take bit-depth into account.
                        if lut.get_direction() == TransformDirection::Forward {
                            // For an inverse Lut1D, the fileOutDepth is used for the inBitDepth
                            // above.
                            out_bd = get_validated_file_bit_depth(
                                lut.get_file_output_bit_depth(),
                                op_type,
                            )?;
                        }
                        let op_writer = Lut1DWriter {
                            lut: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Lut3D(lut) => {
                        if self.is_clf && lut.get_direction() != TransformDirection::Forward {
                            return Err(throw_write_op("InverseLUT3D"));
                        }
                        // Avoid copying LUT, write will take bit-depth into account.
                        if lut.get_direction() == TransformDirection::Forward {
                            // For an inverse Lut3D, the fileOutDepth is used for the inBitDepth
                            // above.
                            out_bd = get_validated_file_bit_depth(
                                lut.get_file_output_bit_depth(),
                                op_type,
                            )?;
                        }
                        let op_writer = Lut3DWriter {
                            lut: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Matrix(mat_src) => {
                        if self.is_clf && mat_src.has_alpha()? {
                            return Err(throw_write_op("Matrix with alpha component"));
                        }

                        out_bd = get_validated_file_bit_depth(
                            mat_src.get_file_output_bit_depth(),
                            op_type,
                        )?;
                        // inBD has already been set at previous iteration. inBD can be:
                        // - This op input file bit-depth if previous op does not define an
                        //   output file bit-depth.
                        // - Previous op output file bit-depth if previous op is a LUT, a
                        //   Matrix or a Range.

                        let op_writer = MatrixWriter {
                            version,
                            matrix: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Range(range_src) => {
                        out_bd = get_validated_file_bit_depth(
                            range_src.get_file_output_bit_depth(),
                            op_type,
                        )?;
                        // inBD has already been set at previous iteration.

                        let op_writer = RangeWriter {
                            range: op,
                            in_bit_depth: in_bd,
                            out_bit_depth: out_bd,
                        };
                        op_writer.write(formatter)?;
                    }
                    OpData::Reference(_) => {
                        return Err(Exception::new(
                            "Reference ops should have been replaced by their content.",
                        ));
                    }
                    OpData::NoOp(_) => {
                        return Err(Exception::new("Noop ops should have been removed."));
                    }
                }

                // For next op.
                in_bd = out_bd;
                out_bd = BitDepth::F32;
            }
        }
        if num_saved_ops == 0 {
            // When there are no ops, save an identity matrix.
            let mat = OpData::Matrix(MatrixOpData::new());

            let op_writer = MatrixWriter {
                version,
                matrix: &mat,
                in_bit_depth: BitDepth::F32,
                out_bit_depth: BitDepth::F32,
            };
            op_writer.write(formatter)?;
        }
        Ok(())
    }
}

impl XmlElementWriter for TransformWriter<'_> {
    /// Port of `TransformWriter::write` (CTFTransform.cpp:2531-2610 @ v2.5.2).
    fn write(&self, formatter: &mut XmlFormatter<'_>) -> Result<()> {
        let process_list_tag = TAG_PROCESS_LIST;

        let mut attributes = Attributes::new();

        let mut write_version = CTF_PROCESS_LIST_VERSION_2_0;

        if self.is_clf {
            // Save with CLF version 3.
            attributes.push((ATTR_COMP_CLF_VERSION.to_vec(), b"3".to_vec()));
        } else {
            write_version = get_minimum_version(self.transform)?;
            attributes.push((
                ATTR_VERSION.to_vec(),
                write_version.to_string().into_bytes(),
            ));
        }

        let mut id = self.transform.get_id().to_vec();
        if id.is_empty() {
            for op in self.transform.get_ops() {
                id.extend_from_slice(&op.get_cache_id()?);
            }

            id = cache_id_hash(&id).into_bytes();
        }
        attributes.push((ATTR_ID.to_vec(), id));

        let name = self.transform.get_name();
        if !name.is_empty() {
            attributes.push((ATTR_NAME.to_vec(), name.to_vec()));
        }

        let inverse_of_id = self.transform.get_inverse_of_id();
        if !inverse_of_id.is_empty() {
            attributes.push((ATTR_INVERSE_OF.to_vec(), inverse_of_id.to_vec()));
        }

        formatter.write_start_tag_with(process_list_tag, &attributes);
        {
            let mut scope_indent = XmlScopeIndent::new(formatter);

            write_descriptions(
                &mut scope_indent,
                TAG_DESCRIPTION,
                self.transform.get_descriptions(),
            );

            let input_desc = self.transform.get_input_descriptor();
            if !input_desc.is_empty() {
                scope_indent.write_content_tag(METADATA_INPUT_DESCRIPTOR, input_desc);
            }

            let output_desc = self.transform.get_output_descriptor();
            if !output_desc.is_empty() {
                scope_indent.write_content_tag(METADATA_OUTPUT_DESCRIPTOR, output_desc);
            }

            let info = self.transform.get_info_metadata();
            Self::write_process_list_metadata(&mut scope_indent, info);

            self.write_ops(&mut scope_indent, write_version)?;
        }

        formatter.write_end_tag(process_list_tag);
        Ok(())
    }
}
