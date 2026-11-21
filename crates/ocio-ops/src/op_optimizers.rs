// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The finalization and optimization of op lists: a port of
//! `src/OpenColorIO/OpOptimizers.cpp` @ v2.5.2.
//!
//! The optimizer's core: `OpRcPtrVec::finalize`, `optimize` (the pass loop and its generic
//! steps, which ask the ops only through [`Op`]'s methods) and `optimizeForBitdepth`, with
//! `OptimizeSeparablePrefix`, which bakes a prefix of separable ops into a Lut1D for integer
//! and half input. The steps that act on LUT data match over [`OpData`] without a wildcard:
//! - `ReplaceInverseLuts` replaces an inverse Lut1D or Lut3D with its fast forward LUT, and
//!   `RemoveInverseOps` replaces a pair of inverse Lut1Ds with the pair's own identity
//!   replacement (`Lut1DOpData::getPairIdentityReplacement`), a pair of Lut3Ds with the
//!   first's (`OpData::getIdentityReplacement`).

use crate::bit_depth_utils::is_float_bit_depth;
use crate::exception::{Exception, Result};
use crate::logging::{is_debug_logging_enabled, log_debug};
use crate::op::{Op, OpVec, serialize_op_vec};
use crate::op_data::{OpData, OpDataType};
use crate::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use crate::ops::lut1d::Lut1DOpData;
use crate::ops::lut1d::lut1d_op::create_lut1d_op;
use crate::ops::lut1d::lut1d_op_data::make_fast_lut1d_from_inverse;
use crate::ops::lut3d::lut3d_op::create_lut3d_op;
use crate::ops::lut3d::lut3d_op_data::make_fast_lut3d_from_inverse;
use crate::ops::matrix::matrix_op::create_matrix_op;
use crate::ops::range::range_op::create_range_op;

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
/// Port of `MAX_OPTIMIZATION_PASSES` (src/OpenColorIO/OpOptimizers.cpp:75 @ v2.6.0).
const MAX_OPTIMIZATION_PASSES: i32 = 80;

/// Removes the no-op types (allocation, file and look no-ops), and returns how many. Public for
/// the file tests of the `ocio` crate (`opt_prefix_test1` calls it, as upstream's test file
/// includes the optimizer's source). The flags are unused.
///
/// Port of `RemoveNoOpTypes` (src/OpenColorIO/OpOptimizers.cpp:77-87 @ v2.6.0).
pub fn remove_no_op_types(op_vec: &mut OpVec, _flags: OptimizationFlags) -> usize {
    // `std::remove_if` keeps the order of the ops it keeps.
    let before = op_vec.len();
    let mut i = 0;
    while i < op_vec.len() {
        if op_vec[i].is_no_op_type() {
            op_vec.erase(i);
        } else {
            i += 1;
        }
    }
    before - op_vec.len()
}

/// With `NO_DYNAMIC_PROPERTIES`, replaces each dynamic op with a copy whose dynamic properties
/// are no longer dynamic, and returns how many.
///
/// Port of `RemoveDynamicProperties` (src/OpenColorIO/OpOptimizers.cpp:89-110 @ v2.6.0).
pub fn remove_dynamic_properties(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    let mut count = 0;
    let remove_dynamic = o_flags.has_flag(OptimizationFlags::NO_DYNAMIC_PROPERTIES);
    if !remove_dynamic {
        return Ok(count);
    }

    for op in op_vec.iter_mut() {
        if op.is_dynamic() {
            // Optimization flag is tested before.
            let mut replaced_by = op.clone_op()?;
            replaced_by.remove_dynamic_properties();
            *op = replaced_by;
            count += 1;
        }
    }
    Ok(count)
}

/// With `IDENTITY`, removes the ops that are no-ops, identity matrices included, and returns
/// how many.
///
/// Port of `RemoveNoOps` (src/OpenColorIO/OpOptimizers.cpp:112-129 @ v2.6.0).
fn remove_no_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    let optimize_identity = o_flags.has_flag(OptimizationFlags::IDENTITY);
    if !optimize_identity {
        return Ok(0);
    }

    // `std::remove_if` keeps the order of the ops it keeps.
    let before = op_vec.len();
    let mut i = 0;
    while i < op_vec.len() {
        if op_vec[i].is_no_op()? {
            op_vec.erase(i);
        } else {
            i += 1;
        }
    }
    Ok(before - op_vec.len())
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

