// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The two upstream virtual-display tests waiting for SystemMonitors (9.3a).

use super::*;
use crate::SystemMonitors;
use crate::test_env::EnvGuard;
#[cfg(target_os = "linux")]
use ocio_testkit::upstream::check_throw_what;

const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}
  - !<View> {name: sview2, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}
    - !<Views> [sview1]

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<View> {name: Film, view_transform: display_vt, display_colorspace: <USE_DISPLAY_NAME>}
  - !<Views> [sview2]

active_displays: []
active_views: []

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: ""
    equalitygroup: ""
    bitdepth: unknown
    isdata: true
    allocation: uniform
"#;

/// Port of `OCIO_ADD_TEST(Config, virtual_display)` @ v2.5.2.
#[test]
fn virtual_display() {
    let _env = EnvGuard::new();
    // Step1: validate a config containing a virtual display.
    let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();
    // Step2: correctly loaded and saved, with all the upstream basic checks.
    assert_eq!(config.serialize().unwrap(), CONFIG.as_bytes());
    assert_eq!(config.num_views("sRGB"), 3);
    assert_eq!(
        config.num_views_of_type(ViewType::DisplayDefined, "sRGB"),
        2
    );
    assert_eq!(config.num_views_of_type(ViewType::Shared, "sRGB"), 1);
    assert_eq!(config.view_of_type(ViewType::Shared, "sRGB", 0), b"sview1");
    assert!(config.has_view("sRGB", "sview1"));
    assert!(config.is_view_shared("sRGB", "sview1"));
    assert!(!config.is_view_shared("sRGB", ""));
    assert!(!config.is_view_shared("sRGB", b"")); // nullptr's Rust spelling (D4).
    assert_eq!(
        config.display_view_color_space_name("sRGB", "sview1"),
        b"raw"
    );
    assert_eq!(
        config.virtual_display_view_color_space_name("sview2"),
        b"raw"
    );
    // Step3: the virtual display information.
    {
        let mut cfg = (*config).clone();
        assert!(Config::are_views_equal(&config, &cfg, "sRGB", "sview1"));
        assert!(Config::are_views_equal(&config, &cfg, "sRGB", "Raw"));
        assert!(Config::are_views_equal(&config, &cfg, "sRGB", "view"));
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 2);
        let view_name = cfg.virtual_display_view(ViewType::DisplayDefined, 0);
        assert!(cfg.has_virtual_view(view_name));
        assert_eq!(view_name, b"Raw");
        assert_eq!(cfg.virtual_display_view_transform_name(view_name), b"");
        assert_eq!(cfg.virtual_display_view_color_space_name(view_name), b"raw");
        assert_eq!(cfg.virtual_display_view_looks(view_name), b"");
        assert_eq!(cfg.virtual_display_view_rule(view_name), b"");
        assert_eq!(cfg.virtual_display_view_description(view_name), b"");
        assert!(Config::are_virtual_views_equal(&config, &cfg, view_name));
        let view_name = cfg.virtual_display_view(ViewType::DisplayDefined, 1);
        assert_eq!(view_name, b"Film");
        assert_eq!(
            cfg.virtual_display_view_transform_name(view_name),
            b"display_vt"
        );
        assert_eq!(
            cfg.virtual_display_view_color_space_name(view_name),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(cfg.virtual_display_view_looks(view_name), b"");
        assert_eq!(cfg.virtual_display_view_rule(view_name), b"");
        assert_eq!(cfg.virtual_display_view_description(view_name), b"");
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
        assert_eq!(cfg.virtual_display_view(ViewType::Shared, 0), b"sview2");
        assert_eq!(cfg.virtual_display_view_color_space_name("sview2"), b"raw");
        assert!(cfg.has_virtual_view("sview2"));
        assert!(cfg.is_virtual_view_shared("sview2"));
        assert!(Config::are_virtual_views_equal(&config, &cfg, "sview2"));
        assert!(!cfg.is_virtual_view_shared(""));
        assert!(!cfg.is_virtual_view_shared(b"")); // nullptr.
        cfg.remove_virtual_display_view("Raw");
        assert!(!Config::are_virtual_views_equal(&config, &cfg, "Raw"));
        assert!(!cfg.has_virtual_view("Raw"));
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
        assert_eq!(
            cfg.virtual_display_view(ViewType::DisplayDefined, 0),
            b"Film"
        );
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
        assert_eq!(cfg.virtual_display_view(ViewType::Shared, 0), b"sview2");
        cfg.remove_virtual_display_view("sview2");
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 0);
        assert!(!cfg.has_virtual_view("sview2"));
        assert!(!cfg.is_virtual_view_shared("sview2"));
        assert!(!Config::are_virtual_views_equal(&config, &cfg, "sview2"));
        {
            let config2 = Config::create_from_stream(&cfg.serialize().unwrap()).unwrap();
            assert_eq!(
                config2.virtual_display_num_views(ViewType::DisplayDefined),
                1
            );
            assert_eq!(config2.virtual_display_num_views(ViewType::Shared), 0);
        }
        cfg.add_virtual_display_shared_view("sview2").unwrap();
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 1);
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 1);
        assert!(cfg.is_virtual_view_shared("sview2"));
        cfg.clear_virtual_display();
        assert_eq!(cfg.virtual_display_num_views(ViewType::DisplayDefined), 0);
        assert_eq!(cfg.virtual_display_num_views(ViewType::Shared), 0);
        assert!(!cfg.is_virtual_view_shared("sview2"));
        {
            let config2 = Config::create_from_stream(&cfg.serialize().unwrap()).unwrap();
            assert_eq!(
                config2.virtual_display_num_views(ViewType::DisplayDefined),
                0
            );
            assert_eq!(config2.virtual_display_num_views(ViewType::Shared), 0);
        }
    }
    // Step4: the upstream Windows/Linux blocks, with P9-4's required monitor assertion.
    let icc_profile = format!(
        "{}/icc-test-1.icc",
        crate::fileformats::test_utils::get_test_files_dir()
    );
    #[cfg(windows)]
    {
        assert!(SystemMonitors::get().is_supported());
        let monitor_name = SystemMonitors::get().monitor_name(0).unwrap();
        let mut cfg = (*config).clone();
        cfg.instantiate_display_from_monitor_name(monitor_name)
            .unwrap();
        assert_eq!(cfg.num_displays(), 1 + config.num_displays());
        let num_color_spaces = config.num_color_spaces_with(
            SearchReferenceSpaceType::Display,
            ColorSpaceVisibility::Active,
        );
        assert_eq!(
            cfg.num_color_spaces_with(
                SearchReferenceSpaceType::Display,
                ColorSpaceVisibility::Active
            ),
            1 + num_color_spaces
        );
        let display_name = cfg.display(config.num_displays());
        assert_eq!(cfg.num_views(display_name), 3);
        assert_eq!(
            cfg.num_views_of_type(ViewType::DisplayDefined, display_name),
            2
        );
        assert_eq!(cfg.num_views_of_type(ViewType::Shared, display_name), 1);
        assert!(!cfg.is_display_temporary(config.num_displays() - 1));
        assert!(cfg.is_display_temporary(config.num_displays()));
        let cs = cfg.color_space(display_name).unwrap();
        assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
        let tr = cs.transform(ColorSpaceDirection::FromReference);
        assert!(tr.is_some());
        assert!(matches!(tr, Some(Transform::File(_))));
        let display_pos = cfg
            .instantiate_display_from_monitor_name(monitor_name)
            .unwrap();
        assert_eq!(display_pos, config.num_displays());
        assert_eq!(cfg.num_displays(), 1 + config.num_displays());
        assert_eq!(
            cfg.num_color_spaces_with(
                SearchReferenceSpaceType::Display,
                ColorSpaceVisibility::Active
            ),
            1 + num_color_spaces
        );
        {
            let config2 = Config::create_from_stream(&cfg.serialize().unwrap()).unwrap();
            assert_eq!(config2.num_displays(), config.num_displays());
            assert_eq!(config2.num_displays(), cfg.num_displays() - 1);
            assert_eq!(config2.num_color_spaces(), config.num_color_spaces());
            assert_eq!(config2.num_color_spaces(), cfg.num_color_spaces() - 1);
        }
        {
            assert!(cfg.is_display_temporary(config.num_displays()));
            cfg.set_display_temporary(config.num_displays(), false);
            assert!(!cfg.is_display_temporary(config.num_displays()));
            let config2 = Config::create_from_stream(&cfg.serialize().unwrap()).unwrap();
            assert_eq!(config2.num_displays(), config.num_displays() + 1);
            assert_eq!(config2.num_displays(), cfg.num_displays());
            assert_eq!(config2.num_color_spaces(), config.num_color_spaces() + 1);
            assert_eq!(config2.num_color_spaces(), cfg.num_color_spaces());
        }
        let mut cfg = (*config).clone();
        let display_pos = cfg
            .instantiate_display_from_icc_profile(&icc_profile)
            .unwrap();
        assert_eq!(display_pos, config.num_displays());
        assert_eq!(cfg.num_displays(), 1 + config.num_displays());
        assert_eq!(
            cfg.num_color_spaces_with(
                SearchReferenceSpaceType::Display,
                ColorSpaceVisibility::Active
            ),
            1 + num_color_spaces
        );
        let display = cfg.display(config.num_displays());
        assert_eq!(cfg.num_views(display), 3);
        assert_eq!(cfg.num_views_of_type(ViewType::DisplayDefined, display), 2);
        assert_eq!(cfg.num_views_of_type(ViewType::Shared, display), 1);
    }
    #[cfg(target_os = "linux")]
    {
        assert!(!SystemMonitors::get().is_supported());
        check_throw_what(
            SystemMonitors::get().monitor_name(0),
            "Invalid index for the monitor name 0 where the number of monitors is 0.",
        );
        let mut cfg = (*config).clone();
        cfg.instantiate_display_from_icc_profile(&icc_profile)
            .unwrap();
        assert_eq!(cfg.num_displays(), 1 + config.num_displays());
        let num_color_spaces = config.num_color_spaces_with(
            SearchReferenceSpaceType::Display,
            ColorSpaceVisibility::Active,
        );
        assert_eq!(
            cfg.num_color_spaces_with(
                SearchReferenceSpaceType::Display,
                ColorSpaceVisibility::Active
            ),
            1 + num_color_spaces
        );
        let display = cfg.display(config.num_displays());
        assert_eq!(cfg.num_views(display), 3);
        assert_eq!(cfg.num_views_of_type(ViewType::DisplayDefined, display), 2);
        assert_eq!(cfg.num_views_of_type(ViewType::Shared, display), 1);
    }
}

