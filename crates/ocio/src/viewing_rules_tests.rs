// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/ViewingRules_tests.cpp` @ v2.5.2; `crates/ocio/tests/
//! viewing_rules_oracle.rs` checks the rules against the wheel.

use ocio_testkit::upstream::check_throw_what;

use super::*;

/// Port of `OCIO_ADD_TEST(ViewingRules, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut vrules = ViewingRules::new();
    assert_eq!(vrules.num_entries(), 0);

    // Rules have to exist to be accessed.
    check_throw_what(vrules.name(0), "Viewing rules: rule index '0' invalid.");
    check_throw_what(
        vrules.insert_rule(1, "test"),
        "Viewing rules: rule index '1' invalid.",
    );
    // New rules must have a name.
    check_throw_what(
        vrules.insert_rule(0, ""),
        "Viewing rules: rule must have a non-empty name.",
    );
    // Upstream's null pointer is an empty name here (`name ? name : ""`).
    check_throw_what(
        vrules.insert_rule(0, b"" as &[u8]),
        "Viewing rules: rule must have a non-empty name.",
    );

    // Add rules.
    let rule_name0 = "Rule0";
    vrules.insert_rule(0, rule_name0).unwrap();

    let rule_name2 = "Rule2";
    vrules.insert_rule(1, rule_name2).unwrap();

    // Adding Rule1 at index 1 will move Rule2 at index 2.
    let rule_name1 = "Rule1";
    vrules.insert_rule(1, rule_name1).unwrap();

    assert_eq!(vrules.num_entries(), 3);
    assert_eq!(vrules.name(0).unwrap(), rule_name0.as_bytes());
    assert_eq!(vrules.name(1).unwrap(), rule_name1.as_bytes());
    assert_eq!(vrules.name(2).unwrap(), rule_name2.as_bytes());

    // Only have 3 rules, index 3 is invalid.
    check_throw_what(vrules.name(3), "Viewing rules: rule index '3' invalid.");

    // Rule names are unique.
    check_throw_what(
        vrules.insert_rule(1, rule_name1),
        "A rule named 'Rule1' already exists",
    );

    // Check added rules properties are empty.
    for r in 0..3 {
        assert_eq!(vrules.num_color_spaces(r).unwrap(), 0);
        assert_eq!(vrules.num_encodings(r).unwrap(), 0);
        assert_eq!(vrules.num_custom_keys(r).unwrap(), 0);
    }

    // Set colorspaces and verify.
    let cs0 = "colorspace0";
    let cs1 = "colorspace1";
    vrules.add_color_space(0, cs0).unwrap();
    vrules.add_color_space(0, cs1).unwrap();
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 2);
    assert_eq!(vrules.color_space(0, 0).unwrap(), Some(cs0.as_bytes()));
    assert_eq!(vrules.color_space(0, 1).unwrap(), Some(cs1.as_bytes()));
    // Can not access non existing colorspaces.
    check_throw_what(
        vrules.color_space(0, 2),
        "rule 'Rule0' at index '0': colorspace index '2' is invalid.",
    );
    // Remove color space.
    check_throw_what(
        vrules.remove_color_space(3, 0),
        "Viewing rules: rule index '3' invalid.",
    );
    check_throw_what(
        vrules.remove_color_space(0, 2),
        "rule 'Rule0' at index '0': colorspace index '2' is invalid.",
    );
    vrules.remove_color_space(0, 0).unwrap();
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 1);
    assert_eq!(vrules.color_space(0, 0).unwrap(), Some(cs1.as_bytes()));
    // Re-add color space.
    vrules.add_color_space(0, cs0).unwrap();

    // Same with encodings.
    let enc0 = "encoding0";
    let enc1 = "encoding1";
    check_throw_what(
        vrules.add_encoding(0, enc0),
        "encoding can't be added if there are colorspaces.",
    );
    vrules.add_encoding(1, enc0).unwrap();
    vrules.add_encoding(1, enc1).unwrap();
    check_throw_what(
        vrules.add_color_space(1, cs0),
        "colorspace can't be added if there are encodings.",
    );
    assert_eq!(vrules.num_encodings(1).unwrap(), 2);
    assert_eq!(vrules.encoding(1, 0).unwrap(), Some(enc0.as_bytes()));
    assert_eq!(vrules.encoding(1, 1).unwrap(), Some(enc1.as_bytes()));
    check_throw_what(
        vrules.encoding(1, 2),
        "rule 'Rule1' at index '1': encoding index '2' is invalid.",
    );
    // Remove encoding.
    check_throw_what(
        vrules.remove_encoding(3, 0),
        "Viewing rules: rule index '3' invalid.",
    );
    check_throw_what(
        vrules.remove_encoding(1, 2),
        "rule 'Rule1' at index '1': encoding index '2' is invalid.",
    );
    vrules.remove_encoding(1, 0).unwrap();
    assert_eq!(vrules.num_encodings(1).unwrap(), 1);
    assert_eq!(vrules.encoding(1, 0).unwrap(), Some(enc1.as_bytes()));
    // Re-add encoding.
    vrules.add_encoding(1, enc0).unwrap();

    // Same with custom keys.
    let key0 = "key0";
    let value0 = "value0";
    let key1 = "key1";
    let value1 = "value1";
    vrules.set_custom_key(0, key0, value0).unwrap();
    vrules.set_custom_key(0, key1, value1).unwrap();
    assert_eq!(vrules.num_custom_keys(0).unwrap(), 2);
    assert_eq!(vrules.custom_key_name(0, 0).unwrap(), key0.as_bytes());
    assert_eq!(vrules.custom_key_value(0, 0).unwrap(), value0.as_bytes());
    assert_eq!(vrules.custom_key_name(0, 1).unwrap(), key1.as_bytes());
    assert_eq!(vrules.custom_key_value(0, 1).unwrap(), value1.as_bytes());
    check_throw_what(
        vrules.custom_key_name(0, 2),
        "rule named 'Rule0' error: Key index '2' is invalid",
    );
    check_throw_what(
        vrules.custom_key_value(0, 2),
        "rule named 'Rule0' error: Key index '2' is invalid",
    );

    let newvalue0 = "newvalue0";
    vrules.set_custom_key(0, key0, newvalue0).unwrap();
    assert_eq!(vrules.num_custom_keys(0).unwrap(), 2);
    assert_eq!(vrules.custom_key_value(0, 0).unwrap(), newvalue0.as_bytes());

    // Test serialization.
    const OUTPUT: &str = "<ViewingRule name=Rule0, colorspaces=[colorspace1, colorspace0], \
                          customKeys=[(key0, newvalue0), (key1, value1)]>\n\
                          <ViewingRule name=Rule1, encodings=[encoding1, encoding0]>\n\
                          <ViewingRule name=Rule2>";
    assert_eq!(vrules.to_bytes(), OUTPUT.as_bytes());

    // Removing a rule: throws if index is not valid.
    let num_rules = vrules.num_entries();
    check_throw_what(
        vrules.remove_rule(num_rules),
        "rule index '3' invalid. There are only '3' rules",
    );
    assert_eq!(vrules.num_entries(), num_rules);
    assert_eq!(vrules.num_color_spaces(0).unwrap(), 2);
    assert_eq!(vrules.num_encodings(1).unwrap(), 2);

    // Remove rule, and check it is gone.
    vrules.remove_rule(1).unwrap();
    assert_eq!(vrules.num_entries(), 2);
    assert_eq!(vrules.name(0).unwrap(), rule_name0.as_bytes());
    assert_eq!(vrules.name(1).unwrap(), rule_name2.as_bytes());

    // Get index of a rule.
    assert_eq!(0, vrules.index_for_rule(rule_name0).unwrap());
    assert_eq!(1, vrules.index_for_rule(rule_name2).unwrap());
    check_throw_what(
        vrules.index_for_rule("I am not there"),
        "rule name 'I am not there' not found",
    );
}

