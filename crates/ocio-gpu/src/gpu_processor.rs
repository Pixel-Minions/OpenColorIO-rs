// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GPUProcessor.h` and `GPUProcessor.cpp` @ v2.5.2:
//! [`GpuProcessor`], whose construction finalizes and optimizes a processor's ops and makes
//! the cache ID, and whose extraction asks each op for its code, then writes the function
//! header and footer around it ([`write_shader_header`], [`write_shader_footer`]) and
//! finalizes the description.
//!
//! Each op's code comes from its family's GPU writer, chosen by the variant of the op's data
//! ([`extract_op_gpu_shader_info`]): upstream's virtual `Op::extractGpuShaderInfo`, which
//! lives here since `ocio-ops` doesn't know the GPU side.
//!
//! The resource key of the `GpuShaderCreator` overload of `extractGpuShaderInfo`
//! (GPUProcessor.cpp:157-206) reaches only the creator's `begin`, which does nothing in the
//! one description there is ([`GpuShaderDesc::begin`]), so it changes no output; Python's
//! overload (lines 151-155) doesn't compute it. It isn't ported (docs/improvements.md, U-6).

use ocio_ops::op::{Op, OpVec};
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::{Exception, Result};

use crate::gpu_shader_desc::GpuShaderDesc;
use crate::gpu_shader_utils::GpuShaderText;
use crate::open_color_types::GpuLanguage;
use crate::ops::cdl::cdl_op_gpu::get_cdl_gpu_shader_program;
use crate::ops::exponent::exponent_op_gpu::get_exponent_gpu_shader_program;
use crate::ops::fixedfunction::fixed_function_op_gpu::get_fixed_function_gpu_shader_program;
use crate::ops::gamma::gamma_op_gpu::get_gamma_gpu_shader_program;
use crate::ops::log::log_op_gpu::get_log_gpu_shader_program;
use crate::ops::matrix::matrix_op_gpu::get_matrix_gpu_shader_program;
use crate::ops::range::range_op_gpu::get_range_gpu_shader_program;

/// Adds the OCIO function's header to the description: its signature, its opening brace, and
/// the pixel variable set to the input. An empty pixel name is an error ("GPU variable name
/// is empty."), but in OSL, which declares the pixel as a `color4` without that check.
///
/// Port of `WriteShaderHeader` (GPUProcessor.cpp:27-54 @ v2.5.2).
pub fn write_shader_header(shader_creator: &mut GpuShaderDesc) -> Result<()> {
    let fcn_name = shader_creator.function_name().to_vec();

    let ss = GpuShaderText::new(shader_creator.language());

    ss.new_line();
    ss.new_line()
        .put("// Declaration of the OCIO shader function");
    ss.new_line();

    if shader_creator.language() == GpuLanguage::Osl1 {
        ss.new_line()
            .put("color4 ")
            .put(&fcn_name)
            .put("(color4 inPixel)");
        ss.new_line().put("{");
        ss.indent();
        ss.new_line()
            .put("color4 ")
            .put(shader_creator.pixel_name())
            .put(" = inPixel;");
    } else {
        ss.new_line()
            .put(ss.float4_keyword())
            .put(" ")
            .put(&fcn_name)
            .put("(")
            .put(ss.float4_keyword())
            .put(" inPixel)");
        ss.new_line().put("{");
        ss.indent();
        let decl = ss.float4_decl(shader_creator.pixel_name())?;
        ss.new_line().put(decl).put(" = inPixel;");
    }

    shader_creator.add_to_function_header_shader_code(ss.string());
    Ok(())
}

/// Adds the OCIO function's footer to the description: it returns the pixel, and closes.
///
/// Port of `WriteShaderFooter` (GPUProcessor.cpp:57-68 @ v2.5.2).
pub fn write_shader_footer(shader_creator: &mut GpuShaderDesc) {
    let ss = GpuShaderText::new(shader_creator.language());

    ss.new_line();
    ss.indent();
    ss.new_line()
        .put("return ")
        .put(shader_creator.pixel_name())
        .put(";");
    ss.dedent();
    ss.new_line().put("}");

    shader_creator.add_to_function_footer_shader_code(ss.string());
}