const ACTIVE_CONFIG: &str = r#"ocio_profile_version: 2

roles:
  default: raw

file_rules:
  - !<Rule> {name: Default, colorspace: default}

shared_views:
  - !<View> {name: sview1, colorspace: raw}

displays:
  Raw:
    - !<View> {name: Raw, colorspace: raw}
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: view, view_transform: display_vt, display_colorspace: display_cs}

virtual_display:
  - !<View> {name: Raw, colorspace: raw}
  - !<Views> [sview1]

active_displays: [sRGB]
active_views: [view]

view_transforms:
  - !<ViewTransform>
    name: default_vt
    to_scene_reference: !<CDLTransform> {sat: 1.5}

  - !<ViewTransform>
    name: display_vt
    to_display_reference: !<CDLTransform> {sat: 1.5}

display_colorspaces:
  - !<ColorSpace>
    name: display_cs
    to_display_reference: !<CDLTransform> {sat: 1.5}

colorspaces:
  - !<ColorSpace>
    name: raw
"#;

/// Port of `OCIO_ADD_TEST(Config, virtual_display_with_active_displays)` @ v2.5.2.
#[test]
fn virtual_display_with_active_displays() {
    let _env = EnvGuard::new();
    let config = Config::create_from_stream(ACTIVE_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();
    assert_eq!(config.num_displays(), 1);
    assert_eq!(config.num_views("sRGB"), 1);
    #[cfg(windows)]
    {
        assert!(SystemMonitors::get().is_supported());
        let monitor_name = SystemMonitors::get().monitor_name(0).unwrap();
        let mut cfg = (*config).clone();
        let display_index = cfg
            .instantiate_display_from_monitor_name(monitor_name)
            .unwrap();
        assert_eq!(cfg.num_displays(), 2);
        assert_eq!(cfg.num_views("sRGB"), 2);
        assert_eq!(cfg.num_views(cfg.display(display_index)), 2);
    }
}

/// Exact same-machine wheel list, index errors, and all monitor/empty/missing config calls.
/// Windows must have at least one monitor (P9-4); Linux must exercise its empty list.
#[test]
fn monitors_and_instantiated_displays_match_the_local_wheel() {
    use ocio_testkit::Oracle;
    use serde_json::{Value, json};
    let _env = EnvGuard::new();
    let script = r#"
import json,sys
import PyOpenColorIO as OCIO
monitors=list(OCIO.SystemMonitors().getMonitors())
names=[pair[0] for pair in monitors]+["OCIO missing monitor",""]
names += [pair[0].swapcase() for pair in monitors]
names += [pair[0]+"\0ignored suffix" for pair in monitors]
results=[]
for text in json.loads(sys.argv[1]):
    for name in names:
        cfg=OCIO.Config.CreateFromStream(text)
        try:
            index=cfg.instantiateDisplayFromMonitorName(name)
            display=cfg.getDisplays()[index]
            result={"index":index,"display":display,"serialize":cfg.serialize(),
                    "num_displays":len(cfg.getDisplays()),"num_views":len(cfg.getViews(display)),
                    "temporary":cfg.isDisplayTemporary(display),
                    "profile_src":cfg.getColorSpace(display).getTransform(OCIO.COLORSPACE_DIR_FROM_REFERENCE).getSrc()}
            cfg.setDisplayTemporary(display,False)
            result["persisted_serialize"]=cfg.serialize()
        except OCIO.Exception as error:
            result={"exception":{"type":type(error).__name__,"message":str(error)}}
        results.append({"name":name,"result":result})
indices=[len(monitors),len(monitors)+1,2147483647,-1]
invalid=[]
for index in indices:
    try:
        OCIO.SystemMonitors().getMonitors()[index]
    except OCIO.Exception as error:
        invalid.append({"index":index,"message":str(error)})
    else:
        raise AssertionError(index)
print(json.dumps({"monitors":monitors,"results":results,"invalid":invalid}))
"#;
    let lines = Oracle::get().run_script(script, &[json!([CONFIG, ACTIVE_CONFIG]).to_string()]);
    assert_eq!(lines.len(), 1);
    let wheel: Value = serde_json::from_str(&lines[0]).unwrap();
    let monitors = SystemMonitors::get();
    assert!(std::ptr::eq(monitors, SystemMonitors::get()));
    let expected = wheel["monitors"].as_array().unwrap();
    #[cfg(windows)]
    assert!(
        !expected.is_empty(),
        "P9-4: the upstream Windows monitor tests require a real monitor"
    );
    #[cfg(target_os = "linux")]
    assert!(expected.is_empty());
    assert_eq!(monitors.num_monitors(), expected.len());
    assert_eq!(monitors.is_supported(), !expected.is_empty());
    for (index, pair) in expected.iter().enumerate() {
        assert_eq!(
            monitors.monitor_name(index).unwrap(),
            pair[0].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            monitors.profile_filepath(index).unwrap(),
            pair[1].as_str().unwrap().as_bytes()
        );
    }
    let invalid = wheel["invalid"].as_array().unwrap();
    assert_eq!(invalid.len(), 4);
    for entry in invalid {
        let index = entry["index"].as_i64().unwrap() as usize;
        assert_eq!(
            monitors.monitor_name(index).unwrap_err().what(),
            entry["message"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            monitors.profile_filepath(index).unwrap_err().what(),
            entry["message"].as_str().unwrap().as_bytes()
        );
    }
    let results = wheel["results"].as_array().unwrap();
    let cases_per_config = 3 * expected.len() + 2;
    assert_eq!(results.len(), 2 * cases_per_config);
    for (text, rows) in [CONFIG, ACTIVE_CONFIG]
        .into_iter()
        .zip(results.chunks_exact(cases_per_config))
    {
        for (position, row) in rows.iter().enumerate() {
            let mut cfg = (*Config::create_from_stream(text.as_bytes()).unwrap()).clone();
            let name = row["name"].as_str().unwrap();
            assert!(row["result"].is_object(), "{row}");
            let wheel_succeeded = row["result"].get("exception").is_none();
            if position < expected.len() {
                assert!(wheel_succeeded, "the real monitor must instantiate: {row}");
            }
            match cfg.instantiate_display_from_monitor_name(name) {
                Ok(index) => {
                    assert!(
                        wheel_succeeded,
                        "monitor call succeeded where the wheel refused: {row}"
                    );
                    let display = cfg.display(index);
                    // PyConfig.cpp:554-577 @ v2.5.2 resolves the name among ALL displays
                    // before these APIs; instantiateDisplay returns an active-list index.
                    let all_display_index = cfg.display_all_by_name(display);
                    assert!(all_display_index >= 0);
                    let Some(Transform::File(file)) = cfg
                        .color_space(display)
                        .unwrap()
                        .transform(ColorSpaceDirection::FromReference)
                    else {
                        panic!("monitor display has no ICC FileTransform");
                    };
                    let mut actual = json!({"index":index,"display":std::str::from_utf8(display).unwrap(),
                        "serialize":std::str::from_utf8(&cfg.serialize().unwrap()).unwrap(),
                        "num_displays":cfg.num_displays(),"num_views":cfg.num_views(display),
                        "temporary":cfg.is_display_temporary(all_display_index),
                        "profile_src":std::str::from_utf8(file.src()).unwrap()});
                    cfg.set_display_temporary(all_display_index, false);
                    actual["persisted_serialize"] =
                        json!(std::str::from_utf8(&cfg.serialize().unwrap()).unwrap());
                    assert_eq!(actual, row["result"], "{name}");
                }
                Err(error) => {
                    assert!(
                        !wheel_succeeded,
                        "monitor call failed where the wheel succeeded: {name}: {error}"
                    );
                    assert_eq!(
                        json!({"exception":{"type":"Exception","message":error.message()}}),
                        row["result"]
                    );
                }
            }
        }
    }
}
