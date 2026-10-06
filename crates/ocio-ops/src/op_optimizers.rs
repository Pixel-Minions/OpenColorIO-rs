// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The finalization and optimization of op lists: a port of
//! `src/OpenColorIO/OpOptimizers.cpp` @ v2.5.2.
//!
//! The optimizer's core: `OpRcPtrVec::finalize`, `optimize` (the pass loop and its generic
//! steps, which ask the ops only through [`Op`]'s methods) and `optimizeForBitdepth`, with
//! `OptimizeSeparablePrefix`, which bakes a prefix of separable ops into a Lut1D for integer
//! and half input. The steps that act on LUT data match over [`OpData`] without a wildcard:
//! - `ReplaceInverseLuts` and `RemoveInverseOps` refuse an inverse Lut1D, whose set-up is
//!   Phase 2's (WP 2.1); only Phase 2's sources make one. The Lut3D arms come with Lut3D.

use crate::bit_depth_utils::is_float_bit_depth;
use crate::exception::{Exception, Result};
use crate::logging::{is_debug_logging_enabled, log_debug};
use crate::op::{Op, OpVec, serialize_op_vec};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use crate::ops::lut1d::Lut1DOpData;
use crate::ops::lut1d::lut1d_op::{NOT_PORTED_FAST_INVERSE, create_lut1d_op};

/// Whether `flags` let the optimizer remove a pair of inverse ops of type `op_type`.
///
/// Port of `IsPairInverseEnabled` (src/OpenColorIO/OpOptimizers.cpp:23-59 @ v2.5.2).
fn is_pair_inverse_enabled(op_type: OpDataType, flags: OptimizationFlags) -> bool {
    match op_type {
        OpDataType::Cdl => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_CDL),
        OpDataType::ExposureContrast => {
            flags.has_flag(OptimizationFlags::PAIR_IDENTITY_EXPOSURE_CONTRAST)
        }
        OpDataType::FixedFunction => {
            flags.has_flag(OptimizationFlags::PAIR_IDENTITY_FIXED_FUNCTION)
        }
        OpDataType::Gamma => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_GAMMA),
        OpDataType::Lut1D => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_LUT1D),
        OpDataType::Lut3D => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_LUT3D),
        OpDataType::Log => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_LOG),

        OpDataType::GradingPrimary
        | OpDataType::GradingRgbCurve
        | OpDataType::GradingHueCurve
        | OpDataType::GradingTone => flags.has_flag(OptimizationFlags::PAIR_IDENTITY_GRADING),

        // Use composition to optimize.
        OpDataType::Exponent | OpDataType::Matrix | OpDataType::Range => false,

        // Other types are not controlled by a flag.
        OpDataType::Reference | OpDataType::NoOp => true,
    }
}

/// Whether `flags` let the optimizer combine a pair of ops whose first is of type `op_type`.
///
/// Port of `IsCombineEnabled` (src/OpenColorIO/OpOptimizers.cpp:61-70 @ v2.5.2).
fn is_combine_enabled(op_type: OpDataType, flags: OptimizationFlags) -> bool {
    // Some types are controlled by a flag.
    (op_type == OpDataType::Exponent && flags.has_flag(OptimizationFlags::COMP_EXPONENT))
        || (op_type == OpDataType::Gamma && flags.has_flag(OptimizationFlags::COMP_GAMMA))
        || (op_type == OpDataType::Lut1D && flags.has_flag(OptimizationFlags::COMP_LUT1D))
        || (op_type == OpDataType::Lut3D && flags.has_flag(OptimizationFlags::COMP_LUT3D))
        || (op_type == OpDataType::Matrix && flags.has_flag(OptimizationFlags::COMP_MATRIX))
        || (op_type == OpDataType::Range && flags.has_flag(OptimizationFlags::COMP_RANGE))
}

/// The most passes the optimizer makes.
///
/// Port of `MAX_OPTIMIZATION_PASSES` (src/OpenColorIO/OpOptimizers.cpp:72 @ v2.5.2).
const MAX_OPTIMIZATION_PASSES: i32 = 80;