/// Adds `op`'s shader code, uniforms and textures to `shader_creator`, with its family's GPU
/// writer.
///
/// The no-ops write nothing (`AllocationNoOp`, `FileNoOp` and `LookNoOp::extractGpuShaderInfo`,
/// src/OpenColorIO/ops/noop/NoOps.cpp:54, 325, 411 @ v2.5.2). A Matrix op still inverse
/// wasn't finalized: upstream refuses it ("Op::finalize has to be called.",
/// `MatrixOffsetOp::extractGpuShaderInfo`, src/OpenColorIO/ops/matrix/MatrixOp.cpp:190-198),
/// and so is a Range op (`RangeOp::extractGpuShaderInfo`,
/// src/OpenColorIO/ops/range/RangeOp.cpp:202-210).
/// A family whose GPU writer isn't ported yet is the port's error, naming the op.
///
/// Port of `Op::extractGpuShaderInfo`, pure virtual (src/OpenColorIO/Op.h:251 @ v2.5.2), and
/// its overrides.
pub(crate) fn extract_op_gpu_shader_info(
    op: &Op,
    shader_creator: &mut GpuShaderDesc,
) -> Result<()> {
    match &**op.data() {
        OpData::Matrix(data) => {
            if data.get_direction() == TransformDirection::Inverse {
                return Err(Exception::new("Op::finalize has to be called."));
            }
            get_matrix_gpu_shader_program(shader_creator, data)
        }
        OpData::Range(data) => {
            if data.get_direction() == TransformDirection::Inverse {
                return Err(Exception::new("Op::finalize has to be called."));
            }
            get_range_gpu_shader_program(shader_creator, data)
        }
        OpData::Exponent(data) => get_exponent_gpu_shader_program(shader_creator, data),
        OpData::Cdl(data) => get_cdl_gpu_shader_program(shader_creator, data),
        OpData::Gamma(data) => get_gamma_gpu_shader_program(shader_creator, data),
        OpData::Log(data) => get_log_gpu_shader_program(shader_creator, data),
        OpData::FixedFunction(data) => get_fixed_function_gpu_shader_program(shader_creator, data),
        OpData::NoOp(_) => Ok(()),
        OpData::Reference(_) => unreachable!("an op never holds a ReferenceOpData"),
        // The families whose GPU writer comes later.
        #[allow(unreachable_patterns)]
        _ => Err(not_on_the_gpu_yet(op)),
    }
}

/// The port's error for an op whose family's GPU writer isn't ported yet.
fn not_on_the_gpu_yet(op: &Op) -> Exception {
    Exception::new(format!(
        "The GPU writer of {} is not ported yet.",
        op.get_info()
    ))
}

/// A GPU processor: a processor's ops, finalized and optimized, which write a shader program
/// into a [`GpuShaderDesc`].
///
/// Port of `GPUProcessor` and `GPUProcessor::Impl` (src/OpenColorIO/GPUProcessor.h,
/// GPUProcessor.cpp:71-206 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GpuProcessor {
    /// The ops, after `finalize` and `optimize`.
    ops: OpVec,
    is_no_op: bool,
    has_channel_crosstalk: bool,
    cache_id: Vec<u8>,
}

impl GpuProcessor {
    /// The GPU processor of `raw_ops`, a processor's ops, optimized as `o_flags` allow. Unlike
    /// the CPU processor's, the list may end up empty, and the dynamic properties are always
    /// checked.
    ///
    /// The processor that makes it (`Processor::Impl::getGPUProcessor`,
    /// src/OpenColorIO/Processor.cpp:491-523 @ v2.5.2) first applies the `OCIO_OPTIMIZATION_FLAGS`
    /// override to the flags, and keeps a cache; those come with the processor (1.8g).
    ///
    /// Port of `GPUProcessor::Impl::finalize` (GPUProcessor.cpp:73-98 @ v2.5.2).
    pub fn new(raw_ops: &OpVec, o_flags: OptimizationFlags) -> Result<GpuProcessor> {
        // Prepare the list of ops.
        let mut ops = raw_ops.clone();
        ops.finalize()?;
        ops.optimize(o_flags)?;
        ops.validate_dynamic_properties()?;

        // Is NoOp ?
        let is_no_op = ops.is_no_op()?;

        // Does the color processing introduce crosstalk between the pixel channels?
        let has_channel_crosstalk = ops.has_channel_crosstalk();

        // Calculate and assemble the GPU cache ID from the ops.
        let mut cache_id = format!("GPU Processor: oFlags {} ops : ", o_flags.0).into_bytes();
        cache_id.extend_from_slice(&ops.get_cache_id()?);

        Ok(GpuProcessor {
            ops,
            is_no_op,
            has_channel_crosstalk,
            cache_id,
        })
    }