/// Port of `OCIO_ADD_TEST(ViewingRules, config_io)` @ v2.5.2.
#[test]
fn config_io() {
    let _env = crate::test_env::EnvGuard::new();
    // Create a config with viewing rules.
    let mut config = (*crate::Config::create_raw().unwrap()).clone();

    let mut vrules = ViewingRules::new();
    let rule_name0 = "Rule0";
    vrules.insert_rule(0, rule_name0).unwrap();
    let rule_name1 = "Rule1";
    vrules.insert_rule(1, rule_name1).unwrap();

    let key0 = "key0";
    let value0 = "value0";
    let key1 = "key1";
    let value1 = "value1";
    vrules.set_custom_key(0, key0, value0).unwrap();
    vrules.set_custom_key(0, key1, value1).unwrap();

    let enc0 = "encoding0";
    let enc1 = "encoding1";
    vrules.add_encoding(1, enc0).unwrap();
    vrules.add_encoding(1, enc1).unwrap();

    // Rules have to refer to colorspace or encoding.
    config.set_viewing_rules(&vrules);
    check_throw_what(
        config.validate(),
        "must have either a color space or an encoding",
    );

    let cs0 = "colorspace0";
    vrules.add_color_space(0, cs0).unwrap();
    let mut cs = crate::ColorSpace::new();
    cs.set_name(cs0);
    config.add_color_space(&cs).unwrap();

    cs.set_name("cs_enc0");
    cs.set_encoding(enc0);
    config.add_color_space(&cs).unwrap();

    cs.set_name("cs_enc1");
    cs.set_encoding(enc1);
    config.add_color_space(&cs).unwrap();

    config.set_viewing_rules(&vrules);
    config.validate().unwrap();

    // Save config and load back.
    let config_str = config.serialize().unwrap();
    let config_back = crate::Config::create_from_stream(&config_str).unwrap();

    // Verify rules have been loaded.
    let vr = config_back.viewing_rules().get();
    assert_eq!(vr.num_entries(), 2);

    assert_eq!(vr.name(0).unwrap(), rule_name0.as_bytes());
    assert_eq!(vr.name(1).unwrap(), rule_name1.as_bytes());

    assert_eq!(vr.num_color_spaces(0).unwrap(), 1);
    assert_eq!(vr.num_encodings(0).unwrap(), 0);
    assert_eq!(vr.num_custom_keys(0).unwrap(), 2);

    assert_eq!(vr.color_space(0, 0).unwrap(), Some(cs0.as_bytes()));
    assert_eq!(vr.custom_key_name(0, 0).unwrap(), key0.as_bytes());
    assert_eq!(vr.custom_key_value(0, 0).unwrap(), value0.as_bytes());
    assert_eq!(vr.custom_key_name(0, 1).unwrap(), key1.as_bytes());
    assert_eq!(vr.custom_key_value(0, 1).unwrap(), value1.as_bytes());

    assert_eq!(vr.num_color_spaces(1).unwrap(), 0);
    assert_eq!(vr.num_encodings(1).unwrap(), 2);
    assert_eq!(vr.num_custom_keys(1).unwrap(), 0);
    assert_eq!(vr.encoding(1, 0).unwrap(), Some(enc0.as_bytes()));
    assert_eq!(vr.encoding(1, 1).unwrap(), Some(enc1.as_bytes()));
}

