// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's config of a spec of the oracle's `serialize_built_config`, made through the
//! port's API with the same calls, in the same order, as the oracle makes through the wheel's
//! (`ocio_oracle/text.py` `build_config` and `_build_colorspace`).

use std::sync::Arc;

use ocio::{
    Allocation, AllocationTransform, ColorSpace, ColorSpaceDirection, Config, FileRules,
    GroupTransform, MatrixTransform, Transform, ViewingRules,
};
use ocio_ops::platform::{MapEnv, set_thread_env_provider};
use serde_json::Value;

/// A spec's string: its UTF-8 bytes, or a `{"hex": ...}`'s bytes.
fn bytes(v: &Value) -> Vec<u8> {
    match v {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Object(o) => {
            let h = o["hex"].as_str().expect("a hex string");
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
                .collect()
        }
        other => panic!("not a string: {other}"),
    }
}

fn list<'a>(spec: &'a Value, key: &str) -> &'a [Value] {
    spec.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn f64s(bits: &Value) -> Vec<f64> {
    bits.as_array()
        .expect("bits")
        .iter()
        .map(|b| f64::from_bits(b.as_u64().expect("u64 bits")))
        .collect()
}

fn f32s(bits: &Value) -> Vec<f32> {
    bits.as_array()
        .expect("bits")
        .iter()
        .map(|b| f32::from_bits(u32::try_from(b.as_u64().expect("u32 bits")).expect("u32")))
        .collect()
}

/// The oracle's `_build_colorspace`.
fn build_color_space(spec: &Value) -> ColorSpace {
    let mut cs = ColorSpace::new();
    cs.set_name(bytes(&spec["name"]));
    if let Some(v) = spec.get("family") {
        cs.set_family(bytes(v));
    }
    if let Some(v) = spec.get("description") {
        cs.set_description(bytes(v));
    }
    if let Some(v) = spec.get("encoding") {
        cs.set_encoding(bytes(v));
    }
    for alias in list(spec, "aliases") {
        cs.add_alias(bytes(alias));
    }
    for category in list(spec, "categories") {
        cs.add_category(bytes(category));
    }
    for pair in list(spec, "interchange") {
        cs.set_interchange_attribute(bytes(&pair[0]), bytes(&pair[1]))
            .expect("setInterchangeAttribute");
    }
    if let Some(bits) = spec.get("allocation_vars_bits") {
        cs.set_allocation_vars(&f32s(bits));
    }
    if spec.get("matrices_bits").is_some() {
        let mut group = GroupTransform::new();
        for bits in list(spec, "matrices_bits") {
            let values = f64s(bits);
            let mut m = MatrixTransform::new();
            m.set_matrix(&values[..16].try_into().expect("16 values"));
            m.set_offset(&values[16..].try_into().expect("4 offsets"));
            group.append_transform(m.into());
        }
        cs.set_transform(
            Some(&Transform::from(group)),
            ColorSpaceDirection::ToReference,
        )
        .expect("setTransform");
    }
    if spec.get("allocations_bits").is_some() {
        let mut group = GroupTransform::new();
        for bits in list(spec, "allocations_bits") {
            let mut t = AllocationTransform::new();
            t.set_allocation(Allocation::Lg2);
            t.set_vars(&f32s(bits));
            group.append_transform(t.into());
        }
        cs.set_transform(
            Some(&Transform::from(group)),
            ColorSpaceDirection::FromReference,
        )
        .expect("setTransform");
    }
    cs
}

/// The oracle's `build_config`, in an empty environment (`Config()` reads its active and
/// inactive lists from the environment).
pub(crate) fn build_config(spec: &Value) -> Config {
    set_thread_env_provider(Some(Arc::new(MapEnv::from_entries::<&str, &str>(&[]))));
    let mut config = Config::new().expect("Config()");
    set_thread_env_provider(None);
    if let Some(v) = spec.get("name") {
        config.set_name(bytes(v));
    }
    if let Some(v) = spec.get("description") {
        config.set_description(bytes(v));
    }
    for pair in list(spec, "environment") {
        config.add_environment_var(bytes(&pair[0]), Some(&bytes(&pair[1])));
    }
    for path in list(spec, "search_paths") {
        config.add_search_path(bytes(path));
    }
    if let Some(v) = spec.get("family_separator") {
        config
            .set_family_separator(bytes(v)[0])
            .expect("setFamilySeparator");
    }
    if let Some(bits) = spec.get("luma_bits") {
        let luma = f64s(bits);
        config.set_default_luma_coefs(&luma[..].try_into().expect("3 coefficients"));
    }
    for cs in list(spec, "colorspaces") {
        config
            .add_color_space(&build_color_space(cs))
            .expect("addColorSpace");
    }
    for pair in list(spec, "roles") {
        config
            .set_role(bytes(&pair[0]), Some(&bytes(&pair[1])))
            .expect("setRole");
    }
    for display in list(spec, "displays") {
        let name = bytes(&display[0]);
        for view in display[1].as_array().expect("views") {
            let description = view.get("description").map(bytes).unwrap_or_default();
            config
                .add_display_view_with_view_transform(
                    &name,
                    bytes(&view["name"]),
                    b"",
                    bytes(&view["colorspace"]),
                    b"",
                    b"",
                    description,
                )
                .expect("addDisplayView");
        }
    }
    if let Some(v) = spec.get("active_displays") {
        config
            .set_active_displays(bytes(v))
            .expect("setActiveDisplays");
    }
    if let Some(v) = spec.get("active_views") {
        config.set_active_views(bytes(v)).expect("setActiveViews");
    }
    if let Some(v) = spec.get("inactive_colorspaces") {
        config.set_inactive_color_spaces(bytes(v));
    }
    if spec.get("file_rules").is_some() {
        let mut rules = FileRules::new();
        for (i, rule) in list(spec, "file_rules").iter().enumerate() {
            rules
                .insert_rule(
                    i,
                    bytes(&rule["name"]),
                    bytes(&rule["colorspace"]),
                    bytes(&rule["pattern"]),
                    bytes(&rule["extension"]),
                )
                .expect("insertRule");
            for pair in list(rule, "custom") {
                rules
                    .set_custom_key(i, bytes(&pair[0]), bytes(&pair[1]))
                    .expect("setCustomKey");
            }
        }
        config.set_file_rules(&rules);
    }
    if spec.get("viewing_rules").is_some() {
        let mut rules = ViewingRules::new();
        for (i, rule) in list(spec, "viewing_rules").iter().enumerate() {
            rules
                .insert_rule(i, bytes(&rule["name"]))
                .expect("insertRule");
            for cs in list(rule, "colorspaces") {
                rules.add_color_space(i, bytes(cs)).expect("addColorSpace");
            }
            for pair in list(rule, "custom") {
                rules
                    .set_custom_key(i, bytes(&pair[0]), bytes(&pair[1]))
                    .expect("setCustomKey");
            }
        }
        config.set_viewing_rules(&rules);
    }
    config
}