    /// Whether the processor leaves every pixel as it is.
    ///
    /// Port of `GPUProcessor::isNoOp` (GPUProcessor.cpp:136-139 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        self.is_no_op
    }

    /// Whether an output channel depends on another input channel.
    ///
    /// Port of `GPUProcessor::hasChannelCrosstalk` (GPUProcessor.cpp:141-144 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        self.has_channel_crosstalk
    }

    /// `GPU Processor: oFlags <flags> ops : ` and the ops' cache IDs.
    ///
    /// Port of `GPUProcessor::getCacheID` (GPUProcessor.cpp:146-149 @ v2.5.2).
    pub fn get_cache_id(&self) -> &[u8] {
        &self.cache_id
    }

    /// The ops, finalized and optimized.
    pub fn ops(&self) -> &OpVec {
        &self.ops
    }

    /// Writes the shader program into `shader_desc`: each op's code, then the function's
    /// header and footer; then finalizes the description. An op's error stops it, with the
    /// description left as the ops before it filled it.
    ///
    /// The `GpuShaderCreator` overload also makes a resource key from the cache IDs and calls
    /// the creator's `begin` and `end` around the extraction (GPUProcessor.cpp:157-204); for a
    /// `GpuShaderDesc` they do nothing, so it changes no output (see the module notes).
    ///
    /// Port of `GPUProcessor::extractGpuShaderInfo(GpuShaderDescRcPtr &)` and
    /// `GPUProcessor::Impl::extractGpuShaderInfo` (GPUProcessor.cpp:151-155, 100-115 @
    /// v2.5.2).
    pub fn extract_gpu_shader_info(&self, shader_desc: &mut GpuShaderDesc) -> Result<()> {
        // Create the shader program information.
        for op in self.ops.iter() {
            extract_op_gpu_shader_info(op, shader_desc)?;
        }

        write_shader_header(shader_desc)?;
        write_shader_footer(shader_desc);

        shader_desc.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ocio_ops::ops::matrix::MatrixOpData;
    use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
    use ocio_ops::ops::range::range_op::create_range_op_from_values;

    /// A Matrix or Range op that is still inverse, which only a list that wasn't finalized
    /// holds (`GpuProcessor::new` finalizes its own copy), is refused with upstream's message
    /// (`MatrixOffsetOp::extractGpuShaderInfo`, src/OpenColorIO/ops/matrix/MatrixOp.cpp:190-198;
    /// `RangeOp::extractGpuShaderInfo`, src/OpenColorIO/ops/range/RangeOp.cpp:202-210
    /// @ v2.5.2); once finalized, both write their code.
    #[test]
    fn unfinalized_inverse_ops_are_refused() {
        let mut ops = OpVec::new();
        create_range_op_from_values(&mut ops, 0.1, 1.1, 0.5, 1.5, TransformDirection::Inverse)
            .unwrap();
        let mut matrix = MatrixOpData::new();
        matrix.set_rgba(&[
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        matrix.validate().unwrap();
        create_matrix_op(&mut ops, matrix, TransformDirection::Inverse);
        assert_eq!(ops.len(), 2);
        for op in ops.iter() {
            let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
            let err = extract_op_gpu_shader_info(op, &mut desc).unwrap_err();
            assert_eq!(err.message(), "Op::finalize has to be called.", "{op}");
        }
        ops.finalize().unwrap();
        for op in ops.iter() {
            let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
            assert!(extract_op_gpu_shader_info(op, &mut desc).is_ok(), "{op}");
        }
    }
}
