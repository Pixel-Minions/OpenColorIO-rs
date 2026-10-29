// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! P7-6 owner-approved GPU renderer manifest lookup (2026-10-27).
//! No pixel comparison is implemented here. The execution harness enforces scope.

use serde::Deserialize;

// Upstream MatrixOp_test.cpp order; only the final two use a generic shader.
const TEST_IDS: [&str; 10] = [
    "MatrixOps/matrix",
    "MatrixOps/scale",
    "MatrixOps/offset",
    "MatrixOps/matrix_offset",
    "MatrixOps/matrix_inverse",
    "MatrixOps/scale_inverse",
    "MatrixOps/offset_inverse",
    "MatrixOps/matrix_offset_inverse",
    "MatrixOps/matrix_offset_generic_shader",
    "MatrixOps/matrix_offset_inverse_generic_shader",
];

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
struct Row {
    index: usize,
    input: String,
    cpu: String,
    gpu: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    approved: bool,
    test_id: String,
    processor_cache_id: String,
    platform: String,
    ocio_version: String,
    source: String,
    input_sha256: String,
    width: usize,
    height: usize,
    components: usize,
    entries: usize,
    original_entries: usize,
    run: usize,
    retests: usize,
    threshold_bits: String,
    minimal_value_bits: String,
    relative: bool,
    print_min_max: bool,
    nan: bool,
    infinity: bool,
    wide: bool,
    range_min_bits: String,
    range_max_bits: String,
    legacy: bool,
    legacy_edge: u32,
    shader_settings_json: String,
    evidence_sha256: String,
    log_sha256: String,
    nan_mismatches: Vec<Row>,
    renderer: Renderer,
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

fn hex(text: &str, digits: usize) -> bool {
    text.len() == digits
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate(rule: &Rule) -> Result<(), String> {
    if rule.test_id.is_empty()
        || !hex(&rule.processor_cache_id, 32)
        || rule.platform != "linux"
        || rule.ocio_version != "2.5.2"
        || rule.source.is_empty()
        || !hex(&rule.input_sha256, 64)
        || !hex(&rule.evidence_sha256, 64)
        || !hex(&rule.log_sha256, 64)
        || rule.width != 256
        || rule.height != 256
        || rule.components != 4
        || rule.entries != 262_144
        || rule.original_entries != rule.entries
        || rule.run != 0
        || rule.retests != 0
        || rule.threshold_bits != "350637bd"
        || rule.minimal_value_bits != "358637bd"
        || rule.relative
        || rule.print_min_max
        || !rule.nan
        || !rule.infinity
        || !rule.wide
        || rule.range_min_bits != "bf800000"
        || rule.range_max_bits != "40000000"
        || rule.legacy_edge != 32
        || rule.shader_settings_json
            != r#"{"language":"GPU_LANGUAGE_GLSL_1_2","pixel_name":"myPixel"}"#
        || rule.nan_mismatches.is_empty()
        || rule.renderer.vendor.is_empty()
        || rule.renderer.renderer.is_empty()
        || rule.renderer.version.is_empty()
        || rule.renderer.shading_language_version.is_empty()
        || rule.renderer.egl_vendor.is_empty()
        || rule.renderer.egl_version.is_empty()
        || rule.renderer.max_texture_size == 0
    {
        return Err("P7-6 renderer rule has invalid or broadened scope".into());
    }
    let position = TEST_IDS
        .iter()
        .position(|id| *id == rule.test_id)
        .ok_or("P7-6 proposal has an unapproved test ID")?;
    let legacy = position < 8;
    if rule.legacy != legacy {
        return Err("P7-6 proposal has a changed legacy/generic route".into());
    }
    let mut previous = None;
    for row in &rule.nan_mismatches {
        if row.index >= rule.entries
            || previous.is_some_and(|last| last >= row.index)
            || !hex(&row.input, 8)
            || !hex(&row.cpu, 8)
            || !hex(&row.gpu, 8)
        {
            return Err(
                "P7-6 component tuples must be in-range, sorted, unique and exact hex".into(),
            );
        }
        let cpu = u32::from_str_radix(&row.cpu, 16).map_err(|e| e.to_string())?;
        let gpu = u32::from_str_radix(&row.gpu, 16).map_err(|e| e.to_string())?;
        if f32::from_bits(cpu).is_nan() || !f32::from_bits(gpu).is_nan() {
            return Err(
                "P7-6 proposal only covers the observed CPU non-NaN to GPU NaN direction".into(),
            );
        }
        previous = Some(row.index);
    }
    Ok(())
}

/// Unpublished test-helper signature approved by the project owner on 2026-10-27.
/// Reads owner-approved, schema-checked rules: exact legacy test IDs plus the
/// single non-finite class approved on 2026-10-29. The latter has kind gpu_nonfinite.
/// The private GPU execution harness must still check renderer, all settings,
/// complete mismatch set and every tuple's bits; this function grants nothing alone.
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
    let mut ids = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for entry in entries {
        if let Some(raw) = entry.get("gpu_nonfinite") {
            if !ids.insert("W0013".to_owned()) {
                return Err("Duplicate GPU non-finite class".into());
            }
            if let Some(rule) = nonfinite_rule(entry, raw)? {
                out.push(rule);
            }
            continue;
        }
        let Some(raw) = entry.get("gpu_renderer") else {
            continue;
        };
        let rule: Rule = raw
            .clone()
            .try_into()
            .map_err(|e| format!("P7-6 schema: {e}"))?;
        validate(&rule)?;
        let id = entry
            .get("id")
            .and_then(toml::Value::as_str)
            .ok_or("P7-6 waiver has no ID")?;
        let position = TEST_IDS
            .iter()
            .position(|test| *test == rule.test_id)
            .ok_or("Unknown P7-6 test")?;
        if id != format!("W{:04}", position + 3) {
            return Err("P7-6 waiver ID does not match its reserved test".into());
        }
        if !ids.insert(id.to_owned()) {
            return Err(format!("Duplicate P7-6 waiver ID {id}"));
        }
        if rule.test_id != test_id || !rule.approved {
            continue;
        }
        let signer = entry
            .get("approved_by")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        let date = entry
            .get("date")
            .and_then(toml::Value::as_str)
            .unwrap_or("");
        if signer.trim().is_empty() || date.trim().is_empty() {
            return Err(format!("P7-6 {id} has no owner approval provenance"));
        }
        let mut value = serde_json::to_value(raw).map_err(|e| e.to_string())?;
        value["id"] = serde_json::Value::String(id.to_owned());
        out.push(value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn document(doc: &toml::Value) -> String {
        // Value's Display is an inline table, not a complete TOML document.
        toml::to_string(doc).unwrap()
    }

    // Only policy metadata is edited in memory. No actual manifest/approval is changed.
    fn fixture() -> toml::Value {
        let mut doc: toml::Value = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../waivers.toml"
        )))
        .unwrap();
        let entries = doc["waiver"].as_array_mut().unwrap();
        entries.retain(|entry| entry.get("gpu_nonfinite").is_none());
        let entry = entries
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["approved_by"] =
            toml::Value::String("policy-test metadata, not owner approval".into());
        entry["date"] = toml::Value::String("2026-10-27".into());
        entry["gpu_renderer"]["approved"] = toml::Value::Boolean(true);
        // Every fresh fixture must reach the actual rule before any negative
        // mutation; a serialization/parser failure is not guard coverage.
        let rules = parse(&document(&doc), "MatrixOps/matrix").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], "W0003");
        assert_eq!(rules[0]["test_id"], "MatrixOps/matrix");
        doc
    }

    #[test]
    fn approval_and_exact_test_id_are_mandatory() {
        let mut doc = fixture();
        assert_eq!(parse(&document(&doc), "MatrixOps/matrix").unwrap().len(), 1);
        assert!(
            parse(&document(&doc), "Infrastructure/extracted GPU property")
                .unwrap()
                .is_empty()
        );
        let entry = doc["waiver"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["gpu_renderer"]["approved"] = toml::Value::Boolean(false);
        assert!(
            parse(&document(&doc), "MatrixOps/matrix")
                .unwrap()
                .is_empty()
        );
        let entry = doc["waiver"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["gpu_renderer"]["approved"] = toml::Value::Boolean(true);
        entry["approved_by"] = toml::Value::String(String::new());
        assert!(parse(&document(&doc), "MatrixOps/matrix").is_err());
    }

    #[test]
    fn unknown_fields_duplicate_ids_and_reordered_tuples_fail() {
        let mut doc = fixture();
        let entries = doc["waiver"].as_array_mut().unwrap();
        let entry = entries
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["gpu_renderer"]
            .as_table_mut()
            .unwrap()
            .insert("legacy_typo".into(), toml::Value::Boolean(true));
        assert!(parse(&document(&doc), "MatrixOps/matrix").is_err());
        let mut doc = fixture();
        let entry = doc["waiver"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["gpu_renderer"]["processor_cache_id"] = toml::Value::String("not32hex".into());
        assert!(parse(&document(&doc), "MatrixOps/matrix").is_err());
        let mut doc = fixture();
        let entries = doc["waiver"].as_array_mut().unwrap();
        let duplicate = entries
            .iter()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap()
            .clone();
        entries.push(duplicate);
        assert!(parse(&document(&doc), "MatrixOps/matrix").is_err());
        let mut doc = fixture();
        let entry = doc["waiver"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"].as_str() == Some("W0003"))
            .unwrap();
        entry["gpu_renderer"]["nan_mismatches"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(parse(&document(&doc), "MatrixOps/matrix").is_err());
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
}
