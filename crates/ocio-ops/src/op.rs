// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The op model: a port of `src/OpenColorIO/Op.h` and `Op.cpp` @ v2.5.2.
//!
//! - [`CpuOp`]: a CPU renderer (`OpCPU`). [`Pixels`] and [`PixelsMut`] carry the pixels of
//!   the renderers at the ends of a CPU processor, which convert bit depths.
//! - [`Op`]: an op, which is its data and the behaviors of its type (`Op` and its
//!   subclasses). The data is in [`crate::op_data`].
//! - [`OpVec`]: a list of ops (`OpRcPtrVec`), with [`serialize_op_vec`] and
//!   [`create_op_vec_from_op_data`]. Its `finalize` is in [`crate::op_optimizers`], as
//!   upstream's is in `OpOptimizers.cpp`.
//!
//! Not yet ported: `Op::dumpMetadata`, which needs the processor's metadata (WP 1.8),
//! and `OpRcPtrVec::optimize` and `optimizeForBitdepth` (the optimizer, chunk 1.2d). `HasFlag`
//! is [`OptimizationFlags::has_flag`](crate::open_color_types::OptimizationFlags::has_flag).
//! The GPU side, `extractGpuShaderInfo`, is `ocio-gpu`'s.

use std::fmt::{self, Debug};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::format_metadata::FormatMetadataImpl;
use crate::logging::log_warning;
use crate::op_data::{OpData, OpDataRcPtr, OpDataType, OpDataVec, get_type_name};
use crate::open_color_types::{DynamicPropertyType, TransformDirection};
use crate::ops::cdl::cdl_op::create_cdl_op;
use crate::ops::exponent::exponent_op::create_exponent_op;
use crate::ops::gamma::gamma_op::create_gamma_op;
use crate::ops::matrix::matrix_op::create_matrix_op;
use crate::ops::range::range_op::create_range_op;

/// RGBA pixels in one of the CPU processor's channel types (`BitDepthInfo<BD>::Type`): the
/// input of an op that converts from an image's bit depth.
#[derive(Debug, Clone, Copy)]
pub enum Pixels<'a> {
    /// `uint8_t`: 8-bit images.
    U8(&'a [u8]),
    /// `uint16_t`: 10-, 12- and 16-bit images.
    U16(&'a [u16]),
    /// `half`: F16 images.
    F16(&'a [half::f16]),
    /// `float`: F32 images.
    F32(&'a [f32]),
}

/// RGBA pixels in one of the CPU processor's channel types, to write: the output of an op that
/// converts to an image's bit depth.
#[derive(Debug)]
pub enum PixelsMut<'a> {
    /// `uint8_t`: 8-bit images.
    U8(&'a mut [u8]),
    /// `uint16_t`: 10-, 12- and 16-bit images.
    U16(&'a mut [u16]),
    /// `half`: F16 images.
    F16(&'a mut [half::f16]),
    /// `float`: F32 images.
    F32(&'a mut [f32]),
}

impl Pixels<'_> {
    /// The channel type's name, for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Pixels::U8(_) => "uint8_t",
            Pixels::U16(_) => "uint16_t",
            Pixels::F16(_) => "half",
            Pixels::F32(_) => "float",
        }
    }
}

impl PixelsMut<'_> {
    /// The channel type's name, for messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            PixelsMut::U8(_) => "uint8_t",
            PixelsMut::U16(_) => "uint16_t",
            PixelsMut::F16(_) => "half",
            PixelsMut::F32(_) => "float",
        }
    }
}

/// A CPU renderer: processes RGBA `f32` pixels.
///
/// Upstream's `OpCPU::apply(inImg, outImg, numPixels)` reads `inImg` and writes `outImg`,
/// which may be the same buffer. Every renderer that works on `f32` pixels reads a whole
/// pixel before writing it, so the port applies them in place: `rgba` holds
/// `numPixels * 4` values, and the caller copies the input first when it has separate
/// buffers (`docs/architecture.md`, "CPU renderers and numeric profiles").
///
/// Port of `OpCPU` (src/OpenColorIO/Op.h:27-51, Op.cpp:29-42 @ v2.5.2).
pub trait CpuOp: Send + Sync + Debug {
    /// Port of `OpCPU::apply`, in place. `rgba.len()` must be a multiple of 4.
    fn apply(&self, rgba: &mut [f32]);

