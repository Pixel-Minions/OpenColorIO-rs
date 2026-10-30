// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::super::parse;

fn entries() -> Vec<toml::Value> {
    let doc: toml::Value = toml::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../waivers.toml"
    )))
    .unwrap();
    doc["waiver"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("gpu_exact_pin").is_some())
        .cloned()
        .collect()
}

fn document(entries: Vec<toml::Value>) -> String {
    let mut doc = toml::Table::new();
    doc.insert("waiver".into(), toml::Value::Array(entries));
    toml::to_string(&doc).unwrap()
}

#[test]
fn only_the_five_owner_approved_observed_failures_are_selected() {
    // Expected failure bits/indices are the original C++ comparator's captured outputs
    // (CI41017434341), not calculated from port renderer output.
    let expected = [
        ("W0014", "style_aces2_nan_bug", "3f5691a0", 2),
        ("W0015", "style_aces2_4000nit_p3_rndtrip", "3c96534b", 19448),
        ("W0016", "style_aces2_rec709_rndtrip", "3b92de74", 1090),
        ("W0017", "style_aces2_rgb_to_jmh_fwd", "39700000", 46),
        ("W0018", "style_aces_gamutcomp13_inv", "37bc0000", 14),
    ];
    let text = document(entries());
    assert_eq!(entries().len(), expected.len());
    for (id, name, bits, component) in expected {
        let rules = parse(&text, &format!("FixedFunction/{name}")).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["id"], id);
        assert_eq!(rules[0]["kind"], "gpu_exact_pin");
        assert_eq!(rules[0]["max_diff_bits"], bits);
        assert_eq!(rules[0]["component"], component);
    }
    assert!(
        parse(&text, "FixedFunction/a_different_test")
            .unwrap()
            .is_empty()
    );
    let nan = parse(&text, "FixedFunction/style_aces2_nan_bug").unwrap();
    assert_eq!(
        nan[0]["gpu_rgb_bits"],
        serde_json::json!(["4389b37a", "444b9830", "4470eaee"])
    );
    assert_eq!(
        nan[0]["cpu_rgb_bits"],
        serde_json::json!(["44739272", "440e0021", "45ba139c"])
    );
}

#[test]
fn exact_pin_schema_rejects_broadened_scope_or_missing_safeguards() {
    let original = entries().remove(0);
    let test = "FixedFunction/style_aces2_nan_bug";
    for field in [
        "approved",
        "whole_image_gpu_identity_required",
        "native_readback_required",
        "reject_passing",
    ] {
        let mut changed = original.clone();
        changed["gpu_exact_pin"][field] = toml::Value::Boolean(false);
        assert!(parse(&document(vec![changed]), test).is_err(), "{field}");
    }
    for (field, value) in [
        ("test_id", "FixedFunction/other"),
        ("platform", "any"),
        ("language", "GPU_LANGUAGE_GLSL_4_0"),
        ("max_diff_bits", "not-bits"),
        ("gpu_sha256", "not-a-digest"),
    ] {
        let mut changed = original.clone();
        changed["gpu_exact_pin"][field] = toml::Value::String(value.into());
        assert!(parse(&document(vec![changed]), test).is_err(), "{field}");
    }
    let mut changed = original.clone();
    changed["gpu_exact_pin"]["renderer"]["version"] = toml::Value::String("other Mesa".into());
    assert!(parse(&document(vec![changed]), test).is_err());
    let mut changed = original.clone();
    changed["approved_by"] = toml::Value::String(String::new());
    assert!(parse(&document(vec![changed]), test).is_err());
    let mut changed = original.clone();
    changed["gpu_exact_pin"]
        .as_table_mut()
        .unwrap()
        .insert("unknown".into(), toml::Value::Boolean(true));
    assert!(parse(&document(vec![changed]), test).is_err());
    assert!(
        parse(&document(vec![original.clone(), original]), test)
            .unwrap_err()
            .contains("Duplicate")
    );
}
