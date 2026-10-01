// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPU processor: a port of `src/OpenColorIO/CPUProcessor.cpp` @ v2.5.2.
//!
//! - the generic bit-depth conversions, `BitDepthCast` and `CreateGenericBitDepthHelper`,
//!   which image packing and the scanline helper call (chunks 1.1d, 1.1e);
//! - [`CpuProcessor`]: the processor's ops and their renderers (`FinalizeOpsForCPU`,
//!   `CreateCPUEngine`), its cache ID and its queries (chunk 1.2d). Its `apply` methods, with
//!   `CreateScanlineHelper`, come next.

use std::fmt;
use std::hint::black_box;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::bit_depth_utils::{
    BitDepthInfo, ChannelType, Converter, F16, F32, Uint8, Uint10, Uint12, Uint16,
};
use crate::dynamic_property::DynamicPropertyRcPtr;
use crate::exception::{Exception, Result};
use crate::op::{CpuOp, OpVec, Pixels, PixelsMut};
use crate::op_data::OpData;
use crate::open_color_types::{
    BitDepth, DynamicPropertyType, OptimizationFlags, bit_depth_to_string,
};
use crate::ops::matrix::matrix_op::create_identity_matrix_op;

/// The conversion of RGBA pixels from bit depth `I` to bit depth `O`: each value is scaled by
/// `maxValue(O) / maxValue(I)`, then cast (`Converter<O>::CastValue`). From F32 to F32 it only
/// copies, as upstream specializes it.
///
/// Port of `BitDepthCast<inBD, outBD>` and `BitDepthCast<BIT_DEPTH_F32, BIT_DEPTH_F32>`
/// (src/OpenColorIO/CPUProcessor.cpp:20-66 @ v2.5.2).
pub struct BitDepthCast<I, O> {
    /// `m_scale = float(BitDepthInfo<outBD>::maxValue) / float(BitDepthInfo<inBD>::maxValue)`.
    scale: f32,
    bit_depths: PhantomData<fn() -> (I, O)>,
}

impl<I: BitDepthInfo, O: BitDepthInfo> fmt::Debug for BitDepthCast<I, O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BitDepthCast")
            .field("in", &I::BIT_DEPTH)
            .field("out", &O::BIT_DEPTH)
            .field("scale", &self.scale)
            .finish()
    }
}

impl<I: BitDepthInfo, O: Converter> BitDepthCast<I, O> {
    /// The conversion from `I` to `O`.
    pub fn new() -> Self {
        BitDepthCast {
            scale: O::MAX_VALUE as f32 / I::MAX_VALUE as f32,
            bit_depths: PhantomData,
        }
    }

    /// Whether this is the F32 to F32 specialization, which copies.
    fn copies() -> bool {
        I::BIT_DEPTH == BitDepth::F32 && O::BIT_DEPTH == BitDepth::F32
    }
}

impl<I: BitDepthInfo, O: Converter> Default for BitDepthCast<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: BitDepthInfo + 'static, O: Converter + 'static> CpuOp for BitDepthCast<I, O> {
    /// In place, `inImg == outImg`: only the F32 to F32 conversion runs so, as the last op of an
    /// F32 image, and it leaves the pixels alone (its `memcpy` needs separate buffers).
    fn apply(&self, _rgba: &mut [f32]) {
        assert!(
            Self::copies(),
            "{self:?} converts between channel types; it can't work in place"
        );
    }

    fn apply_bit_depth(&self, input: Pixels<'_>, output: PixelsMut<'_>) {
        let (in_name, out_name) = (input.type_name(), output.type_name());

        if Self::copies() {
            // BitDepthCast<BIT_DEPTH_F32, BIT_DEPTH_F32>: a memcpy, as the buffers differ.
            let (Pixels::F32(input), PixelsMut::F32(output)) = (input, output) else {
                panic!("{self:?} got {in_name} to {out_name}");
            };
            output.copy_from_slice(input);
            return;
        }

        let (Some(input), Some(output)) = (
            I::Type::from_pixels(input),
            O::Type::from_pixels_mut(output),
        ) else {
            panic!("{self:?} got {in_name} to {out_name}");
        };
        assert_eq!(
            input.len(),
            output.len(),
            "{self:?}: the pixel counts differ"
        );

        // C++ multiplies by `m_scale`, a member it reads at run time, so a scale of 1 still
        // runs the multiply and quiets signaling NaNs; `black_box` keeps LLVM from folding it
        // (spike S4, docs/spikes/s4.md).
        let scale = black_box(self.scale);
        for (out, &value) in output.iter_mut().zip(input) {
            *out = O::cast_value(value.to_float() * scale);
        }
    }
}