    /// Port of `OpCPU::apply(inImg, outImg, numPixels)` where the images' channel types need
    /// not be `float`: the first op of a CPU processor converts from its input bit depth, and
    /// the last one to its output bit depth (`CreateCPUEngine`,
    /// src/OpenColorIO/CPUProcessor.cpp:122-184 @ v2.5.2). `input` and `output` hold the same
    /// number of values, a multiple of 4.
    ///
    /// The default serves the renderers that process `float` only, which the engine uses at an
    /// end of the chain only when that end is F32: it processes a copy of the input in place,
    /// which gives what upstream's separate buffers give, as [`apply`](Self::apply) reads each
    /// pixel before writing it. Any other pair of types is a bug in the caller, and panics.
    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        match (input, output) {
            (Pixels::F32(input), PixelsMut::F32(output)) => {
                output.copy_from_slice(input);
                self.apply(output);
            }
            (input, output) => panic!(
                "{self:?} processes float pixels only, not {} to {}",
                input.type_name(),
                output.type_name()
            ),
        }
    }

    /// Port of `OpCPU::apply(pixel, pixel, 1)` on the one pixel of `CPUProcessor::applyRGB` and
    /// `applyRGBA` (src/OpenColorIO/CPUProcessor.cpp:433-465 @ v2.5.2): the op reads the
    /// pixel's bytes as its input channel type and writes its output channel type over the same
    /// bytes, whatever the bit depths.
    ///
    /// The default serves the renderers that process `float` only: [`apply`](Self::apply) in
    /// place. The bit-depth conversions override it (docs/improvements.md, I-41).
    fn apply_pixel_in_place(&self, pixel: &mut [f32; 4]) {
        self.apply(pixel);
    }

    /// Whether the renderer has a dynamic property that is dynamic.
    ///
    /// Port of `OpCPU::isDynamic` (src/OpenColorIO/Op.cpp:29-32 @ v2.5.2): `false` unless the
    /// renderer overrides it.
    fn is_dynamic(&self) -> bool {
        false
    }

    /// Whether the renderer has a dynamic property of the type.
    ///
    /// Port of `OpCPU::hasDynamicProperty` (src/OpenColorIO/Op.cpp:34-37 @ v2.5.2): `false`
    /// unless the renderer overrides it.
    fn has_dynamic_property(&self, _type: DynamicPropertyType) -> bool {
        false
    }

    /// The renderer's dynamic property of the type.
    ///
    /// Port of `OpCPU::getDynamicProperty` (src/OpenColorIO/Op.cpp:39-42 @ v2.5.2): an error
    /// unless the renderer overrides it.
    fn get_dynamic_property(&self, _type: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        Err(Exception::new(NO_DYNAMIC_PROPERTY))
    }
}

/// The error of the base `OpCPU::getDynamicProperty` and `Op::getDynamicProperty`
/// (src/OpenColorIO/Op.cpp:39-42, 173-176 @ v2.5.2).
const NO_DYNAMIC_PROPERTY: &str = "Op does not implement dynamic property.";

/// An op: its data, and the behaviors of its type.
///
/// Upstream has a subclass of `Op` per op type, each holding a shared `OpData`. Here an op is
/// its data: each of `Op`'s virtual methods matches on the data's variant and calls its
/// family's module, and the variants that don't override a method share the base's behavior
/// (`docs/architecture.md`, "Op data and ops").
///
/// `Clone` shares the data, as a copy of upstream's `OpRcPtr` shares the op. The data is never
/// changed in place while shared: an op replaces it ([`finalize`](Self::finalize)) or changes
/// it through [`Arc::make_mut`], which copies shared data first. So a change never reaches
/// another op's data behind its back. Upstream's `clone()`, which copies the data, is
/// [`clone_op`](Self::clone_op).
///
/// An op never holds a `ReferenceOpData`: no op class takes one, and
/// [`create_op_vec_from_op_data`] refuses it.
///
/// Port of `Op` (src/OpenColorIO/Op.h:177-298, Op.cpp:145-212 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Op {
    /// The parameters (LUT values, matrix coefficients, ...): upstream's `m_data`.
    data: OpDataRcPtr,
}

impl Op {
    /// An op holding `data`. The families call it for their own data, as their op classes'
    /// constructors set `m_data`.
    pub(crate) fn new(data: OpData) -> Op {
        Op {
            data: Arc::new(data),
        }
    }

    /// The data, to question it and to cast it to its variant.
    ///
    /// Port of `Op::data() const` (src/OpenColorIO/Op.h:288 @ v2.5.2).
    pub fn data(&self) -> &OpDataRcPtr {
        &self.data
    }

    /// A copy of the op that shares nothing it could change.
    ///
    /// Port of `Op::clone`, pure virtual (src/OpenColorIO/Op.h:186 @ v2.5.2), and its
    /// overrides.
    pub fn clone_op(&self) -> Op {
        match &*self.data {
            OpData::Cdl(data) => data.clone_op(),
            OpData::Gamma(data) => data.clone_op(),
            OpData::Matrix(data) => data.clone_op(),
            // The op's data was validated when the op was made, so the copy is valid too.
            OpData::Range(data) => data.clone_op().expect("an op's range is valid"),
            OpData::Exponent(data) => data.clone_op(),
            OpData::Reference(_) => no_reference_op(),
            OpData::NoOp(data) => data.clone_op(),
        }
    }