/// Removes the no-op types (allocation, file and look no-ops), and returns how many.
///
/// Port of `RemoveNoOpTypes` (src/OpenColorIO/OpOptimizers.cpp:74-94 @ v2.5.2).
fn remove_no_op_types(op_vec: &mut OpVec) -> i32 {
    let mut count = 0;
    let mut iter = 0;
    while iter != op_vec.len() {
        if op_vec[iter].data().get_type() == OpDataType::NoOp {
            op_vec.erase(iter);
            count += 1;
        } else {
            iter += 1;
        }
    }
    count
}

/// Replaces each dynamic op with a copy whose dynamic properties are no longer dynamic.
///
/// Port of `RemoveDynamicProperties` (src/OpenColorIO/OpOptimizers.cpp:96-111 @ v2.5.2).
fn remove_dynamic_properties(op_vec: &mut OpVec) -> Result<()> {
    for op in op_vec.iter_mut() {
        if op.is_dynamic() {
            // Optimization flag is tested before.
            let mut replaced_by = op.clone_op()?;
            replaced_by.remove_dynamic_properties();
            *op = replaced_by;
        }
    }
    Ok(())
}

/// Removes the ops that are no-ops, identity matrices included, and returns how many.
///
/// Port of `RemoveNoOps` (src/OpenColorIO/OpOptimizers.cpp:113-130 @ v2.5.2).
fn remove_no_ops(op_vec: &mut OpVec) -> Result<i32> {
    let mut count = 0;
    let mut iter = 0;
    while iter != op_vec.len() {
        if op_vec[iter].is_no_op()? {
            op_vec.erase(iter);
            count += 1;
        } else {
            iter += 1;
        }
    }
    Ok(count)
}

/// Finalizes each op ([`Op::finalize`]): e.g. Matrix and Range ops become forward, and a
/// Lut1D gets ready for inversion.
///
/// Port of `FinalizeOps` (src/OpenColorIO/OpOptimizers.cpp:132-139 @ v2.5.2).
fn finalize_ops(op_vec: &mut OpVec) -> Result<()> {
    for op in op_vec.iter_mut() {
        // Prepare LUT 1D for inversion and ensure Matrix & Range are forward.
        op.finalize()?;
    }
    Ok(())
}

/// Replaces each op whose data has simpler ops that do the same (e.g. a CDL that doesn't use
/// its power) with those, and returns how many it replaced.
///
/// Port of `ReplaceOps` (src/OpenColorIO/OpOptimizers.cpp:141-173 @ v2.5.2).
fn replace_ops(op_vec: &mut OpVec) -> Result<i32> {
    let mut count = 0;
    let mut firstindex = 0;

    let mut tmpops = OpVec::new();

    while firstindex < op_vec.len() {
        tmpops.clear();
        let op = op_vec[firstindex].clone();
        op.get_simpler_replacement(&mut tmpops)?;

        if !tmpops.is_empty() {
            finalize_ops(&mut tmpops)?;

            // Erase the initial op we've replaced.
            op_vec.erase_range(firstindex, firstindex + 1);

            // Insert the new ops at this location.
            op_vec.insert(firstindex, &tmpops);

            // We've done something so increment the count!
            count += 1;
        }
        firstindex += 1;
    }

    Ok(count)
}

