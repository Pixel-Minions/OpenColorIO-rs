// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Context variables: a port of `src/OpenColorIO/ContextVariableUtils.cpp` @ v2.5.2, so far
//! `CollectContextVariables` for the transform classes in the port. The variables' resolution
//! and the overloads of the classes that use them come with the context's state (Phase 3).

use crate::config::Config;
use crate::context::Context;
use crate::transform::Transform;

/// Whether `transform` uses context variables, which it adds to `used_context_vars`: so far
/// none of the classes in the port use any, and a group asks its children.
///
/// Port of `CollectContextVariables(const Config &, const Context &, ConstTransformRcPtr,
/// ContextRcPtr &)` (src/OpenColorIO/ContextVariableUtils.cpp:179-207 @ v2.5.2) and its
/// `GroupTransform` overload (transforms/GroupTransform.cpp:206-223 @ v2.5.2). The overloads of
/// the color space, display view, file and look transforms come with those classes; the other
/// classes use none.
// The color space, display view, file and look transforms read the config, the context and
// the used variables; the group only passes them on.
#[allow(clippy::only_used_in_recursion)]
pub(crate) fn collect_context_variables(
    config: &Config,
    context: &Context,
    transform: &Transform,
    used_context_vars: &mut Context,
) -> bool {
    match transform {
        Transform::Group(tr) => {
            let mut found_context_vars = false;

            for idx in 0..tr.num_transforms() {
                let child = tr.transform(idx).expect("an index inside the group");
                if collect_context_variables(config, context, child, used_context_vars) {
                    found_context_vars = true;
                }
            }

            found_context_vars
        }
        // Their overloads read the config's color spaces, displays and looks; they come with
        // their op builders (WP 3.2), and until then the processor refuses to build their ops.
        Transform::ColorSpace(_) | Transform::DisplayView(_) | Transform::Look(_) => false,
        // The classes that use no context variable.
        Transform::Allocation(_)
        | Transform::Cdl(_)
        | Transform::Exponent(_)
        | Transform::ExponentWithLinear(_)
        | Transform::FixedFunction(_)
        | Transform::LogAffine(_)
        | Transform::LogCamera(_)
        | Transform::Lut1D(_)
        | Transform::Log(_)
        | Transform::Matrix(_)
        | Transform::Range(_) => false,
    }
}