    /// Something short and printable, such as `<FileNoOp>`: what one wants to see when
    /// debugging.
    ///
    /// Port of `Op::getInfo`, pure virtual (src/OpenColorIO/Op.h:188-190 @ v2.5.2), and its
    /// overrides.
    pub fn get_info(&self) -> &'static str {
        match &*self.data {
            OpData::Cdl(data) => data.get_info(),
            OpData::Gamma(data) => data.get_info(),
            OpData::Matrix(data) => data.get_info(),
            OpData::Range(data) => data.get_info(),
            OpData::Exponent(data) => data.get_info(),
            OpData::Reference(_) => no_reference_op(),
            OpData::NoOp(data) => data.get_info(),
        }
    }

    /// Whether the op is a no-op type (whose data is `NoOpData`), which the optimizer always
    /// removes.
    ///
    /// Port of `Op::isNoOpType` (src/OpenColorIO/Op.h:192 @ v2.5.2), which no subclass
    /// overrides.
    pub fn is_no_op_type(&self) -> bool {
        self.data.get_type() == OpDataType::NoOp
    }

    /// Whether the op leaves every pixel as it is. It is valid before optimization.
    ///
    /// Port of `Op::isNoOp` (src/OpenColorIO/Op.h:194-198 @ v2.5.2), and its overrides.
    pub fn is_no_op(&self) -> Result<bool> {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default: the data's.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => self.data.is_no_op(),
        }
    }

    /// Whether the op leaves pixels as they are in its intended domain.
    ///
    /// Port of `Op::isIdentity` (src/OpenColorIO/Op.h:200 @ v2.5.2), and its overrides.
    pub fn is_identity(&self) -> Result<bool> {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default: the data's.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => self.data.is_identity(),
        }
    }

    /// The op that replaces this one where the optimizer finds it to be an identity: an
    /// identity Matrix op, which the optimizer then removes, or a clamping Range op, as the
    /// data's [`OpData::get_identity_replacement`] says.
    ///
    /// Port of `Op::getIdentityReplacement` (src/OpenColorIO/Op.cpp:178-202 @ v2.5.2).
    pub fn get_identity_replacement(&self) -> Result<Op> {
        let op_data = self.data.get_identity_replacement();
        let mut ops = OpVec::new();
        match op_data {
            OpData::Matrix(mat) => {
                // No-op that will be optimized.
                create_matrix_op(&mut ops, mat, TransformDirection::Forward);
            }
            OpData::Range(range) => {
                // Clamping op.
                create_range_op(&mut ops, range, TransformDirection::Forward)?;
            }
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Exponent(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => {
                return Err(Exception::new(format!(
                    "Unexpected type in getIdentityReplacement. Expecting Matrix or Range, \
                     got :{}.",
                    get_type_name(op_data.get_type())?
                )));
            }
        }
        Ok(ops[0].clone())
    }

    /// Appends to `ops` the simpler ops that replace this one, if its data has any
    /// ([`OpData::get_simpler_replacement`]).
    ///
    /// Port of `Op::getSimplerReplacement` (src/OpenColorIO/Op.cpp:204-212 @ v2.5.2).
    pub fn get_simpler_replacement(&self, ops: &mut OpVec) -> Result<()> {
        let mut op_data_vec = OpDataVec::new();
        self.data.get_simpler_replacement(&mut op_data_vec)?;
        for op_data in &op_data_vec {
            create_op_vec_from_op_data(ops, op_data, TransformDirection::Forward)?;
        }
        Ok(())
    }

    /// Whether `op` is of the same op class.
    ///
    /// Port of `Op::isSameType`, pure virtual (src/OpenColorIO/Op.h:205 @ v2.5.2), and its
    /// overrides.
    pub fn is_same_type(&self, op: &Op) -> bool {
        match &*self.data {
            OpData::Cdl(data) => data.is_same_type(op),
            OpData::Gamma(data) => data.is_same_type(op),
            OpData::Matrix(data) => data.is_same_type(op),
            OpData::Range(data) => data.is_same_type(op),
            OpData::Exponent(data) => data.is_same_type(op),
            OpData::Reference(_) => no_reference_op(),
            OpData::NoOp(data) => data.is_same_type(op),
        }
    }

    /// Whether `op` undoes this op.
    ///
    /// Port of `Op::isInverse`, pure virtual (src/OpenColorIO/Op.h:207 @ v2.5.2), and its
    /// overrides.
    pub fn is_inverse(&self, op: &Op) -> bool {
        match &*self.data {
            OpData::Cdl(data) => data.is_inverse_op(op),
            OpData::Gamma(data) => data.is_inverse_op(op),
            OpData::Matrix(data) => data.is_inverse(op),
            OpData::Range(data) => data.is_inverse(op),
            OpData::Exponent(data) => data.is_inverse(op),
            OpData::Reference(_) => no_reference_op(),
            OpData::NoOp(data) => data.is_inverse(op),
        }
    }

    /// Whether the op can be combined with `op` into simpler ops. The ops must be validated
    /// and finalized.
    ///
    /// Port of `Op::canCombineWith` (src/OpenColorIO/Op.h:209, Op.cpp:145-148 @ v2.5.2), and
    /// its overrides.
    pub fn can_combine_with(&self, op: &Op) -> Result<bool> {
        match &*self.data {
            OpData::Cdl(data) => Ok(data.can_combine_with(op)),
            OpData::Gamma(data) => Ok(data.can_combine_with(op)),
            OpData::Matrix(data) => data.can_combine_with(op),
            OpData::Range(data) => data.can_combine_with(op),
            OpData::Exponent(data) => Ok(data.can_combine_with(op)),
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::NoOp(_) => Ok(false),
        }
    }

    /// Appends to `ops` the ops that do what this op followed by `second_op` does. `ops` may
    /// stay empty when the result is a no-op.
    ///
    /// Port of `Op::combineWith` (src/OpenColorIO/Op.h:211-217, Op.cpp:150-156 @ v2.5.2), and
    /// its overrides.
    pub fn combine_with(&self, ops: &mut OpVec, second_op: &Op) -> Result<()> {
        match &*self.data {
            OpData::Cdl(data) => data.combine_with(ops, second_op),
            OpData::Gamma(data) => data.combine_with(ops, second_op),
            OpData::Matrix(data) => data.combine_with(ops, second_op),
            OpData::Range(data) => data.combine_with(ops, second_op),
            OpData::Exponent(data) => data.combine_with(ops, second_op),
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::NoOp(_) => Err(self.cannot_combine()),
        }
    }

    /// The error of the base `Op::combineWith` (src/OpenColorIO/Op.cpp:150-156 @ v2.5.2).
    fn cannot_combine(&self) -> Exception {
        Exception::new(format!(
            "Op: {} cannot be combined. A type-specific combining function is not defined.",
            self.get_info()
        ))
    }

    /// Whether the output of a channel depends on the other channels.
    ///
    /// Port of `Op::hasChannelCrosstalk` (src/OpenColorIO/Op.h:219 @ v2.5.2), and its
    /// overrides.
    pub fn has_channel_crosstalk(&self) -> bool {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default: the data's.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => self.data.has_channel_crosstalk(),
        }
    }

    /// Checks the data. A 3x3 matrix (from a CLF or CTF file) becomes its canonical 4x4 form in
    /// the op's data, as upstream's `const` `MatrixArray::validate` makes it through a
    /// `const_cast` (src/OpenColorIO/ops/matrix/MatrixOpData.cpp:413-436 @ v2.5.2): the op's
    /// later queries, renderers and cache ID then work on 4 by 4 values. The data is copied
    /// first if another op shares it ([`Arc::make_mut`]), where upstream changes it for every
    /// op that shares it.
    ///
    /// Port of `Op::validate` (src/OpenColorIO/Op.cpp:158-161 @ v2.5.2).
    pub fn validate(&mut self) -> Result<()> {
        if let OpData::Matrix(mat) = &*self.data
            && mat.get_array().get_length() == 3
        {
            let OpData::Matrix(mat) = Arc::make_mut(&mut self.data) else {
                unreachable!("the data is a matrix");
            };
            return mat.validate();
        }
        self.data.validate()
    }

    /// Prepares the op for optimization and rendering: e.g. a Matrix or Range op becomes
    /// forward.
    ///
    /// Port of `Op::finalize` (src/OpenColorIO/Op.h:226-227 @ v2.5.2), and its overrides.
    pub fn finalize(&mut self) -> Result<()> {
        match &*self.data {
            // An inverse matrix becomes its forward equivalent: new data.
            // Port of `MatrixOffsetOp::finalize` (src/OpenColorIO/ops/matrix/MatrixOp.cpp:
            // 164-171 @ v2.5.2).
            OpData::Matrix(mat) => {
                if mat.get_direction() == TransformDirection::Inverse {
                    self.data = Arc::new(OpData::Matrix(mat.get_as_forward()?));
                }
                Ok(())
            }
            // An inverse range becomes its forward equivalent: new data.
            // Port of `RangeOp::finalize` (src/OpenColorIO/ops/range/RangeOp.cpp:176-183 @
            // v2.5.2).
            OpData::Range(range) => {
                if range.get_direction() == TransformDirection::Inverse {
                    self.data = Arc::new(OpData::Range(range.get_as_forward()?));
                }
                Ok(())
            }
            OpData::Reference(_) => no_reference_op(),
            // The Op default: nothing. (A reverse Gamma style renders as it is.)
            OpData::Cdl(_) | OpData::Gamma(_) | OpData::Exponent(_) | OpData::NoOp(_) => Ok(()),
        }
    }

    /// A text that identifies the op, for the cache IDs of processors.
    ///
    /// Port of `Op::getCacheID`, pure virtual (src/OpenColorIO/Op.h:229-230 @ v2.5.2), and its
    /// overrides.
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        match &*self.data {
            OpData::Cdl(data) => Ok(data.get_op_cache_id()),
            OpData::Gamma(data) => data.get_op_cache_id(),
            OpData::Matrix(data) => data.get_op_cache_id(),
            OpData::Range(data) => Ok(data.get_op_cache_id()),
            OpData::Exponent(data) => Ok(data.get_op_cache_id()),
            OpData::Reference(_) => no_reference_op(),
            OpData::NoOp(data) => Ok(data.get_op_cache_id()),
        }
    }

    /// Renders `rgba` (RGBA F32 pixels) in place, with the renderer that is used without fast
    /// math. It is meant for tests. The op must be finalized.
    ///
    /// Port of `Op::apply(void *, long)` (src/OpenColorIO/Op.h:232-241 @ v2.5.2), and its
    /// overrides.
    pub fn apply(&self, rgba: &mut [f32]) -> Result<()> {
        match &*self.data {
            // The Op default: `getCPUOp(false)->apply(img, img, numPixels)`.
            OpData::Cdl(data) => {
                data.get_cpu_op(false).apply(rgba);
                Ok(())
            }
            OpData::Gamma(data) => {
                data.get_cpu_op(false)?.apply(rgba);
                Ok(())
            }
            OpData::Matrix(data) => {
                data.get_cpu_op()?.apply(rgba);
                Ok(())
            }
            OpData::Range(data) => {
                data.get_cpu_op()?.apply(rgba);
                Ok(())
            }
            OpData::Exponent(data) => {
                data.get_cpu_op().apply(rgba);
                Ok(())
            }
            OpData::Reference(_) => no_reference_op(),
            // AllocationNoOp, FileNoOp and LookNoOp::apply do nothing
            // (src/OpenColorIO/ops/noop/NoOps.cpp:49, 320, 406 @ v2.5.2).
            OpData::NoOp(_) => Ok(()),
        }
    }

    /// Renders `input` (RGBA F32 pixels) into `output`, which has the same length, with the
    /// renderer that is used without fast math. It is meant for tests. The op must be
    /// finalized.
    ///
    /// Port of `Op::apply(const void *, void *, long)` (src/OpenColorIO/Op.h:243-244 @
    /// v2.5.2), and its overrides.
    pub fn apply_in_out(&self, input: &[f32], output: &mut [f32]) -> Result<()> {
        match &*self.data {
            // The Op default: `getCPUOp(false)->apply(inImg, outImg, numPixels)`. The matrix
            // renderers read a pixel before writing it, so they render a copy in place.
            // The CDL renderers read a pixel before writing it too.
            OpData::Cdl(data) => {
                let renderer = data.get_cpu_op(false);
                output.copy_from_slice(input);
                renderer.apply(output);
                Ok(())
            }
            // The gamma renderers read a pixel's four values before writing it too.
            OpData::Gamma(data) => {
                let renderer = data.get_cpu_op(false)?;
                output.copy_from_slice(input);
                renderer.apply(output);
                Ok(())
            }
            OpData::Matrix(data) => {
                let renderer = data.get_cpu_op()?;
                output.copy_from_slice(input);
                renderer.apply(output);
                Ok(())
            }
            // The range renderers read a pixel before writing it too.
            OpData::Range(data) => {
                let renderer = data.get_cpu_op()?;
                output.copy_from_slice(input);
                renderer.apply(output);
                Ok(())
            }
            // The Exponent renderer reads each value before writing it too.
            OpData::Exponent(data) => {
                let renderer = data.get_cpu_op();
                output.copy_from_slice(input);
                renderer.apply(output);
                Ok(())
            }
            OpData::Reference(_) => no_reference_op(),
            // The no-ops copy (src/OpenColorIO/ops/noop/NoOps.cpp:51-52, 322-323, 408-409 @
            // v2.5.2).
            OpData::NoOp(_) => {
                output.copy_from_slice(input);
                Ok(())
            }
        }
    }

    /// Whether the legacy GPU shader path can render the op analytically.
    ///
    /// Port of `Op::supportedByLegacyShader` (src/OpenColorIO/Op.h:247-248 @ v2.5.2), and its
    /// overrides.
    pub fn supported_by_legacy_shader(&self) -> bool {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => true,
        }
    }

    /// Whether the op has a dynamic property that is dynamic.
    ///
    /// Port of `Op::isDynamic` (src/OpenColorIO/Op.cpp:163-166 @ v2.5.2), and its overrides.
    pub fn is_dynamic(&self) -> bool {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => false,
        }
    }

    /// Whether the op has a dynamic property of the type.
    ///
    /// Port of `Op::hasDynamicProperty` (src/OpenColorIO/Op.cpp:168-171 @ v2.5.2), and its
    /// overrides.
    pub fn has_dynamic_property(&self, _type: DynamicPropertyType) -> bool {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => false,
        }
    }

    /// The op's dynamic property of the type.
    ///
    /// Port of `Op::getDynamicProperty` (src/OpenColorIO/Op.cpp:173-176 @ v2.5.2), and its
    /// overrides.
    pub fn get_dynamic_property(&self, _type: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => Err(Exception::new(NO_DYNAMIC_PROPERTY)),
        }
    }

    /// Makes the op use `prop` as its dynamic property of the type, so that several ops
    /// share one.
    ///
    /// Port of the `Op::replaceDynamicProperty` overloads (src/OpenColorIO/Op.h:256-280 @
    /// v2.5.2), one per class of property, and their overrides.
    pub fn replace_dynamic_property(
        &mut self,
        _type: DynamicPropertyType,
        prop: &DynamicPropertyRcPtr,
    ) -> Result<()> {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default: each overload's error.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => Err(cannot_replace(prop)),
        }
    }

    /// Makes the op's dynamic properties non-dynamic.
    ///
    /// Port of `Op::removeDynamicProperties` (src/OpenColorIO/Op.h:282-283 @ v2.5.2), and its
    /// overrides.
    pub fn remove_dynamic_properties(&mut self) {
        match &*self.data {
            OpData::Reference(_) => no_reference_op(),
            // The Op default: nothing.
            OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::NoOp(_) => {}
        }
    }

    /// The op's CPU renderer, chosen for its parameters and, where the op has a fast-math
    /// variant, `fast_log_exp_pow`. `None` for the no-ops, which the optimizer removes before
    /// a CPU processor asks. The op must be finalized.
    ///
    /// Port of `Op::getCPUOp`, pure virtual (src/OpenColorIO/Op.h:285-286 @ v2.5.2), and its
    /// overrides.
    pub fn get_cpu_op(&self, fast_log_exp_pow: bool) -> Result<Option<Arc<dyn CpuOp>>> {
        match &*self.data {
            OpData::Cdl(data) => Ok(Some(data.get_cpu_op(fast_log_exp_pow))),
            OpData::Gamma(data) => Ok(Some(data.get_cpu_op(fast_log_exp_pow)?)),
            OpData::Matrix(data) => Ok(Some(data.get_cpu_op()?)),
            OpData::Range(data) => Ok(Some(data.get_cpu_op()?)),
            OpData::Exponent(data) => Ok(Some(data.get_cpu_op())),
            OpData::Reference(_) => no_reference_op(),
            // AllocationNoOp, FileNoOp and LookNoOp::getCPUOp return nullptr
            // (src/OpenColorIO/ops/noop/NoOps.cpp:47, 318, 404 @ v2.5.2).
            OpData::NoOp(_) => Ok(None),
        }
    }
}

