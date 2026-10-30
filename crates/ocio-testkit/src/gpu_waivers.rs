// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Owner-approved non-finite class and individual exact finite GPU pins.
//! This metadata grants nothing alone; the execution harness enforces every check.

use serde::Deserialize;

mod exact_pin;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Renderer {
    vendor: String,
    renderer: String,
    version: String,
    shading_language_version: String,
    egl_vendor: String,
    egl_version: String,
    max_texture_size: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NonfiniteClass {
    approved: bool,
    platform: String,
    ocio_version: String,
    language: String,
    width: usize,
    height: usize,
    components: usize,
    image_filter: String,
    image_wrap: String,
    finite_footprint_checks: bool,
    complete_finite_companion: bool,
    native_readback_required: bool,
    renderer: Renderer,
}

fn nonfinite_rule(
    entry: &toml::Value,
    raw: &toml::Value,
) -> Result<Option<serde_json::Value>, String> {
    let rule: NonfiniteClass = raw
        .clone()
        .try_into()
        .map_err(|e| format!("GPU non-finite class schema: {e}"))?;
    if entry.get("id").and_then(toml::Value::as_str) != Some("W0013")
        || rule.platform != "linux"
        || rule.ocio_version != "2.5.2"
        || rule.language != "GPU_LANGUAGE_GLSL_1_2"
        || (rule.width, rule.height, rule.components) != (256, 256, 4)
        || rule.image_filter != "GL_LINEAR"
        || rule.image_wrap != "GL_CLAMP"
        || !rule.finite_footprint_checks
        || !rule.complete_finite_companion
        || !rule.native_readback_required
        || rule.renderer.vendor != "Mesa"
        || rule.renderer.renderer != "llvmpipe (LLVM 21.1.8, 256 bits)"
        || rule.renderer.version != "4.5 (Compatibility Profile) Mesa 25.2.7"
        || rule.renderer.shading_language_version != "4.50"
        || rule.renderer.egl_vendor != "Mesa Project"
        || rule.renderer.egl_version != "1.5"
        || rule.renderer.max_texture_size != 16384
    {
        return Err("GPU non-finite class has unsupported scope or missing safeguards".into());
    }
    if !rule.approved {
        return Ok(None);
    }
    for field in ["approved_by", "date"] {
        if entry
            .get(field)
            .and_then(toml::Value::as_str)
            .is_none_or(|v| v.trim().is_empty())
        {
            return Err("GPU non-finite class lacks owner provenance".into());
        }
    }
    let mut value = serde_json::to_value(raw).map_err(|e| e.to_string())?;
    value["id"] = serde_json::Value::String("W0013".into());
    value["kind"] = serde_json::Value::String("gpu_nonfinite".into());
    Ok(Some(value))
}

/// Existing unpublished test-helper signature; the class applies to every family.
/// The harness must enforce GPU identity, finite footprints, complete finite
/// companions, native readback and actual renderer/sampling state. Retired Matrix
/// rules remain rejected. The five separately approved finite pins require exact
/// test identity, original settings, historical outputs and their own safeguards.
pub fn gpu_renderer_waivers(test_id: &str) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::paths::workspace_root().join("waivers.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text, test_id).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse(text: &str, test_id: &str) -> Result<Vec<serde_json::Value>, String> {
    let doc: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let entries = doc
        .get("waiver")
        .and_then(toml::Value::as_array)
        .ok_or("No waiver array")?;
    let mut out = Vec::new();
    let mut class_seen = false;
    let mut pin_tests = std::collections::BTreeSet::new();
    for entry in entries {
        if entry.get("gpu_renderer").is_some() {
            return Err(
                "Per-test GPU renderer rules were retired after debug/release class validation"
                    .into(),
            );
        }
        if let Some(raw) = entry.get("gpu_nonfinite") {
            if class_seen {
                return Err("Duplicate GPU non-finite class".into());
            }
            class_seen = true;
            if let Some(rule) = nonfinite_rule(entry, raw)? {
                out.push(rule);
            }
        }
        if let Some(raw) = entry.get("gpu_exact_pin") {
            if entry.get("gpu_nonfinite").is_some() {
                return Err("An exact GPU pin cannot also be a class rule".into());
            }
            let test = raw
                .get("test_id")
                .and_then(toml::Value::as_str)
                .ok_or("Exact GPU pin lacks test id")?;
            if !pin_tests.insert(test.to_owned()) {
                return Err("Duplicate exact GPU test pin".into());
            }
            if let Some(rule) = exact_pin::rule(entry, raw, test_id)? {
                out.push(rule);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse;
    fn document(doc: &toml::Value) -> String {
        toml::to_string(doc).unwrap()
    }

    #[test]
    fn nonfinite_class_requires_each_safeguard_and_owner_provenance() {
        let original: toml::Value = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../waivers.toml"
        )))
        .unwrap();
        let class = original["waiver"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"].as_str() == Some("W0013"))
            .unwrap()
            .clone();
        let class_doc = |entry: toml::Value| {
            let mut d = toml::Table::new();
            d.insert("waiver".into(), toml::Value::Array(vec![entry]));
            toml::Value::Table(d)
        };
        let doc = class_doc(class.clone());
        assert_eq!(
            parse(&document(&doc), "any/family").unwrap()[0]["id"],
            "W0013"
        );
        for field in [
            "finite_footprint_checks",
            "complete_finite_companion",
            "native_readback_required",
        ] {
            let mut changed = class.clone();
            changed["gpu_nonfinite"][field] = toml::Value::Boolean(false);
            assert!(
                parse(&document(&class_doc(changed)), "any/family").is_err(),
                "{field}"
            );
        }
        let mut changed = class.clone();
        changed["approved_by"] = toml::Value::String(String::new());
        assert!(parse(&document(&class_doc(changed)), "any/family").is_err());
        let mut changed = class.clone();
        changed["gpu_nonfinite"]["renderer"]["version"] = toml::Value::String("different".into());
        assert!(parse(&document(&class_doc(changed)), "any/family").is_err());
        let mut changed = class;
        changed["gpu_nonfinite"]["approved"] = toml::Value::Boolean(false);
        assert!(
            parse(&document(&class_doc(changed)), "any/family")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn retired_matrix_rules_are_archived_and_cannot_be_reactivated() {
        let archived = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/archive/gpu-matrix-waivers-2026-10-27.toml"
        ));
        let doc: toml::Value = toml::from_str(archived).unwrap();
        assert_eq!(doc["waiver"].as_array().unwrap().len(), 10);
        assert!(
            parse(archived, "MatrixOps/matrix")
                .unwrap_err()
                .contains("retired")
        );
        let active = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../waivers.toml"));
        for id in [
            "MatrixOps/matrix",
            "RangeOps/arbitrary_1_no_clamp",
            "any/family",
        ] {
            let rules = parse(active, id).unwrap();
            assert_eq!(rules.len(), 1);
            assert_eq!(rules[0]["id"], "W0013");
            assert_eq!(rules[0]["kind"], "gpu_nonfinite");
        }
    }

    #[test]
    fn unknown_fields_and_duplicate_classes_fail_closed() {
        let active = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../waivers.toml"));
        let mut doc: toml::Value = toml::from_str(active).unwrap();
        let entries = doc["waiver"].as_array_mut().unwrap();
        let class = entries
            .iter()
            .find(|e| e.get("gpu_nonfinite").is_some())
            .unwrap()
            .clone();
        entries.push(class);
        assert!(
            parse(&document(&doc), "any/family")
                .unwrap_err()
                .contains("Duplicate")
        );
        let mut doc: toml::Value = toml::from_str(active).unwrap();
        let class = doc["waiver"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e.get("gpu_nonfinite").is_some())
            .unwrap();
        class["gpu_nonfinite"]
            .as_table_mut()
            .unwrap()
            .insert("unknown_field".into(), toml::Value::Boolean(true));
        assert!(parse(&document(&doc), "any/family").is_err());
    }
}
