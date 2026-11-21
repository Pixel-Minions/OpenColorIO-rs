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
    ocio_version: Vec<String>,
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

/// Whether a rule's `ocio_version` list holds the running wheel's version: a list of distinct,
/// non-empty versions, or an error. A rule for other releases is inactive (the test then fails
/// as it would without it).
fn applies_to(versions: &[String], wheel_version: &str) -> Result<bool, String> {
    let distinct: std::collections::BTreeSet<&String> = versions.iter().collect();
    if versions.is_empty()
        || distinct.len() != versions.len()
        || versions.iter().any(|v| v.is_empty())
    {
        return Err("GPU waiver ocio_version must list distinct OCIO versions".into());
    }
    Ok(versions.iter().any(|v| v == wheel_version))
}

fn nonfinite_rule(
    entry: &toml::Value,
    raw: &toml::Value,
    wheel_version: &str,
) -> Result<Option<serde_json::Value>, String> {
    let rule: NonfiniteClass = raw
        .clone()
        .try_into()
        .map_err(|e| format!("GPU non-finite class schema: {e}"))?;
    let applies = applies_to(&rule.ocio_version, wheel_version)?;
    if entry.get("id").and_then(toml::Value::as_str) != Some("W0013")
        || rule.platform != "linux"
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
    if !rule.approved || !applies {
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
/// Each rule applies only to the OCIO releases its `ocio_version` lists, compared with the
/// running wheel's version (the oracle's `info`).
pub fn gpu_renderer_waivers(test_id: &str) -> Result<Vec<serde_json::Value>, String> {
    let path = crate::paths::workspace_root().join("waivers.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_for_version(&text, test_id, &wheel_version()?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The running wheel's `OCIO.__version__`.
fn wheel_version() -> Result<String, String> {
    static VERSION: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            let info = crate::Oracle::get()
                .try_call("info", serde_json::json!({}), &[])
                .map_err(|e| format!("the oracle's info: {e}"))?;
            info.result["ocio_version"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("the oracle's info has no ocio_version: {}", info.result))
        })
        .clone()
}

/// [`parse_for_version`] for 2.5.2, the release every rule lists.
#[cfg(test)]
fn parse(text: &str, test_id: &str) -> Result<Vec<serde_json::Value>, String> {
    parse_for_version(text, test_id, "2.5.2")
}

fn parse_for_version(
    text: &str,
    test_id: &str,
    wheel_version: &str,
) -> Result<Vec<serde_json::Value>, String> {
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
            if let Some(rule) = nonfinite_rule(entry, raw, wheel_version)? {
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
            if let Some(rule) = exact_pin::rule(entry, raw, test_id, wheel_version)? {
                out.push(rule);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{parse, parse_for_version};
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

    /// A rule applies only to the wheel versions its `ocio_version` lists; an empty list or one
    /// that repeats a version is an error.
    #[test]
    fn rules_apply_only_to_the_wheel_versions_they_list() {
        let active = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../waivers.toml"));
        let pinned = "FixedFunction/style_aces2_nan_bug";
        for test in ["any/family", pinned] {
            assert!(!parse_for_version(active, test, "2.5.2").unwrap().is_empty());
            assert!(
                parse_for_version(active, test, "0.0.0").unwrap().is_empty(),
                "{test}"
            );
        }
        // The 2.6.0 re-check (owner, 2026-11-18): the class and three pins extended; the pins
        // of the two tests whose 2.6.0 thresholds they pass, W0015 and W0018, not.
        for (test, extended) in [
            ("any/family", true),
            ("FixedFunction/style_aces2_nan_bug", true),
            ("FixedFunction/style_aces2_rec709_rndtrip", true),
            ("FixedFunction/style_aces2_rgb_to_jmh_fwd", true),
            ("FixedFunction/style_aces2_4000nit_p3_rndtrip", false),
            ("FixedFunction/style_aces_gamutcomp13_inv", false),
        ] {
            let ids: Vec<_> = parse_for_version(active, test, "2.6.0")
                .unwrap()
                .iter()
                .map(|r| r["kind"].as_str().unwrap().to_owned())
                .collect();
            let pinned = ids.iter().any(|k| k == "gpu_exact_pin");
            assert_eq!(
                pinned || test == "any/family" && ids.iter().any(|k| k == "gpu_nonfinite"),
                extended,
                "{test}: {ids:?}"
            );
        }
        let doc: toml::Value = toml::from_str(active).unwrap();
        for (kind, test) in [("gpu_nonfinite", "any/family"), ("gpu_exact_pin", pinned)] {
            for versions in [vec![], vec!["2.5.2", "2.5.2"], vec![""]] {
                let mut doc = doc.clone();
                for entry in doc["waiver"].as_array_mut().unwrap() {
                    if let Some(rule) = entry.get_mut(kind) {
                        rule["ocio_version"] = toml::Value::Array(
                            versions
                                .iter()
                                .map(|v| toml::Value::String((*v).into()))
                                .collect(),
                        );
                    }
                }
                assert!(
                    parse_for_version(&document(&doc), test, "2.5.2").is_err(),
                    "{kind} {versions:?}"
                );
            }
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