/// The arm of an [`Op`] method for a `ReferenceOpData`, which an op never holds: no op class
/// takes one, and [`create_op_vec_from_op_data`] refuses it.
fn no_reference_op() -> ! {
    unreachable!("an op never holds a ReferenceOpData")
}

/// The error of the base `Op::replaceDynamicProperty` overload for `prop`'s class
/// (src/OpenColorIO/Op.h:256-280 @ v2.5.2).
fn cannot_replace(prop: &DynamicPropertyRcPtr) -> Exception {
    match prop {
        DynamicPropertyRcPtr::Double(_) => {
            Exception::new("Op does not implement double dynamic property.")
        }
    }
}

impl fmt::Display for Op {
    /// The op's [`get_info`](Op::get_info).
    ///
    /// Port of `operator<<(std::ostream &, const Op &)` (src/OpenColorIO/Op.cpp:467-471 @
    /// v2.5.2).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_info())
    }
}

/// A list of ops, with the format metadata of what they came from.
///
/// It dereferences to a slice of its ops, as upstream's class follows the `std::vector` API.
/// The ops are shared: copying the list, inserting and appending share them, as upstream's
/// list of `OpRcPtr` does. [`clone_ops`](Self::clone_ops) and [`invert`](Self::invert) make
/// new ones.
///
/// Port of `OpRcPtrVec` (src/OpenColorIO/Op.h:302-410, Op.cpp:214-465 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct OpVec {
    /// `m_ops`.
    ops: Vec<Op>,
    /// `m_metadata`.
    metadata: FormatMetadataImpl,
}