/// The conversion between two bit depths, for the ends of a CPU processor's chain that no op
/// absorbs: "Unsupported bit-depth" for the bit depths the CPU processor doesn't take.
///
/// Port of `CreateGenericBitDepthHelper` (src/OpenColorIO/CPUProcessor.cpp:68-116 @ v2.5.2).
pub fn create_generic_bit_depth_helper(
    input: BitDepth,
    output: BitDepth,
) -> Result<Arc<dyn CpuOp>> {
    let unsupported = || Exception::new("Unsupported bit-depth");

    macro_rules! to_output {
        ($in:ty) => {
            match output {
                BitDepth::Uint8 => Arc::new(BitDepthCast::<$in, Uint8>::new()) as Arc<dyn CpuOp>,
                BitDepth::Uint10 => Arc::new(BitDepthCast::<$in, Uint10>::new()),
                BitDepth::Uint12 => Arc::new(BitDepthCast::<$in, Uint12>::new()),
                BitDepth::Uint16 => Arc::new(BitDepthCast::<$in, Uint16>::new()),
                BitDepth::F16 => Arc::new(BitDepthCast::<$in, F16>::new()),
                BitDepth::F32 => Arc::new(BitDepthCast::<$in, F32>::new()),
                BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => {
                    return Err(unsupported());
                }
            }
        };
    }

    Ok(match input {
        BitDepth::Uint8 => to_output!(Uint8),
        BitDepth::Uint10 => to_output!(Uint10),
        BitDepth::Uint12 => to_output!(Uint12),
        BitDepth::Uint16 => to_output!(Uint16),
        BitDepth::F16 => to_output!(F16),
        BitDepth::F32 => to_output!(F32),
        BitDepth::Uint14 | BitDepth::Uint32 | BitDepth::Unknown => return Err(unsupported()),
    })
}

/// The ops a CPU processor renders: a copy of the processor's ops, finalized, optimized for
/// `o_flags` and the bit depths, and an identity matrix op if none is left, "as the input and
/// output buffers could be different". Their dynamic properties are then checked, unless the
/// flags made them static.
///
/// Port of `FinalizeOpsForCPU` (src/OpenColorIO/CPUProcessor.cpp:311-338 @ v2.5.2).
fn finalize_ops_for_cpu(
    raw_ops: &OpVec,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
    o_flags: OptimizationFlags,
) -> Result<OpVec> {
    let mut ops = raw_ops.clone();

    if !ops.is_empty() {
        // Finalize of all ops.
        ops.finalize()?;

        // Optimize the ops.
        ops.optimize(o_flags)?;
        ops.optimize_for_bitdepth(in_bit_depth, out_bit_depth, o_flags)?;
    }

    // The previous code could change the list of ops so an explicit check to empty is still
    // needed.
    if ops.is_empty() {
        // Needs at least one op (even an identity one) as the input and output buffers could
        // be different.
        create_identity_matrix_op(&mut ops);
    }

    if (o_flags & OptimizationFlags::NO_DYNAMIC_PROPERTIES)
        != OptimizationFlags::NO_DYNAMIC_PROPERTIES
    {
        ops.validate_dynamic_properties()?;
    }
    Ok(ops)
}

/// The renderers of a CPU processor: the first converts from the input bit depth, the last to
/// the output bit depth, and the others are in between.
struct CpuEngine {
    /// The bit-depth "cast", or the first op's renderer.
    in_bit_depth_op: Arc<dyn CpuOp>,
    /// The remaining renderers.
    cpu_ops: Vec<Arc<dyn CpuOp>>,
    /// The bit-depth "cast", or the last op's renderer.
    out_bit_depth_op: Arc<dyn CpuOp>,
}

/// The renderer of `op`. The optimizer has removed the no-op types, the only ops without one.
fn renderer(op: &crate::op::Op, fast_log_exp_pow: bool) -> Result<Arc<dyn CpuOp>> {
    Ok(op
        .get_cpu_op(fast_log_exp_pow)?
        .expect("the optimizer removed the no-op types"))
}