/// Port of `OCIO_ADD_TEST(ViewingRules, filtered_views)` @ v2.5.2.
#[test]
fn filtered_views() {
    let _env = crate::test_env::EnvGuard::new();
    const SIMPLE_CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw
  scene_linear: c3

file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: raw}

viewing_rules:
  - !<Rule> {name: Rule_1, colorspaces: c1}
  - !<Rule> {name: Rule_2, colorspaces: [c2, c3]}
  - !<Rule> {name: Rule_3, colorspaces: scene_linear}
  - !<Rule> {name: Rule_4, colorspaces: [c3, c4]}
  - !<Rule> {name: Rule_5, encodings: log}
  - !<Rule> {name: Rule_6, encodings: [log, video]}

shared_views:
  - !<View> {name: SView_a, colorspace: raw, rule: Rule_2}
  - !<View> {name: SView_b, colorspace: raw, rule: Rule_3}
  - !<View> {name: SView_c, colorspace: raw}
  - !<View> {name: SView_d, colorspace: raw, rule: Rule_5}
  - !<View> {name: SView_e, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: View_a, colorspace: raw, rule: Rule_1}
    - !<View> {name: View_b, colorspace: raw, rule: Rule_2}
    - !<View> {name: View_c, colorspace: raw, rule: Rule_2}
    - !<View> {name: View_d, colorspace: raw, rule: Rule_3}
    - !<View> {name: View_e, colorspace: raw, rule: Rule_4}
    - !<View> {name: View_f, colorspace: raw, rule: Rule_5}
    - !<View> {name: View_g, colorspace: raw, rule: Rule_6}
    - !<View> {name: View_h, colorspace: raw}
    - !<Views> [SView_a, SView_b, SView_d, SView_e]

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c1
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: video
    allocation: uniform

  - !<ColorSpace>
    name: c2
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c3
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: c4
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: log
    allocation: uniform

  - !<ColorSpace>
    name: c5
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: data
    allocation: uniform

  - !<ColorSpace>
    name: c6
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    encoding: video
    allocation: uniform