impl Deref for OpVec {
    type Target = [Op];

    fn deref(&self) -> &[Op] {
        &self.ops
    }
}

impl DerefMut for OpVec {
    fn deref_mut(&mut self) -> &mut [Op] {
        &mut self.ops
    }
}

impl OpVec {
    /// An empty list, with empty metadata.
    ///
    /// Port of `OpRcPtrVec::OpRcPtrVec()` (src/OpenColorIO/Op.cpp:214-217 @ v2.5.2).
    pub fn new() -> OpVec {
        OpVec::default()
    }

    /// Appends the ops of `other`, shared, and combines its metadata into this list's
    /// ([`FormatMetadataImpl::combine`]), which fails if the two metadata elements have
    /// different names; the ops are appended first.
    ///
    /// For `ops += ops`, upstream appends a copy of the list; in Rust the caller passes the
    /// copy.
    ///
    /// Port of `OpRcPtrVec::operator+=` (src/OpenColorIO/Op.cpp:236-249 @ v2.5.2).
    pub fn append(&mut self, other: &OpVec) -> Result<()> {
        self.ops.extend(other.ops.iter().cloned());
        self.metadata.combine(&other.metadata)
    }

    /// Removes the op at `position`.
    ///
    /// Port of `OpRcPtrVec::erase(const_iterator)` (src/OpenColorIO/Op.cpp:251-254 @ v2.5.2).
    pub fn erase(&mut self, position: usize) {
        self.ops.remove(position);
    }