/// The renderers of `ops` for images of the bit depths `in` and `out`: an F32 end is rendered
/// by its op, any other by a bit-depth conversion next to the op's renderer. A Lut1D at either
/// end renders the conversion itself, which comes with its variant.
///
/// Port of `CreateCPUEngine` (src/OpenColorIO/CPUProcessor.cpp:122-184 @ v2.5.2).
fn create_cpu_engine(
    ops: &OpVec,
    in_bit_depth: BitDepth,
    out_bit_depth: BitDepth,
    o_flags: OptimizationFlags,
) -> Result<CpuEngine> {
    let max_ops = ops.len();
    let fast_log_exp_pow = o_flags.has_flag(OptimizationFlags::FAST_LOG_EXP_POW);
    let mut in_bit_depth_op = None;
    let mut cpu_ops = Vec::new();
    let mut out_bit_depth_op = None;
    for (idx, op) in ops.iter().enumerate() {
        // (A Lut1D at either end: `GetLut1DRenderer` converts the bit depths.)
        match &**op.data() {
            OpData::Matrix(_) | OpData::Reference(_) | OpData::NoOp(_) => {}
        }

        if idx == 0 {
            if in_bit_depth == BitDepth::F32 {
                in_bit_depth_op = Some(renderer(op, fast_log_exp_pow)?);
            } else {
                in_bit_depth_op = Some(create_generic_bit_depth_helper(
                    in_bit_depth,
                    BitDepth::F32,
                )?);
                cpu_ops.push(renderer(op, fast_log_exp_pow)?);
            }

            if max_ops == 1 {
                out_bit_depth_op = Some(create_generic_bit_depth_helper(
                    BitDepth::F32,
                    out_bit_depth,
                )?);
            }
        } else if idx == max_ops - 1 {
            if out_bit_depth == BitDepth::F32 {
                out_bit_depth_op = Some(renderer(op, fast_log_exp_pow)?);
            } else {
                out_bit_depth_op = Some(create_generic_bit_depth_helper(
                    BitDepth::F32,
                    out_bit_depth,
                )?);
                cpu_ops.push(renderer(op, fast_log_exp_pow)?);
            }
        } else {
            cpu_ops.push(renderer(op, fast_log_exp_pow)?);
        }
    }
    Ok(CpuEngine {
        in_bit_depth_op: in_bit_depth_op.expect("a CPU processor has an op"),
        cpu_ops,
        out_bit_depth_op: out_bit_depth_op.expect("a CPU processor has an op"),
    })
}

/// A CPU processor: renders images with a processor's ops, optimized for its bit depths and
/// optimization flags.
///
/// Port of `CPUProcessor` and `CPUProcessor::Impl` (src/OpenColorIO/CPUProcessor.h:18-73,
/// CPUProcessor.cpp:242-556 @ v2.5.2). Upstream builds an empty `Impl`, then calls
/// `finalize`; here [`CpuProcessor::new`] does both, under no lock, as nothing shares the
/// processor before it exists.
pub struct CpuProcessor {
    /// `m_inBitDepthOp`, `m_cpuOps` and `m_outBitDepthOp`.
    engine: CpuEngine,
    /// `m_inBitDepth`.
    in_bit_depth: BitDepth,
    /// `m_outBitDepth`.
    out_bit_depth: BitDepth,
    /// `m_isNoOp`.
    is_no_op: bool,
    /// `m_isIdentity`.
    is_identity: bool,
    /// `m_hasChannelCrosstalk`.
    has_channel_crosstalk: bool,
    /// `m_cacheID`.
    cache_id: Vec<u8>,
}

impl fmt::Debug for CpuProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpuProcessor")
            .field("cache_id", &String::from_utf8_lossy(&self.cache_id))
            .finish_non_exhaustive()
    }
}

impl CpuProcessor {
    /// The CPU processor of `raw_ops`, a processor's ops, for images of the bit depths `in` and
    /// `out`, optimized as `o_flags` allow.
    ///
    /// Port of `CPUProcessor::Impl::finalize` (src/OpenColorIO/CPUProcessor.cpp:341-377 @
    /// v2.5.2).
    pub fn new(
        raw_ops: &OpVec,
        in_bit_depth: BitDepth,
        out_bit_depth: BitDepth,
        o_flags: OptimizationFlags,
    ) -> Result<CpuProcessor> {
        // Get the ops of the color transformation without the bit-depth adjustments.

        let ops = finalize_ops_for_cpu(raw_ops, in_bit_depth, out_bit_depth, o_flags)?;

        let is_identity = ops.is_no_op();
        let is_no_op = is_identity && in_bit_depth == out_bit_depth;

        // Does the color processing introduce crosstalk between the pixel channels?
        let has_channel_crosstalk = ops.has_channel_crosstalk();

        // Get the CPU Ops while taking care of the input and output bit-depths.

        let engine = create_cpu_engine(&ops, in_bit_depth, out_bit_depth, o_flags)?;

        // Compute the cache id.

        let mut cache_id = format!(
            "CPU Processor: from {} to {} oFlags {} ops: ",
            bit_depth_to_string(in_bit_depth),
            bit_depth_to_string(out_bit_depth),
            o_flags.0
        )
        .into_bytes();
        cache_id.extend_from_slice(&ops.get_cache_id());

        Ok(CpuProcessor {
            engine,
            in_bit_depth,
            out_bit_depth,
            is_no_op,
            is_identity,
            has_channel_crosstalk,
            cache_id,
        })
    }