/// With `SIMPLIFY_OPS`, replaces each op whose data has simpler ops that do the same (e.g. a
/// CDL that doesn't use its power) with those, and returns how many it replaced.
///
/// Port of `ReplaceOps` (src/OpenColorIO/OpOptimizers.cpp:142-200 @ v2.6.0): it builds a new
/// list from the first replacement on, so an op that replaces another isn't looked at again in
/// the same call (2.5.2 inserted the replacement in place and looked at all but its first op).
fn replace_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    let mut count = 0;
    let replace_ops = o_flags.has_flag(OptimizationFlags::SIMPLIFY_OPS);
    if !replace_ops {
        return Ok(count);
    }

    let mut tmpops = OpVec::new();
    let mut new_op_vec = OpVec::new();
    let mut rebuilding = false;

    for i in 0..op_vec.len() {
        tmpops.clear();
        op_vec[i].get_simpler_replacement(&mut tmpops)?;

        if !tmpops.is_empty() {
            if !rebuilding {
                rebuilding = true;
                // Catch up any unmodified elements prior to this replacement.
                new_op_vec.insert(0, &op_vec[..i]);
            }

            finalize_ops(&mut tmpops)?;
            let end = new_op_vec.len();
            new_op_vec.insert(end, &tmpops);
            count += 1;
        } else if rebuilding {
            let end = new_op_vec.len();
            new_op_vec.insert(end, &op_vec[i..i + 1]);
        }
    }

    if rebuilding {
        *op_vec = new_op_vec;
    }

    Ok(count)
}

/// Replaces the ops that are identities, but not no-ops (they clamp, say), with their
/// identity replacements, and returns how many. Gammas need `IDENTITY_GAMMA`, the others
/// `IDENTITY`; a Range identity stays.
///
/// Port of `ReplaceIdentityOps` (src/OpenColorIO/OpOptimizers.cpp:202-232 @ v2.6.0).
fn replace_identity_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
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
/// keeps any clamping the pair does. A pair of Lut1D ops gets its own, a no-op Matrix or a
/// clamping Range, from both halves of the pair: "Only the inverse LUT has the values needed
/// to generate the replacement."
///
/// Port of the replacement of `RemoveInverseOps` (src/OpenColorIO/OpOptimizers.cpp:249-276 @
/// v2.5.2).
fn pair_identity_replacement(op1: &Op, op2: &Op) -> Result<Op> {
    match &**op1.data() {
        OpData::Lut1D(lut1) => {
            // Lut1D gets special handling so that both halfs of the pair are available.
            // Only the inverse LUT has the values needed to generate the replacement.
            let OpData::Lut1D(lut2) = &**op2.data() else {
                unreachable!("the pair's ops have one type")
            };

            let op_data = lut1.get_pair_identity_replacement(lut2)?;

            let mut ops = OpVec::new();
            match op_data {
                // No-op that will be optimized.
                OpData::Matrix(mat) => create_matrix_op(&mut ops, mat, TransformDirection::Forward),
                // Clamping op.
                OpData::Range(range) => {
                    create_range_op(&mut ops, range, TransformDirection::Forward)?;
                }
                other => unreachable!(
                    "getPairIdentityReplacement gives a Matrix or a Range, not {:?}",
                    other.get_type()
                ),
            }
            Ok(ops[0].clone())
        }
        OpData::Log(_)
        | OpData::FixedFunction(_)
        | OpData::Cdl(_)
        | OpData::Gamma(_)
        | OpData::Matrix(_)
        | OpData::Range(_)
        | OpData::Exponent(_)
        | OpData::GradingHueCurve(_)
        | OpData::GradingTone(_)
        | OpData::GradingRgbCurve(_)
        | OpData::ExposureContrast(_)
        | OpData::GradingPrimary(_)
        | OpData::Lut3D(_)
        | OpData::Reference(_)
        | OpData::NoOp(_) => op1.get_identity_replacement(),
    }
}