    /// Removes the ops from `first` to `last`, `last` excluded.
    ///
    /// Port of `OpRcPtrVec::erase(const_iterator, const_iterator)` (src/OpenColorIO/Op.cpp:
    /// 256-260 @ v2.5.2).
    pub fn erase_range(&mut self, first: usize, last: usize) {
        self.ops.drain(first..last);
    }

    /// Inserts `ops`, shared and in order, at `position`: the ops from `position` on move
    /// right. Inserting at the end appends.
    ///
    /// Port of `OpRcPtrVec::insert` (src/OpenColorIO/Op.cpp:262-267 @ v2.5.2).
    pub fn insert(&mut self, position: usize, ops: &[Op]) {
        self.ops.splice(position..position, ops.iter().cloned());
    }

    /// Removes every op. The metadata stays.
    ///
    /// Port of `OpRcPtrVec::clear` (src/OpenColorIO/Op.h:359 @ v2.5.2).
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    /// Appends `op`.
    ///
    /// Port of `OpRcPtrVec::push_back` (src/OpenColorIO/Op.cpp:269-272 @ v2.5.2).
    pub fn push_back(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// Port of `OpRcPtrVec::getFormatMetadata() const` (src/OpenColorIO/Op.h:374 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpRcPtrVec::getFormatMetadata()` (src/OpenColorIO/Op.h:373 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Whether every op is a no-op; `true` for an empty list.
    ///
    /// Port of `OpRcPtrVec::isNoOp` (src/OpenColorIO/Op.cpp:284-292 @ v2.5.2).
    pub fn is_no_op(&self) -> Result<bool> {
        for op in &self.ops {
            if !op.is_no_op()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether an op mixes channels.
    ///
    /// Port of `OpRcPtrVec::hasChannelCrosstalk` (src/OpenColorIO/Op.cpp:294-299 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        self.ops.iter().any(Op::has_channel_crosstalk)
    }

    /// Whether an op has a dynamic property that is dynamic.
    ///
    /// Port of `OpRcPtrVec::isDynamic` (src/OpenColorIO/Op.cpp:301-306 @ v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.ops.iter().any(Op::is_dynamic)
    }

    /// Whether an op has a dynamic property of the type.
    ///
    /// Port of `OpRcPtrVec::hasDynamicProperty` (src/OpenColorIO/Op.cpp:308-313 @ v2.5.2).
    pub fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        self.ops.iter().any(|op| op.has_dynamic_property(type_))
    }