/// Replaces the ops that are identities, but not no-ops (they clamp, say), with their
/// identity replacements, and returns how many. Gammas need `IDENTITY_GAMMA`, the others
/// `IDENTITY`; a Range identity stays.
///
/// Port of `ReplaceIdentityOps` (src/OpenColorIO/OpOptimizers.cpp:175-205 @ v2.5.2).
fn replace_identity_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<i32> {
    let mut count = 0;

    // Remove any identity ops (other than gamma).
    let opt_identity = o_flags.has_flag(OptimizationFlags::IDENTITY);
    // Remove identity gamma ops (handled separately to give control over negative alpha
    // clamping).
    let opt_id_gamma = o_flags.has_flag(OptimizationFlags::IDENTITY_GAMMA);
    if opt_identity || opt_id_gamma {
        let nb_ops = op_vec.len();
        for i in 0..nb_ops {
            let op = &op_vec[i];
            let op_type = op.data().get_type();
            // Do not replace a range identity.
            if op_type != OpDataType::Range
                && ((op_type == OpDataType::Gamma && opt_id_gamma)
                    || (op_type != OpDataType::Gamma && opt_identity))
                && op.is_identity()?
            {
                // Optimization flag is tested before.
                let mut replaced_by = op.get_identity_replacement()?;
                replaced_by.finalize()?;
                op_vec[i] = replaced_by;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// The op that replaces a pair of inverse ops: the first one's identity replacement, which
/// keeps any clamping the pair does. A pair of Lut1D ops gets its own
/// (`Lut1DOpData::getPairIdentityReplacement`), which the optimizer uses from WP 2.5a: until
/// then such a pair is an error.
///
/// Port of the replacement of `RemoveInverseOps` (src/OpenColorIO/OpOptimizers.cpp:249-276 @
/// v2.5.2).
fn pair_identity_replacement(op1: &Op) -> Result<Op> {
    match &**op1.data() {
        OpData::Lut1D(_) => Err(Exception::new(
            "Lut1D: the identity replacement of a pair of inverse 1D LUTs is not ported yet \
             (Phase 2, WP 2.1).",
        )),
        OpData::Log(_)
        | OpData::FixedFunction(_)
        | OpData::Cdl(_)
        | OpData::Gamma(_)
        | OpData::Matrix(_)
        | OpData::Range(_)
        | OpData::Exponent(_)
        | OpData::GradingRgbCurve(_)
        | OpData::Reference(_)
        | OpData::NoOp(_) => op1.get_identity_replacement(),
    }
}

/// Removes each pair of adjacent ops of one type where the second undoes the first and the
/// flags allow it, or replaces it with an op that keeps the pair's clamping, and returns how
/// many pairs.
///
/// Port of `RemoveInverseOps` (src/OpenColorIO/OpOptimizers.cpp:207-300 @ v2.5.2).
fn remove_inverse_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<i32> {
    let mut count = 0;
    let mut firstindex: isize = 0; // this must be a signed int

    while firstindex < (op_vec.len() as isize - 1) {
        let index = firstindex as usize;
        let op1 = &op_vec[index];
        let op2 = &op_vec[index + 1];
        let type1 = op1.data().get_type();
        let type2 = op2.data().get_type();
        // The common case of inverse ops is to have a deep nesting:
        // ..., A, B, B', A', ...
        //
        // Consider the above, when firstindex reaches B:
        //
        //         |
        // ..., A, B, B', A', ...
        //
        // We will remove B and B'.
        // Firstindex remains pointing at the original location:
        //
        //         |
        // ..., A, A', ...
        //
        // We then decrement firstindex by 1,
        // to backstep and reconsider the A, A' case:
        //
        //      |            <-- firstindex decremented
        // ..., A, A', ...
        //

        if type1 == type2 && is_pair_inverse_enabled(type1, o_flags) && op1.is_inverse(op2) {
            // When a pair of inverse ops is removed, we want the optimized ops to give the
            // same result as the original. For certain ops such as Lut1D or Log this may
            // mean inserting a Range to emulate the clamping done by the original ops.
            let mut replaced_by = pair_identity_replacement(op1)?;

            replaced_by.finalize()?;
            if replaced_by.is_no_op()? {
                op_vec.erase_range(index, index + 2);
                firstindex = 0.max(firstindex - 1);
            } else {
                // Forward + inverse does clamp.
                op_vec[index] = replaced_by;
                op_vec.erase(index + 1);
                firstindex += 1;
            }
            count += 1;
        } else {
            firstindex += 1;
        }
    }

    Ok(count)
}

/// Combines the first pair of adjacent ops that can be combined, where the flags allow it,
/// and returns how many pairs (at most one per call).
///
/// Port of `CombineOps` (src/OpenColorIO/OpOptimizers.cpp:302-367 @ v2.5.2).
fn combine_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<i32> {
    let mut count = 0;
    let mut firstindex: isize = 0; // this must be a signed int

    let mut tmpops = OpVec::new();

    while firstindex < (op_vec.len() as isize - 1) {
        let index = firstindex as usize;
        let op1 = &op_vec[index];
        let op2 = &op_vec[index + 1];
        let type1 = op1.data().get_type();

        if is_combine_enabled(type1, o_flags) && op1.can_combine_with(op2)? {
            tmpops.clear();
            op1.combine_with(&mut tmpops, op2)?;
            finalize_ops(&mut tmpops)?;

            // The tmpops may have any number of ops in it: (0, 1, 2, ...).
            // (Size 0 would occur only if the combination results in a no-op,
            //  for example, a pair of matrices that compose into a no-op are
            //  returned as empty rather than as an identity matrix.)
            //
            // No matter the number, we need to swap them in for the original ops.

            // Erase the initial two ops we've combined.
            op_vec.erase_range(index, index + 2);

            // Insert the new ops (which may be empty) at this location.
            op_vec.insert(index, &tmpops);

            // Decrement firstindex by 1,
            // to backstep and reconsider the A, A' case.
            // See RemoveInverseOps for the full discussion of
            // why this is appropriate.
            // (`firstindex = std::max(0, firstindex - 1);` upstream: the break below leaves
            // the loop before it is read.)

            // We've done something so increment the count!
            count += 1;

            // Break, since combining ops is less desirable than other optimization options.
            // For example, it is preferable to remove a pair of ops using RemoveInverseOps
            // rather than combining them. Consider this example:
            // Lut1D A --> Matrix B --> Matrix C --> Lut1D Ainv
            // If Matrix B & C are not pair inverses but do combine into an identity, then
            // CombineOps would compose Lut1D A & Ainv, into a new Lut1D rather than
            // allowing another round of optimization which would remove them as inverses.
            break;
        } else {
            firstindex += 1;
        }
    }

    Ok(count)
}

/// Replaces each Lut1D or Lut3D evaluated inverse with a faster forward approximation, and
/// returns how many. The fast forward Lut1D (`MakeFastLut1DFromInverse`) is WP 2.1g's: until
/// then an inverse Lut1D is an error ([`NOT_PORTED_FAST_INVERSE`]). The Lut3D arm comes with
/// its variant; no other op is replaced.
///
/// Port of `ReplaceInverseLuts` (src/OpenColorIO/OpOptimizers.cpp:369-408 @ v2.5.2).
fn replace_inverse_luts(op_vec: &mut OpVec) -> Result<i32> {
    let count = 0;
    for op in op_vec.iter() {
        match &**op.data() {
            OpData::Lut1D(lut) => {
                if lut.get_direction() == TransformDirection::Inverse {
                    return Err(Exception::new(NOT_PORTED_FAST_INVERSE));
                }
            }
            // (The Lut3D arm: an inverse LUT becomes a fast forward one, counted.)
            OpData::Log(_)
            | OpData::FixedFunction(_)
            | OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::GradingRgbCurve(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => {}
        }
    }
    Ok(count)
}

/// Removes the leading Range ops that are identities, and returns how many.
///
/// Port of `RemoveLeadingClampIdentity` (src/OpenColorIO/OpOptimizers.cpp:410-434 @ v2.5.2).
fn remove_leading_clamp_identity(op_vec: &mut OpVec) -> Result<usize> {
    let mut count = 0;
    for op in op_vec.iter() {
        let o_data = op.data();
        if o_data.get_type() == OpDataType::Range && o_data.is_identity()? {
            count += 1;
        } else {
            break;
        }
    }
    if count != 0 {
        op_vec.erase_range(0, count);
    }
    Ok(count)
}

/// Removes the trailing Range ops that are identities, and returns how many.
///
/// Port of `RemoveTrailingClampIdentity` (src/OpenColorIO/OpOptimizers.cpp:436-462 @ v2.5.2).
fn remove_trailing_clamp_identity(op_vec: &mut OpVec) -> Result<usize> {
    let mut count = 0;
    for op in op_vec.iter().rev() {
        let o_data = op.data();
        if o_data.get_type() == OpDataType::Range && o_data.is_identity()? {
            count += 1;
        } else {
            break;
        }
    }

    if count != 0 {
        let len = op_vec.len();
        op_vec.erase_range(len - count, len);
    }
    Ok(count)
}

/// Whether the op is a Lut1D evaluated forward.
fn is_forward_lut1d(op: &Op) -> bool {
    match &**op.data() {
        OpData::Lut1D(lut) => lut.get_direction() == TransformDirection::Forward,
        OpData::Log(_)
        | OpData::FixedFunction(_)
        | OpData::Cdl(_)
        | OpData::Gamma(_)
        | OpData::Matrix(_)
        | OpData::Range(_)
        | OpData::Exponent(_)
        | OpData::GradingRgbCurve(_)
        | OpData::Reference(_)
        | OpData::NoOp(_) => false,
    }
}

/// The length of the prefix of separable (channel-independent) ops worth baking into a 1D
/// LUT: 0 when they are only Matrix and Range ops, which are fast already, or when the only
/// one is a forward Lut1D.
///
/// Port of `FindSeparablePrefix` (src/OpenColorIO/OpOptimizers.cpp:464-551 @ v2.5.2).
fn find_separable_prefix(ops: &OpVec) -> Result<usize> {
    let mut prefix_len = 0;

    // Loop over the ops until we get to one that cannot be combined.
    //
    // Note: For some ops such as Matrix and CDL, the separability depends upon the
    //       parameters.
    for op in ops.iter() {
        // In OCIO, the hasChannelCrosstalk method returns false for separable ops.
        if op.has_channel_crosstalk() || op.is_dynamic() {
            break;
        }

        // Op is separable, keep going.
        prefix_len += 1;
    }

    // If the only op is a 1D LUT, there is actually nothing to optimize so set the length to
    // 0. (This also avoids an infinite loop.) (If it is an inverse 1D LUT, proceed since we
    // want to replace it with a 1D LUT.)
    if prefix_len == 1 && is_forward_lut1d(&ops[0]) {
        return Ok(0);
    }

    // Some ops are so fast that it may not make sense to replace just one of those. E.g., if
    // it's just a single matrix, it may not be faster to replace it with a LUT. So make sure
    // there are some more expensive ops to combine.
    let mut expensive_ops = 0;
    for op in &ops[..prefix_len] {
        if op.has_channel_crosstalk() {
            // Non-separable ops (should never get here).
            return Err(Exception::new("Non-separable op."));
        }

        let op_type = op.data().get_type();
        if op_type == OpDataType::Matrix || op_type == OpDataType::Range {
            // Potentially separable, but inexpensive ops.
            // TODO: Perhaps a LUT is faster once the conversion to float is considered?
        } else {
            expensive_ops += 1;
        }
    }

    if expensive_ops == 0 {
        return Ok(0);
    }

    // TODO: The main source of potential lossiness is where there is a 1D LUT that has
    // extended range values followed by something that clamps. In that case, the clamp would
    // get baked into the LUT entries and therefore result in a different interpolated value.
    // Could look for that case and turn off the optimization.

    Ok(prefix_len)
}

/// Replaces the separable prefix of the ops with one Lut1D sampled for the input bit depth,
/// for integer and half input: copies of the prefix's ops render the lookup domain
/// ([`Lut1DOpData::make_lookup_domain`], [`Lut1DOpData::compose_vec`]), and the LUT replaces
/// them.
///
/// Port of `OptimizeSeparablePrefix` (src/OpenColorIO/OpOptimizers.cpp:553-596 @ v2.5.2).
fn optimize_separable_prefix(ops: &mut OpVec, in_bit_depth: BitDepth) -> Result<()> {
    if ops.is_empty() {
        return Ok(());
    }

    // TODO: Investigate whether even the F32 case could be sped up via interpolating in a
    //       half-domain Lut1D (e.g. replacing a string of exponent, log, etc.).
    if in_bit_depth == BitDepth::F32 || in_bit_depth == BitDepth::Uint32 {
        return Ok(());
    }

    let prefix_len = find_separable_prefix(ops)?;
    if prefix_len == 0 {
        return Ok(()); // Nothing to do.
    }

    let mut prefix_ops = OpVec::new();
    for op in &ops[..prefix_len] {
        prefix_ops.push_back(op.clone_op()?);
    }

    // Make a domain for the LUT. (Will be half-domain for target == 16f.)
    let mut new_domain = Lut1DOpData::make_lookup_domain(in_bit_depth)?;

    // Send the domain through the prefix ops.
    // Note: This sets the outBitDepth of newDomain to match prefixOps.
    Lut1DOpData::compose_vec(&mut new_domain, &mut prefix_ops)?;

    // Remove the prefix ops.
    ops.erase_range(0, prefix_len);

    // Insert the new LUT to replace the prefix ops.
    let mut lut_ops = OpVec::new();
    create_lut1d_op(&mut lut_ops, new_domain, TransformDirection::Forward);
    finalize_ops(&mut lut_ops)?;

    ops.insert(0, &lut_ops[..]);
    Ok(())
}

impl OpVec {
    /// Validates the ops, then finalizes each one: e.g. Matrix and Range ops become forward,
    /// and a Lut1D gets ready for inversion. Nothing happens to an empty list.
    ///
    /// Port of `OpRcPtrVec::finalize` (src/OpenColorIO/OpOptimizers.cpp:598-609 @ v2.5.2).
    pub fn finalize(&mut self) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        self.validate()?;

        // Prepare LUT 1D for inversion and ensure Matrix & Range are forward.
        finalize_ops(self)
    }

    /// Optimizes the ops as `o_flags` allow: removes the no-op types, then, pass after pass
    /// until nothing changes, removes no-ops, simplifies ops, replaces identities, removes
    /// inverse pairs and combines a pair. At the debug level, it logs the list before and after.
    ///
    /// Port of `OpRcPtrVec::optimize` (src/OpenColorIO/OpOptimizers.cpp:611-756 @ v2.5.2).
    pub fn optimize(&mut self, o_flags: OptimizationFlags) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        if is_debug_logging_enabled() {
            let mut oss = b"\n**\nOptimizing Op Vec...\n".to_vec();
            oss.extend_from_slice(&serialize_op_vec(self, 4)?);
            oss.push(b'\n');

            log_debug(oss);
        }

        let original_size = self.len();

        // NoOpType can be removed (facilitates conversion to a CPU/GPUProcessor).
        let total_nooptype = remove_no_op_types(self);

        if o_flags == OptimizationFlags::NONE {
            if is_debug_logging_enabled() {
                let final_size = self.len();

                let mut os = format!(
                    "**\nOptimized {original_size}->{final_size}, 1 pass, {total_nooptype} \
                     no-op types removed\n"
                )
                .into_bytes();
                os.extend_from_slice(&serialize_op_vec(self, 4)?);
                log_debug(os);
            }

            return Ok(());
        }

        // Keep dynamic ops using their default values. Remove the ability to modify them
        // dynamically.
        let remove_dynamic = o_flags.has_flag(OptimizationFlags::NO_DYNAMIC_PROPERTIES);
        if remove_dynamic {
            remove_dynamic_properties(self)?;
        }

        // As the input and output bit-depths represent the color processing request and they
        // may be altered by the following optimizations, preserve their values.

        let mut total_noops = 0;
        let mut total_replacedops = 0;
        let mut total_identityops = 0;
        let mut total_inverseops = 0;
        let mut total_combines = 0;
        let mut total_inverses = 0;
        let mut passes = 0;

        let optimize_identity = o_flags.has_flag(OptimizationFlags::IDENTITY);
        let replace_ops_enabled = o_flags.has_flag(OptimizationFlags::SIMPLIFY_OPS);

        let fast_lut = o_flags.has_flag(OptimizationFlags::LUT_INV_FAST);

        while passes <= MAX_OPTIMIZATION_PASSES {
            // Remove all ops for which isNoOp is true, including identity matrices.
            let noops = if optimize_identity {
                remove_no_ops(self)?
            } else {
                0
            };

            // Replace all complex ops with simpler ops (e.g., a CDL which only scales with a
            // matrix). Note this might increase the number of ops.
            let replaced_ops = if replace_ops_enabled {
                replace_ops(self)?
            } else {
                0
            };

            // Replace all complex identities with simpler ops (e.g., an identity Lut1D with a
            // range).
            let identityops = replace_identity_ops(self, o_flags)?;

            // Remove all adjacent pairs of ops that are inverses of each other.
            let inverseops = remove_inverse_ops(self, o_flags)?;

            // Combine a pair of ops, for example multiply two adjacent Matrix ops.
            // (Combines at most one pair on each iteration.)
            let combines = combine_ops(self, o_flags)?;

            if noops + identityops + inverseops + combines == 0 {
                // No optimization progress was made, so stop trying. If requested, replace
                // any inverse LUTs with faster forward LUTs and do another pass to see if more
                // optimization is possible.
                if fast_lut {
                    let inverses = replace_inverse_luts(self)?;
                    if inverses == 0 {
                        break;
                    }

                    total_inverses += inverses;
                } else {
                    break;
                }
            }

            total_noops += noops;
            total_replacedops += replaced_ops;
            total_identityops += identityops;
            total_inverseops += inverseops;
            total_combines += combines;

            passes += 1;
        }

        if passes == MAX_OPTIMIZATION_PASSES {
            log_debug(format!(
                "The max number of passes, {passes}, was reached during optimization. This is \
                 likely a sign that either the complexity of the color transform is very high, \
                 or that some internal optimizers are in conflict (undo-ing / redo-ing the \
                 other's results)."
            ));
        }

        if is_debug_logging_enabled() {
            let final_size = self.len();

            let mut os = format!(
                "**\nOptimized {original_size}->{final_size}, {passes} passes, \
                 {total_nooptype} no-op types removed, {total_noops} no-ops removed, \
                 {total_replacedops} ops replaced, {total_identityops} identity ops replaced, \
                 {total_inverseops} inverse op pairs removed, {total_combines} ops combined, \
                 {total_inverses} ops inverted\n"
            )
            .into_bytes();
            os.extend_from_slice(&serialize_op_vec(self, 4)?);
            log_debug(os);
        }

        Ok(())
    }

    /// The optimizations for the processor's bit depths: an integer input or output already
    /// clamps, so the leading or trailing Range identities go; with `COMP_SEPARABLE_PREFIX`,
    /// the separable prefix is baked into a Lut1D for an integer or half input.
    ///
    /// Port of `OpRcPtrVec::optimizeForBitdepth` (src/OpenColorIO/OpOptimizers.cpp:758-778 @
    /// v2.5.2).
    pub fn optimize_for_bitdepth(
        &mut self,
        in_bit_depth: BitDepth,
        out_bit_depth: BitDepth,
        o_flags: OptimizationFlags,
    ) -> Result<()> {
        if !self.is_empty() {
            if !is_float_bit_depth(in_bit_depth)? {
                remove_leading_clamp_identity(self)?;
            }
            if !is_float_bit_depth(out_bit_depth)? {
                remove_trailing_clamp_identity(self)?;
            }
            if o_flags.has_flag(OptimizationFlags::COMP_SEPARABLE_PREFIX) {
                optimize_separable_prefix(self, in_bit_depth)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "op_optimizers_tests.rs"]
mod tests;