/// Removes each pair of adjacent ops of one type where the second undoes the first and the
/// flags allow it, or replaces it with an op that keeps the pair's clamping, and returns how
/// many pairs. The ops kept so far are a stack: removing a pair exposes the op before it to
/// the next one, and a pair's replacement is compared with the op after the pair.
///
/// Port of `RemoveInverseOps` (src/OpenColorIO/OpOptimizers.cpp:234-328 @ v2.6.0).
fn remove_inverse_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    let mut count = 0;
    let mut write_idx = 0;
    let num_ops = op_vec.len();

    for read_idx in 0..num_ops {
        if write_idx > 0 {
            let last_op = &op_vec[write_idx - 1];
            let const_op = &op_vec[read_idx];
            let type1 = last_op.data().get_type();
            let type2 = const_op.data().get_type();

            // The common case of inverse ops is to have a deep nesting:
            // ..., A, B, B', A', ...
            //
            // By treating the processed portion of the vector as a stack (`writeIdx`),
            // popping an element automatically exposes `A` to be reconsidered against `A'`
            // on the next loop iteration.

            if type1 == type2
                && is_pair_inverse_enabled(type1, o_flags)
                && last_op.is_inverse(const_op)
            {
                // When a pair of inverse ops is removed, we want the optimized ops to give the
                // same result as the original. For certain ops such as Lut1D or Log this may
                // mean inserting a Range to emulate the clamping done by the original ops.
                let mut replaced_by = pair_identity_replacement(last_op, const_op)?;

                replaced_by.finalize()?;
                if replaced_by.is_no_op()? {
                    // Pop the last element off the stack to naturally backstep.
                    write_idx -= 1;
                } else {
                    // Forward + inverse does clamp.
                    op_vec[write_idx - 1] = replaced_by;
                }
                count += 1;
                continue;
            }
        }

        // Push the current op to the stack if it wasn't cancelled out.
        if write_idx != read_idx {
            op_vec[write_idx] = op_vec[read_idx].clone();
        }
        write_idx += 1;
    }

    if count > 0 {
        // Drop any unused elements at the tail in a single O(N) pass.
        let len = op_vec.len();
        op_vec.erase_range(write_idx, len);
    }

    Ok(count)
}

/// Combines the first pair of adjacent ops that can be combined, where the flags allow it,
/// and returns how many pairs (at most one per call).
///
/// Port of `CombineOps` (src/OpenColorIO/OpOptimizers.cpp:330-388 @ v2.6.0): the first
/// adjacent pair that combines (`std::adjacent_find`) is replaced by the ops it combines into,
/// and the count is 1 even when the pair combines into nothing.
fn combine_ops(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    // `std::adjacent_find`: the first pair whose first op may combine with the second.
    let mut found = None;
    for i in 1..op_vec.len() {
        let op1 = &op_vec[i - 1];
        if is_combine_enabled(op1.data().get_type(), o_flags) && op1.can_combine_with(&op_vec[i])? {
            found = Some(i - 1);
            break;
        }
    }

    let Some(it) = found else {
        return Ok(0);
    };

    let mut tmpops = OpVec::new();
    op_vec[it].combine_with(&mut tmpops, &op_vec[it + 1])?;
    finalize_ops(&mut tmpops)?;

    // The tmpops may have any number of ops in it: (0, 1, 2, ...).
    // (Size 0 would occur only if the combination results in a no-op,
    //  for example, a pair of matrices that compose into a no-op are
    //  returned as empty rather than as an identity matrix.)
    //
    // No matter the number, we need to swap them in for the original ops.
    op_vec.erase_range(it, it + 2);
    op_vec.insert(it, &tmpops);

    // Return 1 since combining ops is less desirable than other optimization options.
    // For example, it is preferable to remove a pair of ops using RemoveInverseOps
    // rather than combining them. Consider this example:
    // Lut1D A --> Matrix B --> Matrix C --> Lut1D Ainv
    // If Matrix B & C are not pair inverses but do combine into an identity, then
    // CombineOps would compose Lut1D A & Ainv, into a new Lut1D rather than
    // allowing another round of optimization which would remove them as inverses.
    Ok(1)
}