    /// The dynamic property of the type of the first op that has one.
    ///
    /// Port of `OpRcPtrVec::getDynamicProperty` (src/OpenColorIO/Op.cpp:315-326 @ v2.5.2).
    pub fn get_dynamic_property(&self, type_: DynamicPropertyType) -> Result<DynamicPropertyRcPtr> {
        for op in &self.ops {
            if op.has_dynamic_property(type_) {
                return op.get_dynamic_property(type_);
            }
        }
        Err(Exception::new("Cannot find dynamic property."))
    }

    /// Logs a warning for each dynamic property of a type after the first: only the first
    /// responds to changes, the others keep their values.
    ///
    /// Port of `OpRcPtrVec::validateDynamicProperties` (src/OpenColorIO/Op.cpp:421-446 @
    /// v2.5.2).
    pub fn validate_dynamic_properties(&self) -> Result<()> {
        // The properties in upstream's order, each with whether it is found yet: upstream's
        // empty shared pointers.
        let mut properties = [
            (DynamicPropertyType::Exposure, false),
            (DynamicPropertyType::Contrast, false),
            (DynamicPropertyType::Gamma, false),
            (DynamicPropertyType::GradingPrimary, false),
            (DynamicPropertyType::GradingRgbCurve, false),
            (DynamicPropertyType::GradingHueCurve, false),
            (DynamicPropertyType::GradingTone, false),
        ];
        for op in &self.ops {
            // Each property can only be there once.
            for (type_, found) in &mut properties {
                validate_dynamic_property(op, found, *type_)?;
            }
        }
        Ok(())
    }

    /// A list of copies of the ops ([`Op::clone_op`]), with empty metadata: upstream doesn't
    /// copy it.
    ///
    /// Port of `OpRcPtrVec::clone` (src/OpenColorIO/Op.cpp:328-338 @ v2.5.2).
    pub fn clone_ops(&self) -> OpVec {
        let mut cloned = OpVec::new();
        for op in &self.ops {
            cloned.push_back(op.clone_op());
        }
        cloned
    }

    /// The ops that undo these, in reverse order, with empty metadata. A no-op type is kept, as
    /// a copy, for the information it carries; each other op becomes the inverse of its data
    /// ([`create_op_vec_from_op_data`]).
    ///
    /// Port of `OpRcPtrVec::invert` (src/OpenColorIO/Op.cpp:340-362 @ v2.5.2).
    pub fn invert(&self) -> Result<OpVec> {
        let mut inverted = OpVec::new();
        for op in self.ops.iter().rev() {
            if op.is_no_op_type() {
                // Keep track of the information.
                inverted.push_back(op.clone_op());
            } else {
                create_op_vec_from_op_data(&mut inverted, op.data(), TransformDirection::Inverse)?;
            }
        }
        Ok(inverted)
    }