"#;

    let config = crate::Config::create_from_stream(SIMPLE_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Check 2 rules of 2 non-existing display/views.
    assert_eq!(config.display_view_rule("no", "unknown"), b"");
    assert_eq!(config.display_view_rule("sRGB", "unknown"), b"");
    // sRGB/View_b uses Rule_2.
    assert_eq!(config.display_view_rule("sRGB", "View_b"), b"Rule_2");

    // Access views by colorspace on non existing display: 0 and empty string.
    assert_eq!(
        config.num_views_for_color_space("no", "unknown").unwrap(),
        0
    );
    assert_eq!(
        config.view_for_color_space("no", "unknown", 0).unwrap(),
        b""
    );

    // When display exists, colorspace has to exist or it will throw.
    check_throw_what(
        config.num_views_for_color_space("sRGB", "unknown"),
        "Could not find source color space 'unknown'.",
    );
    check_throw_what(
        config.view_for_color_space("sRGB", "unknown", 0),
        "Could not find source color space 'unknown'.",
    );

    let view = |cs: &str, i: i32| config.view_for_color_space("sRGB", cs, i).unwrap().to_vec();

    // Views by existing colorspace on existing display.
    assert_eq!(config.num_views_for_color_space("sRGB", "c6").unwrap(), 3);
    // View_g rule is Rule_6 that lists encoding video, c6 has encoding video.
    assert_eq!(view("c6", 0), b"View_g");
    // View_h has no rule.
    assert_eq!(view("c6", 1), b"View_h");
    // Shared SView_e has no rule.
    assert_eq!(view("c6", 2), b"SView_e");
    // There is no 4th view: empty string.
    assert!(view("c6", 3).is_empty());

    assert_eq!(config.num_views_for_color_space("sRGB", "c3").unwrap(), 8);
    // View_b rule is Rule_2 that lists c3.
    assert_eq!(view("c3", 0), b"View_b");
    // View_c rule is Rule_2 that lists c3.
    assert_eq!(view("c3", 1), b"View_c");
    // View_d rule is Rule_3 that lists c3.
    assert_eq!(view("c3", 2), b"View_d");
    // View_e rule is Rule_4 that lists c3.
    assert_eq!(view("c3", 3), b"View_e");
    // View_h has no rule.
    assert_eq!(view("c3", 4), b"View_h");
    // SView_a has rule Rule_2 that lists c3.
    assert_eq!(view("c3", 5), b"SView_a");
    // SView_b has rule Rule_3 that lists c3.
    assert_eq!(view("c3", 6), b"SView_b");
    // SView_e has no rule.
    assert_eq!(view("c3", 7), b"SView_e");

    assert_eq!(config.num_views_for_color_space("sRGB", "c4").unwrap(), 6);
    // View_e rule is Rule_4 that lists c4.
    assert_eq!(view("c4", 0), b"View_e");
    // View_f rule is Rule_5 that lists encoding log, c4 has encoding log.
    assert_eq!(view("c4", 1), b"View_f");
    // View_g rule is Rule_6 that lists encoding log, c4 has encoding log.
    assert_eq!(view("c4", 2), b"View_g");
    // View_h has no rule.
    assert_eq!(view("c4", 3), b"View_h");
    // SView_d rule is Rule_5 that lists encoding log, c4 has encoding log.
    assert_eq!(view("c4", 4), b"SView_d");
    // SView_e has no rule.
    assert_eq!(view("c4", 5), b"SView_e");

    assert_eq!(config.serialize().unwrap(), SIMPLE_CONFIG.as_bytes());

    // Copy to set active views.
    let mut configav = (*config).clone();
    configav
        .set_active_views("SView_e, View_h, SView_d, View_d, SView_a, View_b")
        .unwrap();
    configav.validate().unwrap();

    // Viewing rule results are further filtered and re-ordered by the active views list.
    let view = |cs: &str, i: i32| {
        configav
            .view_for_color_space("sRGB", cs, i)
            .unwrap()
            .to_vec()
    };
    assert_eq!(configav.num_views_for_color_space("sRGB", "c3").unwrap(), 5);
    assert_eq!(view("c3", 0), b"SView_e");
    assert_eq!(view("c3", 1), b"View_h");
    assert_eq!(view("c3", 2), b"View_d");
    assert_eq!(view("c3", 3), b"SView_a");
    assert_eq!(view("c3", 4), b"View_b");

    // Test the default methods.

    assert_eq!(configav.default_display(), b"sRGB");
    assert_eq!(
        view("c3", 0),
        configav.default_view_for_color_space("sRGB", "c3").unwrap()
    );
}
