// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/ViewingRules_tests.cpp` @ v2.5.2 that read no YAML. The
//! others read configs and come with the YAML reader (3.3k); `crates/ocio/tests/
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