    /// Checks each op's data.
    ///
    /// Port of `OpRcPtrVec::validate` (src/OpenColorIO/Op.cpp:364-370 @ v2.5.2).
    pub fn validate(&mut self) -> Result<()> {
        for op in &mut self.ops {
            op.validate()?;
        }
        Ok(())
    }

    /// The ops' cache IDs, each after a space. The no-op types are skipped, and so are empty
    /// cache IDs.
    ///
    /// Port of `OpRcPtrVec::getCacheID` (src/OpenColorIO/Op.cpp:448-465 @ v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        let mut stream = Vec::new();
        for op in &self.ops {
            if !op.is_no_op_type() {
                let id = op.get_cache_id()?;
                if !id.is_empty() {
                    stream.push(b' ');
                    stream.extend_from_slice(&id);
                }
            }
        }
        Ok(stream)
    }
}

/// One op and one type of `OpRcPtrVec::validateDynamicProperties`: the first property of the
/// type is found, and the next ones log a warning. Upstream keeps the first property, cast
/// to its class; `found` is whether the cast gave one.
///
/// Port of `ValidateDynamicProperty` (src/OpenColorIO/Op.cpp:374-418 @ v2.5.2).
fn validate_dynamic_property(op: &Op, found: &mut bool, type_: DynamicPropertyType) -> Result<()> {
    if op.has_dynamic_property(type_) {
        if !*found {
            // Initialize property.
            let dp = op.get_dynamic_property(type_)?;
            *found = is_class_of(&dp, type_);
        } else {
            // If the property is already initialized, it means that it is already being used
            // elsewhere in this OpVec.
            let name = match type_ {
                DynamicPropertyType::Exposure => "Exposure",
                DynamicPropertyType::Contrast => "Contrast",
                DynamicPropertyType::Gamma => "Gamma",
                DynamicPropertyType::GradingPrimary => "Grading primary",
                DynamicPropertyType::GradingRgbCurve => "Grading RGB curve",
                DynamicPropertyType::GradingTone => "Grading tone",
                DynamicPropertyType::GradingHueCurve => "Grading hue curve",
            };
            log_warning(format!("{name} dynamic property can only be there once."));
        }
    }
    Ok(())
}

/// Whether `dp` is of the class that `validateDynamicProperties` casts a property of type
/// `type_` to: `DynamicPropertyDoubleImpl` for exposure, contrast and gamma, and the
/// grading classes for their types (src/OpenColorIO/Op.cpp:427-444 @ v2.5.2).
fn is_class_of(dp: &DynamicPropertyRcPtr, type_: DynamicPropertyType) -> bool {
    match dp {
        DynamicPropertyRcPtr::Double(_) => matches!(
            type_,
            DynamicPropertyType::Exposure
                | DynamicPropertyType::Contrast
                | DynamicPropertyType::Gamma
        ),
    }
}

/// The ops as text, one per line: `Op <index>: <info> <cache ID>`, after `indent` spaces
/// (none when `indent` is 0 or less, as `pystring::mul`).
///
/// Port of `SerializeOpVec` (src/OpenColorIO/Op.cpp:473-489 @ v2.5.2).
pub fn serialize_op_vec(ops: &OpVec, indent: i32) -> Result<Vec<u8>> {
    let mut oss = Vec::new();

    for (idx, op) in ops.iter().enumerate() {
        oss.extend(std::iter::repeat_n(b' ', indent.max(0) as usize));
        oss.extend_from_slice(format!("Op {idx}: {op} ").as_bytes());
        oss.extend_from_slice(&op.get_cache_id()?);

        oss.push(b'\n');
    }

    Ok(oss)
}

/// Appends to `ops` the op that renders `op_data` in the direction `dir`, from a copy of the
/// data. A reference, which a file reader returns for its referenced file, and no-op data
/// are errors.
///
/// Port of `CreateOpVecFromOpData` (src/OpenColorIO/Op.cpp:491-620 @ v2.5.2).
pub fn create_op_vec_from_op_data(
    ops: &mut OpVec,
    op_data: &OpDataRcPtr,
    dir: TransformDirection,
) -> Result<()> {
    match &**op_data {
        OpData::Cdl(cdl_src) => {
            let cdl = cdl_src.clone();
            create_cdl_op(ops, cdl, dir);
            Ok(())
        }
        OpData::Gamma(gamma_src) => {
            let gamma = gamma_src.clone();
            create_gamma_op(ops, gamma, dir);
            Ok(())
        }
        OpData::Matrix(matrix_src) => {
            let matrix = matrix_src.clone();
            create_matrix_op(ops, matrix, dir);
            Ok(())
        }
        OpData::Range(range_src) => {
            let range = range_src.clone();
            create_range_op(ops, range, dir)
        }
        OpData::Exponent(exp_src) => {
            let exp = exp_src.clone();
            create_exponent_op(ops, exp, dir)
        }
        OpData::Reference(_) => Err(Exception::new(
            "ReferenceOpData should have been replaced by referenced ops",
        )),
        OpData::NoOp(_) => Err(Exception::new("OpData is not supported")),
    }
}

#[cfg(test)]
#[path = "op_tests.rs"]
mod tests;
