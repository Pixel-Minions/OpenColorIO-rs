// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Context variables: a port of `src/OpenColorIO/ContextVariableUtils.cpp` and `.h` @ v2.5.2:
//! the variables' map and its order, finding, loading and resolving them, and
//! `CollectContextVariables` for the transform classes in the port. The overloads of the
//! classes that use context variables come with those classes.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ocio_ops::platform::env_provider;
use ocio_ops::utils::string_utils::{find, replace_in_place, reverse_find};

use crate::config::Config;
use crate::context::Context;
use crate::transform::Transform;

/// A context variable's name, ordered as `EnvMapKey` orders them: longer names first, then
/// bytes in order, so that `$TEST1NG` is replaced before `$TEST1`.
///
/// Port of `EnvMapKey` (src/OpenColorIO/ContextVariableUtils.h:28-44 @ v2.5.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvMapKey(pub Vec<u8>);

impl Ord for EnvMapKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // If the lengths are unequal, sort by length (the longer first); otherwise, use the
        // standard string sort comparison (std::string's operator<, bytes as unsigned).
        other
            .0
            .len()
            .cmp(&self.0.len())
            .then_with(|| self.0.cmp(&other.0))
    }
}

impl PartialOrd for EnvMapKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The context variables: name to value.
///
/// Port of `EnvMap` (ContextVariableUtils.h:45 @ v2.5.2).
pub type EnvMap = BTreeMap<EnvMapKey, Vec<u8>>;

/// The context variables a resolution used: name to value, names in byte order.
///
/// Port of `UsedEnvs` (ContextVariableUtils.h:52 @ v2.5.2).
pub type UsedEnvs = BTreeMap<Vec<u8>, Vec<u8>>;

/// Whether `str` holds `$` or `%`.
///
/// Port of `ContainsContextVariableToken` (src/OpenColorIO/ContextVariableUtils.cpp:47-53 @
/// v2.5.2).
pub fn contains_context_variable_token(str: &[u8]) -> bool {
    find(str, b"$").is_some() || find(str, b"%").is_some()
}

/// Whether `str` may hold a context variable: a `$`, or two `%`.
///
/// Port of `ContainsContextVariables` (ContextVariableUtils.cpp:56-76 @ v2.5.2).
pub fn contains_context_variables(str: &[u8]) -> bool {
    if find(str, b"$").is_some() {
        return true;
    }
    if let Some(begin) = find(str, b"%")
        && let Some(end) = reverse_find(str, b"%")
        && begin != end
    {
        return true;
    }
    false
}

/// Adds the environment's variables to `map` (an existing name keeps its value), or with
/// `update`, updates the values of the names `map` already has. Each entry of the
/// environment is split at its first `=`; an entry without one is skipped.
///
/// Port of `LoadEnvironment` (ContextVariableUtils.cpp:78-125 @ v2.5.2).
pub fn load_environment(map: &mut EnvMap, update: bool) {
    for env_str in env_provider().entries() {
        let Some(pos) = env_str.iter().position(|&c| c == b'=') else {
            continue;
        };
        let name = env_str[..pos].to_vec();
        let value = env_str[pos + 1..].to_vec();
        if update {
            // Update existing key:values that match.
            if let Some(v) = map.get_mut(&EnvMapKey(name)) {
                *v = value;
            }
        } else {
            map.entry(EnvMapKey(name)).or_insert(value);
        }
    }
}

/// Port of `ResolveContextVariablesImpl` (ContextVariableUtils.cpp:127-172 @ v2.5.2).
fn resolve_context_variables_impl(
    str: &[u8],
    map: &EnvMap,
    used: &mut UsedEnvs,
    depth: i32,
) -> Vec<u8> {
    // Guard against infinite recursion from cyclic variable references.
    if depth > 32 {
        return str.to_vec();
    }
    // Early exit if no reserved tokens are found.
    if !contains_context_variables(str) {
        return str.to_vec();
    }
    let orig = str.to_vec();
    let mut newstr = str.to_vec();
    // This walks through the envmap in key order, from longest to shortest to handle
    // envvars which are substrings.
    for (name, value) in map {
        let name = &name.0;
        let braced = [b"${".as_slice(), name, b"}"].concat();
        if replace_in_place(&mut newstr, &braced, value) {
            used.insert(name.clone(), value.clone());
        }
        let dollar = [b"$".as_slice(), name].concat();
        if replace_in_place(&mut newstr, &dollar, value) {
            used.insert(name.clone(), value.clone());
        }
        let percent = [b"%".as_slice(), name, b"%"].concat();
        if replace_in_place(&mut newstr, &percent, value) {
            used.insert(name.clone(), value.clone());
        }
    }
    // recursively call till string doesn't expand anymore
    if newstr != orig {
        return resolve_context_variables_impl(&newstr, map, used, depth + 1);
    }
    orig
}

/// `str` with its context variables (`${NAME}`, `$NAME`, `%NAME%`) replaced by their
/// values, longest name first, again on the result until nothing changes (32 levels at
/// most); the variables used go to `used`.
///
/// Port of `ResolveContextVariables` (ContextVariableUtils.cpp:174-177 @ v2.5.2).
pub fn resolve_context_variables(str: &[u8], map: &EnvMap, used: &mut UsedEnvs) -> Vec<u8> {
    resolve_context_variables_impl(str, map, used, 0)
}

/// Whether `transform` uses context variables, which it adds to `used_context_vars`: so far
/// none of the classes in the port use any, and a group asks its children.
///
/// Port of `CollectContextVariables(const Config &, const Context &, ConstTransformRcPtr,
/// ContextRcPtr &)` (src/OpenColorIO/ContextVariableUtils.cpp:179-206 @ v2.5.2) and its
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
        // The classes that use no context variable.
        Transform::Allocation(_)
        | Transform::Cdl(_)
        | Transform::Exponent(_)
        | Transform::ExponentWithLinear(_)
        | Transform::LogAffine(_)
        | Transform::LogCamera(_)
        | Transform::Lut1D(_)
        | Transform::Log(_)
        | Transform::Matrix(_)
        | Transform::Range(_) => false,
    }
}

#[cfg(test)]
#[path = "context_variable_utils_tests.rs"]
mod tests;