    /// Whether the processor leaves every pixel as it is, with equal bit depths.
    ///
    /// Port of `CPUProcessor::isNoOp` (src/OpenColorIO/CPUProcessor.h:27,
    /// CPUProcessor.cpp:491-494 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_no_op
    }

    /// Whether the ops are no-ops, whatever the bit depths.
    ///
    /// Port of `CPUProcessor::isIdentity` (src/OpenColorIO/CPUProcessor.h:31,
    /// CPUProcessor.cpp:496-499 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        self.is_identity
    }

    /// Whether an output channel depends on other input channels.
    ///
    /// Port of `CPUProcessor::hasChannelCrosstalk` (src/OpenColorIO/CPUProcessor.h:33,
    /// CPUProcessor.cpp:501-504 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        self.has_channel_crosstalk
    }

    /// `CPU Processor: from <in> to <out> oFlags <flags> ops: ` and the ops' cache IDs.
    ///
    /// Port of `CPUProcessor::getCacheID` (src/OpenColorIO/CPUProcessor.h:35,
    /// CPUProcessor.cpp:506-509 @ v2.5.2).
    pub fn get_cache_id(&self) -> &[u8] {
        &self.cache_id
    }

    /// Port of `CPUProcessor::getInputBitDepth` (src/OpenColorIO/CPUProcessor.h:37,
    /// CPUProcessor.cpp:511-514 @ v2.5.2).
    pub fn get_input_bit_depth(&self) -> BitDepth {
        self.in_bit_depth
    }

    /// Port of `CPUProcessor::getOutputBitDepth` (src/OpenColorIO/CPUProcessor.h:38,
    /// CPUProcessor.cpp:516-519 @ v2.5.2).
    pub fn get_output_bit_depth(&self) -> BitDepth {
        self.out_bit_depth
    }

    /// The renderers in their order: the input conversion, the others, the output conversion.
    fn renderers(&self) -> impl Iterator<Item = &Arc<dyn CpuOp>> {
        std::iter::once(&self.engine.in_bit_depth_op)
            .chain(&self.engine.cpu_ops)
            .chain(std::iter::once(&self.engine.out_bit_depth_op))
    }

    /// Whether a renderer has a dynamic property that is dynamic.
    ///
    /// Port of `CPUProcessor::Impl::isDynamic` (src/OpenColorIO/CPUProcessor.cpp:242-263 @
    /// v2.5.2).
    pub fn is_dynamic(&self) -> bool {
        self.renderers().any(|op| op.is_dynamic())
    }

    /// Whether a renderer has a dynamic property of the type.
    ///
    /// Port of `CPUProcessor::Impl::hasDynamicProperty` (src/OpenColorIO/CPUProcessor.cpp:
    /// 265-286 @ v2.5.2).
    pub fn has_dynamic_property(&self, property_type: DynamicPropertyType) -> bool {
        self.renderers()
            .any(|op| op.has_dynamic_property(property_type))
    }

    /// The first renderer's dynamic property of the type: "Cannot find dynamic property; not
    /// used by CPU processor." if none has one.
    ///
    /// Port of `CPUProcessor::Impl::getDynamicProperty` (src/OpenColorIO/CPUProcessor.cpp:
    /// 288-309 @ v2.5.2).
    pub fn get_dynamic_property(
        &self,
        property_type: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        for op in self.renderers() {
            if op.has_dynamic_property(property_type) {
                return op.get_dynamic_property(property_type);
            }
        }
        Err(Exception::new(
            "Cannot find dynamic property; not used by CPU processor.",
        ))
    }
}

#[cfg(test)]
#[path = "cpu_processor_tests.rs"]
mod tests;