/// Replaces each Lut1D or Lut3D evaluated inverse with a faster forward approximation, and
/// returns how many: an inverse Lut1D or Lut3D becomes its fast forward LUT
/// ([`make_fast_lut1d_from_inverse`], [`make_fast_lut3d_from_inverse`]); no other op is
/// replaced.
///
/// Port of `ReplaceInverseLuts` (src/OpenColorIO/OpOptimizers.cpp:398-441 @ v2.6.0): only with
/// `LUT_INV_FAST`.
fn replace_inverse_luts(op_vec: &mut OpVec, o_flags: OptimizationFlags) -> Result<usize> {
    let mut count = 0;
    let fast_lut = o_flags.has_flag(OptimizationFlags::LUT_INV_FAST);
    if !fast_lut {
        return Ok(count);
    }

    let nb_ops = op_vec.len();
    for i in 0..nb_ops {
        let replaced_by = match &**op_vec[i].data() {
            OpData::Lut1D(lut_data) => {
                if lut_data.get_direction() == TransformDirection::Inverse {
                    let inv_lut_data = make_fast_lut1d_from_inverse(lut_data)?;
                    let mut tmpops = OpVec::new();
                    create_lut1d_op(&mut tmpops, inv_lut_data, TransformDirection::Forward);
                    finalize_ops(&mut tmpops)?;
                    Some(tmpops[0].clone())
                } else {
                    None
                }
            }
            OpData::Lut3D(lut_data) => {
                if lut_data.get_direction() == TransformDirection::Inverse {
                    let inv_lut_data = make_fast_lut3d_from_inverse(lut_data)?;
                    let mut tmpops = OpVec::new();
                    create_lut3d_op(&mut tmpops, inv_lut_data, TransformDirection::Forward);
                    finalize_ops(&mut tmpops)?;
                    Some(tmpops[0].clone())
                } else {
                    None
                }
            }
            OpData::Log(_)
            | OpData::FixedFunction(_)
            | OpData::Cdl(_)
            | OpData::Gamma(_)
            | OpData::Matrix(_)
            | OpData::Range(_)
            | OpData::Exponent(_)
            | OpData::GradingHueCurve(_)
            | OpData::GradingTone(_)
            | OpData::GradingRgbCurve(_)
            | OpData::ExposureContrast(_)
            | OpData::GradingPrimary(_)
            | OpData::Reference(_)
            | OpData::NoOp(_) => None,
        };
        if let Some(op) = replaced_by {
            op_vec[i] = op;
            count += 1;
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
        | OpData::GradingHueCurve(_)
        | OpData::GradingTone(_)
        | OpData::GradingRgbCurve(_)
        | OpData::ExposureContrast(_)
        | OpData::GradingPrimary(_)
        | OpData::Lut3D(_)
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

/// Runs one step of the optimizer and returns its count; at the debug level, logs "<name> - <n>
/// optimisations found" and, when it changed something, the list after it.
///
/// Port of `PerformOptimisation` (src/OpenColorIO/OpOptimizers.cpp:607-621 @ v2.6.0).
fn perform_optimisation(
    operation: impl FnOnce(&mut OpVec, OptimizationFlags) -> Result<usize>,
    op_vec: &mut OpVec,
    o_flags: OptimizationFlags,
    debug_logging_enabled: bool,
    operation_name: &str,
) -> Result<usize> {
    let ops_removed = operation(op_vec, o_flags)?;
    if debug_logging_enabled {
        let mut os = format!("{operation_name} - {ops_removed} optimisations found\n").into_bytes();
        if ops_removed != 0 {
            os.extend_from_slice(&serialize_op_vec(op_vec, 4)?);
        }
        log_debug(os);
    }
    Ok(ops_removed)
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
    /// inverse pairs and combines a pair. At the debug level, it logs the list before, each
    /// step's count (and the list after a step that changed it), each pass, and the list after.
    ///
    /// Port of `OpRcPtrVec::optimize` (src/OpenColorIO/OpOptimizers.cpp:638-772 @ v2.6.0).
    pub fn optimize(&mut self, o_flags: OptimizationFlags) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        let debug_logging_enabled = is_debug_logging_enabled();
        if debug_logging_enabled {
            let mut message = b"\n**\nOptimizing Op Vec...\n".to_vec();
            message.extend_from_slice(&serialize_op_vec(self, 4)?);
            log_debug(message);
        }

        let original_size = self.len();
        let mut total_noops = 0;
        let mut total_replacedops = 0;
        let mut total_identityops = 0;
        let mut total_inverseops = 0;
        let mut total_combines = 0;
        let mut total_inverses = 0;
        let mut passes: i32 = 1;

        // NoOpType can be removed (facilitates conversion to a CPU/GPUProcessor).
        let total_nooptype = perform_optimisation(
            |ops, flags| Ok(remove_no_op_types(ops, flags)),
            self,
            o_flags,
            debug_logging_enabled,
            "RemoveNoOpTypes",
        )?;

        if o_flags == OptimizationFlags::NONE {
            if debug_logging_enabled {
                let final_size = self.len();

                let mut os = format!(
                    "**\nOptimized {original_size}->{final_size}, {passes} pass, \
                     {total_nooptype} no-op types removed\n"
                )
                .into_bytes();
                os.extend_from_slice(&serialize_op_vec(self, 4)?);
                log_debug(os);
            }

            return Ok(());
        }

        // Keep dynamic ops using their default values. Remove the ability to modify them
        // dynamically.
        let total_dynamic_ops = perform_optimisation(
            remove_dynamic_properties,
            self,
            o_flags,
            debug_logging_enabled,
            "RemoveDynamicProperties",
        )?;

        while passes <= MAX_OPTIMIZATION_PASSES {
            if debug_logging_enabled {
                log_debug(format!("Starting pass {passes}"));
            }
            // Remove all ops for which isNoOp is true, including identity matrices.
            let noops = perform_optimisation(
                remove_no_ops,
                self,
                o_flags,
                debug_logging_enabled,
                "RemoveNoOps",
            )?;
            total_noops += noops;

            // Replace all complex ops with simpler ops (e.g., a CDL which only scales with a
            // matrix). Note this might increase the number of ops.
            let replaced_ops = perform_optimisation(
                replace_ops,
                self,
                o_flags,
                debug_logging_enabled,
                "ReplaceOps",
            )?;
            total_replacedops += replaced_ops;

            // Replace all complex identities with simpler ops (e.g., an identity Lut1D with a
            // range).
            let identityops = perform_optimisation(
                replace_identity_ops,
                self,
                o_flags,
                debug_logging_enabled,
                "ReplaceIdentityOps",
            )?;
            total_identityops += identityops;

            // Remove all adjacent pairs of ops that are inverses of each other.
            let inverseops = perform_optimisation(
                remove_inverse_ops,
                self,
                o_flags,
                debug_logging_enabled,
                "RemoveInverseOps",
            )?;
            total_inverseops += inverseops;

            // Combine a pair of ops, for example multiply two adjacent Matrix ops.
            // (Combines at most one pair on each iteration.)
            let combines = perform_optimisation(
                combine_ops,
                self,
                o_flags,
                debug_logging_enabled,
                "CombineOps",
            )?;
            total_combines += combines;

            if noops + replaced_ops + identityops + inverseops + combines == 0 {
                // No optimization progress was made, so stop trying. If requested, replace
                // any inverse LUTs with faster forward LUTs and do another pass to see if more
                // optimization is possible.
                let inverses = perform_optimisation(
                    replace_inverse_luts,
                    self,
                    o_flags,
                    debug_logging_enabled,
                    "ReplaceInverseLuts",
                )?;
                total_inverses += inverses;

                if inverses == 0 {
                    break;
                }
            }

            if debug_logging_enabled {
                log_debug(format!(
                    "Pass {passes} summary: {noops} no-op removed, {replaced_ops} ops replaced, \
                     {identityops} identity ops replaced, {inverseops} inverse op pairs \
                     removed, {combines} ops combined."
                ));
            }
            passes += 1;
        }

        if debug_logging_enabled && passes == MAX_OPTIMIZATION_PASSES {
            log_debug(format!(
                "The max number of passes, {passes}, was reached during optimization. This is \
                 likely a sign that either the complexity of the color transform is very high, \
                 or that some internal optimizers are in conflict (undo-ing / redo-ing the \
                 other's results)."
            ));
        }

        if debug_logging_enabled {
            let final_size = self.len();

            let mut os = format!(
                "**\nOptimized {original_size}->{final_size}, {passes} passes, \
                 {total_nooptype} no-op types removed, {total_dynamic_ops} dynamic-ops made \
                 static, {total_noops} no-ops removed, {total_replacedops} ops replaced, \
                 {total_identityops} identity ops replaced, {total_inverseops} inverse op pairs \
                 removed, {total_combines} ops combined, {total_inverses} ops inverted\n"
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
