// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_calls` command (`oracle/ocio_oracle/config_api.py`), against the wheel
//! itself: the environment of a request is exactly the one it gives and the process's own comes
//! back after it; files land in the request's directory, which is the working directory;
//! strings come back as their bytes, those the binding can't decode included, element by
//! element; the log is OCIO's own, as bytes, per call; doubles and arrays come back exact;
//! exceptions are reported per call; the dump reaches every getter; and the requests it can't
//! run exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, bytes_arg, exception, hex, log, result, result_bytes};
use serde_json::{Value, json};

/// A config of one color space, with no `environment:` section (so its context loads the whole
/// environment) and the extra lines given.
fn yaml(extra: &str) -> String {
    format!(
        "ocio_profile_version: 2\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
         name: raw\n{extra}"
    )
}

fn yaml_bytes(extra: &[u8]) -> Vec<u8> {
    [yaml("").as_bytes(), extra].concat()
}

fn call(args: Value) -> Value {
    Oracle::get().call("config_calls", args, &[]).result
}

/// The calls of a response whose config was made.
fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["config"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// The bytes of each `[name, value]` pair of a list of pairs.
fn pairs(list: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect()
}

/// The context of a config that loads the whole environment holds exactly the request's
/// variables: none of the oracle's own reach it.
#[test]
fn the_environment_is_exactly_the_requests() {
    let response = call(json!({
        "config": {"yaml": yaml("")},
        "env": {"OCIO_RS_PROBE_B": "2", "OCIO_RS_PROBE_A": "one"},
        "calls": [
            {"call": "getCurrentContext", "as": "context"},
            {"call": "getStringVars", "on": "context"},
            {"call": "getEnvironmentMode"},
            {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["PATH"]},
            {"call": "GetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_PROBE_A"]},
        ],
    }));
    let c = calls(&response);
    let mut vars = pairs(result(&c[1]));
    vars.sort();
    assert_eq!(
        vars,
        [
            (b"OCIO_RS_PROBE_A".to_vec(), b"one".to_vec()),
            (b"OCIO_RS_PROBE_B".to_vec(), b"2".to_vec())
        ]
    );
    assert_eq!(*result(&c[2]), json!({"enum": "ENV_ENVIRONMENT_LOAD_ALL"}));
    assert_eq!(*result(&c[3]), json!(false));
    assert_eq!(result_bytes(&c[4]), b"one");
}

/// In one oracle process, a request's variables are gone in the next request, and the
/// process's own are back once a request ends (the command checks that through OCIO and fails
/// otherwise): what a later command of a batch sees doesn't depend on the earlier ones.
#[test]
fn the_process_environment_comes_back() {
    let with = json!({"config": "raw", "env": {"OCIO_RS_PROBE": "x"}, "calls": []});
    let probe = json!({"config": "raw", "calls": [
        {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["OCIO_RS_PROBE"]},
    ]});
    let calls_ = [with.clone(), probe, with];
    let batch: Vec<BatchCall<'_>> = calls_
        .iter()
        .map(|a| BatchCall {
            cmd: "config_calls",
            args: a.clone(),
            blobs: vec![],
        })
        .collect();
    let out: Vec<_> = Oracle::get()
        .batch(&batch, false)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")))
        .collect();
    assert_eq!(*result(&calls(&out[1].result)[0]), json!(false));
}

/// Files land in a new directory, the request's working directory: a relative path finds
/// them, and the config's working directory is the file's directory under it.
#[test]
fn files_land_in_the_working_directory() {
    let response = call(json!({
        "config": {"file": "sub/config.ocio"},
        "files": {"sub/config.ocio": yaml("")},
        "calls": [{"call": "getWorkingDir"}],
    }));
    let dir = response["dir"].as_str().unwrap();
    let working = String::from_utf8(result_bytes(&calls(&response)[0])).unwrap();
    assert!(working.starts_with(dir), "{working} is not under {dir}");
    assert!(working.ends_with("sub"), "{working}");
}

/// Bytes go to the library as they are; a string the binding can't decode comes back as its
/// bytes, from a whole call (`undecodable`) and from each element of a list on its own.
#[test]
fn undecodable_strings_come_back_as_bytes() {
    let config = yaml_bytes(b"  - !<ColorSpace>\n    name: a\xffb\n    aliases: [x\xfey, ok]\n");
    let response = call(json!({
        "config": {"yaml": bytes_arg(&config)},
        "calls": [
            {"call": "getColorSpace", "args": ["raw"], "as": "cs"},
            {"call": "setFamily", "on": "cs", "args": [{"bytes": "61ff62"}]},
            {"call": "getFamily", "on": "cs"},
            {"call": "getColorSpaceNames", "args": [{"enum": "SEARCH_REFERENCE_SPACE_ALL"},
                                                     {"enum": "COLORSPACE_ALL"}]},
            {"call": "getColorSpace", "args": [{"bytes": "61ff62"}], "as": "odd"},
            {"call": "getAliases", "on": "odd"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(c[2]["undecodable"], "61ff62", "{response}");
    assert_eq!(
        *result(&c[3]),
        json!([{"bytes": hex(b"raw")}, {"undecodable": "61ff62"}])
    );
    assert!(
        c[4]["result"]["repr"]["undecodable"]
            .as_str()
            .unwrap()
            .contains("61ff62"),
        "{response}"
    );
    assert_eq!(
        *result(&c[5]),
        json!([{"undecodable": "78fe79"}, {"bytes": hex(b"ok")}])
    );
}

/// The log is OCIO's own messages as bytes, those that aren't UTF-8 included: a warning while
/// loading goes to config_log, one in a call to that call's log only.
#[test]
fn the_log_is_bytes_per_call() {
    let config = yaml_bytes(b"    b\xffd: 1\n");
    let response = call(json!({
        "config": {"yaml": bytes_arg(&config)},
        "calls": [
            {"call": "getName"},
            {"call": "CreateFromStream", "on": "Config", "args": [bytes_arg(&config)]},
            {"call": "getName"},
        ],
    }));
    let config_log = log(&response["config_log"]);
    assert_eq!(config_log.len(), 1, "{response}");
    assert!(
        config_log[0].starts_with(b"[OpenColorIO Warning]: "),
        "{response}"
    );
    assert!(
        config_log[0].windows(3).any(|w| w == b"b\xffd"),
        "{response}"
    );
    assert!(config_log[0].ends_with(b"\n"));
    let c = calls(&response);
    assert_eq!(log(&c[0]["log"]), Vec::<Vec<u8>>::new());
    assert_eq!(log(&c[1]["log"]), config_log);
    assert_eq!(log(&c[2]["log"]), Vec::<Vec<u8>>::new());
}

/// Doubles come back with all their bits (0.1 isn't a float), and arrays whole, as bytes.
#[test]
fn doubles_and_arrays_are_exact() {
    let tenth = 0.1f64.to_bits();
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "MatrixTransform", "kwargs": {"offset": [{"f64": tenth}, 0, 0, 0]}, "as": "m"},
            {"call": "getOffset", "on": "m"},
            {"new": "Lut1DTransform", "args": [4096, false], "as": "lut"},
            {"call": "getData", "on": "lut"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result(&c[1])[0], json!({"f64": tenth}));
    assert_ne!(f64::from(0.1f32).to_bits(), tenth);
    let array = &result(&c[3])["array"];
    assert_eq!(array["dtype"], "float32");
    assert_eq!(array["shape"], json!([4096 * 3]));
    assert_eq!(array["bytes"].as_str().unwrap().len(), 4096 * 3 * 4 * 2);
}

/// What the library raises is reported per call with its log, and nothing is stored for it,
/// a transform's constructor included; a source that raises reports the exception and runs no
/// call.
#[test]
fn exceptions_are_reported_per_call() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"call": "getProcessor", "args": ["nope", "raw"], "as": "p"},
            {"call": "getMajorVersion"},
            {"call": "getProcessor", "args": [
                {"transform": {"class": "LogTransform", "args": {"base": 1.0}}}]},
        ],
    }));
    let c = calls(&response);
    let (kind, message) = exception(&c[0]);
    assert_eq!(kind, "Exception");
    assert!(!message.is_empty());
    assert!(c[0]["log"].is_array());
    assert!(c[1]["result"].is_number());
    assert_eq!(exception(&c[2]).0, "Exception", "{response}");
    let failed = call(json!({"config": {"yaml": "not: a config"}, "calls": [{"dump": "config"}]}));
    assert_eq!(
        failed["config"]["exception"]["type"], "Exception",
        "{failed}"
    );
    assert_eq!(failed["calls"], json!([]));
}

/// The dump of a built-in config reaches every getter but the two that take what no config
/// holds (a file path, a processor's arguments), and lists them; its lookups find each color
/// space by name, alias and role. That includes the six display and view getters OCIO 2.6.0
/// added (display and view aliases, PR #2341, and `getDisplayDescription`).
#[test]
fn the_dump_reaches_every_getter() {
    let response = call(json!({
        "config": {"builtin": "studio-config-v4.0.0_aces-v2.0_ocio-v2.5"},
        "calls": [{"dump": "config"}],
    }));
    let dump = result(&calls(&response)[0]);
    assert_eq!(
        dump["not_dumped"],
        json!(["getColorSpaceFromFilepath", "getProcessor"])
    );
    for key in [
        "getters",
        "color_space_names",
        "color_spaces",
        "categories",
        "roles",
        "lookups",
        "environment",
        "displays",
        "shared_views",
        "virtual_display",
        "looks",
        "view_transforms",
        "default_scene_to_display",
        "named_transform_names",
        "named_transforms",
        "current_context",
        "file_rules",
        "viewing_rules",
        "serialize",
        "validate",
    ] {
        assert!(dump.get(key).is_some(), "the dump has no {key}");
    }
    let space = &dump["color_spaces"][0]["dump"];
    assert_eq!(space["class"], "ColorSpace");
    assert_eq!(space["uncalled"], json!([]));
    assert!(bytes(&dump["serialize"]).starts_with(b"ocio_profile_version"));
    // Each name, alias and role in the lookups, each found by getColorSpace.
    let lookups = dump["lookups"].as_array().unwrap();
    let found: Vec<(Vec<u8>, Vec<u8>)> = lookups
        .iter()
        .filter(|l| !l["getColorSpace"].is_null())
        .map(|l| (bytes(&l["name"]), bytes(&l["getColorSpace"])))
        .collect();
    assert!(found.contains(&(b"ACEScg".to_vec(), b"ACEScg".to_vec())));
    assert!(found.contains(&(b"lin_ap1".to_vec(), b"ACEScg".to_vec())));
    assert!(found.contains(&(b"scene_linear".to_vec(), b"ACEScg".to_vec())));
}

/// The 2.6 display and view getters of the dump: a view's aliases as the config gives them,
/// the view's name and each alias looked up as it is and upper-cased, then a name that is no
/// alias, the names a display and a view resolve to, a display's description, and the lookups of
/// a display color space's alias.
#[test]
fn the_dump_reaches_the_aliases() {
    let config = b"ocio_profile_version: 2.6\nroles: {default: raw}\nuse_display_aliases: true\n\
                   colorspaces:\n  - !<ColorSpace> {name: raw}\ndisplay_colorspaces:\n  - \
                   !<ColorSpace> {name: d, aliases: [d_alias], description: the display}\n\
                   displays:\n  d:\n    - !<View> {name: v, colorspace: d, aliases: [a1, a2]}\n";
    let response =
        call(json!({"config": {"yaml": bytes_arg(config)}, "calls": [{"dump": "config"}]}));
    let dump = result(&calls(&response)[0]);
    assert_eq!(
        dump["not_dumped"],
        json!(["getColorSpaceFromFilepath", "getProcessor"])
    );
    let display = &dump["displays"][0];
    assert_eq!(bytes(&display["display"]), b"d", "{display}");
    assert_eq!(
        bytes(&display["getCanonicalDisplayName"]),
        b"d",
        "{display}"
    );
    assert_eq!(
        bytes(&display["getDisplayDescription"]),
        b"the display",
        "{display}"
    );
    let view = &display["views"][0];
    assert_eq!(bytes(&view["view"]), b"v", "{view}");
    let aliases: Vec<Vec<u8>> = view["getDisplayViewAliases"]
        .as_array()
        .unwrap()
        .iter()
        .map(bytes)
        .collect();
    assert_eq!(aliases, [b"a1".to_vec(), b"a2".to_vec()], "{view}");
    let probes: Vec<(Vec<u8>, Value)> = view["hasDisplayViewAlias"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), p[1].clone()))
        .collect();
    assert_eq!(
        probes,
        [
            (b"v".to_vec(), json!(false)),
            (b"V".to_vec(), json!(false)),
            (b"a1".to_vec(), json!(true)),
            (b"A1".to_vec(), json!(true)),
            (b"a2".to_vec(), json!(true)),
            (b"A2".to_vec(), json!(true)),
            (b"not an alias".to_vec(), json!(false))
        ],
        "{view}"
    );
    assert_eq!(bytes(&view["getCanonicalViewName"]), b"v", "{view}");
    assert_eq!(
        bytes(&view["getResolvedDisplayViewColorSpaceName"]),
        b"d",
        "{view}"
    );
    // The display color space's alias resolves to the display (use_display_aliases).
    let lookup = dump["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| bytes(&l["name"]) == b"d_alias")
        .unwrap();
    assert_eq!(bytes(&lookup["getCanonicalDisplayName"]), b"d", "{lookup}");
    assert_eq!(
        bytes(&lookup["getDisplayDescription"]),
        b"the display",
        "{lookup}"
    );
}

/// The view probes of the dump on a config with more kinds of view: views whose color space is
/// an alias or a role (`getResolvedDisplayViewColorSpaceName` gives the canonical name), a shared
/// view with a view transform and `<USE_DISPLAY_NAME>`, a role naming a display color space, an
/// inactive display and view, and a shared view no display uses. Each probe is the view's name
/// or an alias, as it is and with its ASCII letters upper-cased (a non-ASCII letter stays as
/// it is, as OCIO compares only ASCII letters ignoring case), each once, then a name that is no
/// alias; `by_probe` resolves each by display (`getCanonicalViewName`, Config.cpp:4382-4419
/// @ v2.6.0; `getResolvedDisplayViewColorSpaceName`, Config.cpp:4304-4339 @ v2.6.0).
#[test]
fn the_dump_probes_each_kind_of_view() {
    // `@e@` stands for a non-ASCII letter, U+00E9.
    let config = r#"ocio_profile_version: 2.6
roles:
  default: raw
  display_role: d
use_display_aliases: true
active_displays: [d]
active_views: [v, by_alias, by_role, sv]
shared_views:
  - !<View> {name: sv, view_transform: vt, display_colorspace: <USE_DISPLAY_NAME>, aliases: [sv_alias]}
  - !<View> {name: unused, colorspace: raw}
displays:
  d:
    - !<View> {name: v, colorspace: d, aliases: [a1, "@e@a", zed]}
    - !<View> {name: by_alias, colorspace: d_alias}
    - !<View> {name: by_role, colorspace: display_role}
    - !<Views> [sv]
  e:
    - !<View> {name: w, colorspace: e, aliases: [w_alias, "42", not an alias]}
view_transforms:
  - !<ViewTransform> {name: vt, from_scene_reference: !<MatrixTransform> {}}
colorspaces:
  - !<ColorSpace> {name: raw, isdata: true}
display_colorspaces:
  - !<ColorSpace> {name: d, aliases: [d_alias]}
  - !<ColorSpace> {name: e}
"#
    .replace("@e@", "\u{e9}");
    let response = call(
        json!({"config": {"yaml": bytes_arg(config.as_bytes())}, "calls": [{"dump": "config"}]}),
    );
    let dump = result(&calls(&response)[0]);
    // validate() refuses a role as a view's color space (Config.cpp's display checks); the
    // getters still answer.
    assert!(
        dump["validate"]["exception"].is_object(),
        "{}",
        dump["validate"]
    );
    let view_of = |list: &Value, name: &[u8]| -> Value {
        list.as_array()
            .unwrap()
            .iter()
            .find(|v| bytes(&v["view"]) == name)
            .unwrap_or_else(|| panic!("no view {:?}: {list}", String::from_utf8_lossy(name)))
            .clone()
    };
    let display_of = |name: &[u8]| -> Value {
        dump["displays"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| bytes(&d["display"]) == name)
            .unwrap_or_else(|| panic!("no display {:?}", String::from_utf8_lossy(name)))
            .clone()
    };
    // [probe, hasDisplayViewAlias] and [probe, getCanonicalViewName, resolved color space].
    let has = |view: &Value| -> Vec<(Vec<u8>, Value)> {
        view["hasDisplayViewAlias"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (bytes(&p[0]), p[1].clone()))
            .collect()
    };
    let by_probe = |view: &Value| -> Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        view["by_probe"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (bytes(&p[0]), bytes(&p[1]), bytes(&p[2])))
            .collect()
    };
    let t = |s: &[u8]| s.to_vec();
    let (yes, no) = (json!(true), json!(false));

    let d = display_of(b"d");
    let v = view_of(&d["views"], b"v");
    assert_eq!(
        has(&v),
        [
            (t(b"v"), no.clone()),
            (t(b"V"), no.clone()),
            (t(b"a1"), yes.clone()),
            (t(b"A1"), yes.clone()),
            (t(b"\xc3\xa9a"), yes.clone()),
            (t(b"\xc3\xa9A"), yes.clone()),
            (t(b"zed"), yes.clone()),
            (t(b"ZED"), yes.clone()),
            (t(b"not an alias"), no.clone()),
        ],
        "{v}"
    );
    // A view's own name resolves to itself as the caller spells it; an alias to the view's name.
    assert_eq!(
        by_probe(&v),
        [
            (t(b"v"), t(b"v"), t(b"d")),
            (t(b"V"), t(b"V"), t(b"d")),
            (t(b"a1"), t(b"v"), t(b"d")),
            (t(b"A1"), t(b"v"), t(b"d")),
            (t(b"\xc3\xa9a"), t(b"v"), t(b"d")),
            (t(b"\xc3\xa9A"), t(b"v"), t(b"d")),
            (t(b"zed"), t(b"v"), t(b"d")),
            (t(b"ZED"), t(b"v"), t(b"d")),
            (t(b"not an alias"), t(b""), t(b"")),
        ],
        "{v}"
    );
    // The display's probes: its name upper-cased, its color space's alias as it is and
    // upper-cased (use_display_aliases), the role naming that color space as it is and
    // upper-cased, which getCanonicalDisplayName refuses ignoring case, and a name of no
    // display (Config.cpp:4225-4300 @ v2.6.0).
    let display_probes: Vec<(Vec<u8>, Vec<u8>)> = d["display_probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect();
    assert_eq!(
        display_probes,
        [
            (t(b"D"), t(b"d")),
            (t(b"d_alias"), t(b"d")),
            (t(b"D_ALIAS"), t(b"d")),
            (t(b"display_role"), t(b"")),
            (t(b"DISPLAY_ROLE"), t(b"")),
            (t(b"not a display"), t(b"")),
        ],
        "{d}"
    );
    // Each view by each of those display names: the names the display resolves to find the
    // view by its name and aliases; the role and the unknown name find nothing.
    // [probe, getCanonicalViewName, resolved color space] per probe.
    type Probes = Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>;
    let by_display = |view: &Value| -> Vec<(Vec<u8>, Probes)> {
        view["by_display"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let probes = d[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (bytes(&p[0]), bytes(&p[1]), bytes(&p[2])))
                    .collect();
                (bytes(&d[0]), probes)
            })
            .collect()
    };
    let v_by_display = by_display(&v);
    let names: Vec<&[u8]> = v_by_display.iter().map(|(d, _)| d.as_slice()).collect();
    assert_eq!(
        names,
        [
            &b"D"[..],
            b"d_alias",
            b"D_ALIAS",
            b"display_role",
            b"DISPLAY_ROLE",
            b"not a display"
        ],
        "{v}"
    );
    for (display, probes) in &v_by_display {
        let found = !matches!(
            display.as_slice(),
            b"display_role" | b"DISPLAY_ROLE" | b"not a display"
        );
        assert_eq!(probes.len(), by_probe(&v).len(), "{v}");
        for ((probe, canonical, resolved), (own, own_canonical, own_resolved)) in
            probes.iter().zip(by_probe(&v))
        {
            assert_eq!(probe, &own, "{v}");
            if found {
                assert_eq!(
                    (canonical, resolved),
                    (&own_canonical, &own_resolved),
                    "{v}"
                );
            } else {
                assert_eq!((canonical, resolved), (&t(b""), &t(b"")), "{v}");
            }
        }
    }
    assert_eq!(by_display(&v)[2].1[2], (t(b"a1"), t(b"v"), t(b"d")), "{v}");
    // The color space of a view, by an alias or a role, resolves to its canonical name.
    for (name, cs) in [
        (&b"by_alias"[..], &b"d_alias"[..]),
        (b"by_role", b"display_role"),
    ] {
        let view = view_of(&d["views"], name);
        assert_eq!(bytes(&view["getDisplayViewColorSpaceName"]), cs, "{view}");
        assert_eq!(
            bytes(&view["getResolvedDisplayViewColorSpaceName"]),
            b"d",
            "{view}"
        );
        assert_eq!(by_probe(&view).last().unwrap().0, b"not an alias", "{view}");
    }
    // The shared view: its display color space is the display's, found by its alias too.
    let sv = view_of(&d["views"], b"sv");
    assert_eq!(sv["isViewShared"], yes, "{sv}");
    assert_eq!(
        by_probe(&sv),
        [
            (t(b"sv"), t(b"sv"), t(b"d")),
            (t(b"SV"), t(b"SV"), t(b"d")),
            (t(b"sv_alias"), t(b"sv"), t(b"d")),
            (t(b"SV_ALIAS"), t(b"sv"), t(b"d")),
            (t(b"not an alias"), t(b""), t(b"")),
        ],
        "{sv}"
    );
    // The inactive display and its view; an alias without letters is probed once, and one
    // spelled as the name that ends the probes ends them.
    let e = display_of(b"e");
    let w = view_of(&e["views"], b"w");
    assert_eq!(
        has(&w),
        [
            (t(b"w"), no.clone()),
            (t(b"W"), no.clone()),
            (t(b"w_alias"), yes.clone()),
            (t(b"W_ALIAS"), yes.clone()),
            (t(b"42"), yes.clone()),
            (t(b"not an alias"), yes.clone()),
            (t(b"NOT AN ALIAS"), yes.clone()),
        ],
        "{w}"
    );
    assert_eq!(by_probe(&w)[2], (t(b"w_alias"), t(b"w"), t(b"e")), "{w}");
    let e_probes: Vec<(Vec<u8>, Vec<u8>)> = e["display_probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect();
    assert_eq!(
        e_probes,
        [(t(b"E"), t(b"e")), (t(b"not a display"), t(b""))],
        "{e}"
    );
    assert_eq!(
        by_display(&w)[0].1[2],
        (t(b"w_alias"), t(b"w"), t(b"e")),
        "{w}"
    );
    // The shared view by the display color space's alias, upper-cased.
    let sv_on_d = view_of(&d["views"], b"sv");
    assert_eq!(
        by_display(&sv_on_d)[2],
        (
            t(b"D_ALIAS"),
            vec![
                (t(b"sv"), t(b"sv"), t(b"d")),
                (t(b"SV"), t(b"SV"), t(b"d")),
                (t(b"sv_alias"), t(b"sv"), t(b"d")),
                (t(b"SV_ALIAS"), t(b"sv"), t(b"d")),
                (t(b"not an alias"), t(b""), t(b"")),
            ]
        ),
        "{sv_on_d}"
    );
    // The shared views as the config lists them (display ""): the alias lookup finds the view,
    // while the names, which need a display, resolve to nothing.
    let shared = &dump["shared_views"];
    let sv = view_of(shared, b"sv");
    assert_eq!(has(&sv)[2], (t(b"sv_alias"), yes.clone()), "{sv}");
    assert_eq!(by_probe(&sv)[2], (t(b"sv_alias"), t(b""), t(b"")), "{sv}");
    assert!(sv.get("by_display").is_none(), "{sv}");
    let unused = view_of(shared, b"unused");
    assert_eq!(
        has(&unused),
        [
            (t(b"unused"), no.clone()),
            (t(b"UNUSED"), no.clone()),
            (t(b"not an alias"), no)
        ],
        "{unused}"
    );
    // The role naming a display color space is looked up as one.
    let lookup = dump["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| bytes(&l["name"]) == b"display_role")
        .unwrap();
    assert_eq!(bytes(&lookup["getColorSpace"]), b"d", "{lookup}");
}

/// `[probe, answer]` pairs of a dump list, as bytes.
fn probe_pairs(list: &Value) -> Vec<(Vec<u8>, Value)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), p[1].clone()))
        .collect()
}

/// The display probes of a display named by an alias of its color space (upstream's
/// aliased_display_name case, Config.cpp:4278-4296 @ v2.6.0): the color space's own name and
/// its other alias resolve to the display; a role naming the color space by an alias is probed
/// as it is, upper-cased and lower-cased, and refused ignoring case (`hasRole`); an all
/// upper-case name is probed lower-cased, for displays and for views.
#[test]
fn the_dump_probes_each_name_of_a_display() {
    let config = r#"ocio_profile_version: 2.6
roles:
  default: raw
  alias_role: dcs_alias
use_display_aliases: true
displays:
  SRGB_DISP:
    - !<View> {name: v, colorspace: other_dcs, aliases: [a1]}
  ACES:
    - !<View> {name: RAW, colorspace: raw, aliases: [RAWZ, MiX]}
colorspaces:
  - !<ColorSpace> {name: raw, isdata: true}
display_colorspaces:
  - !<ColorSpace> {name: other_dcs, aliases: [SRGB_DISP, dcs_alias]}
"#;
    let response = call(
        json!({"config": {"yaml": bytes_arg(config.as_bytes())}, "calls": [{"dump": "config"}]}),
    );
    let dump = result(&calls(&response)[0]);
    let displays = dump["displays"].as_array().unwrap();
    assert_eq!(displays.len(), 2, "{}", dump["displays"]);
    let s = |text: &str| json!({"bytes": hex(text.as_bytes())});
    let srgb = displays
        .iter()
        .find(|d| bytes(&d["display"]) == b"SRGB_DISP")
        .unwrap();
    assert_eq!(
        probe_pairs(&srgb["display_probes"]),
        [
            (b"srgb_disp".to_vec(), s("SRGB_DISP")),
            (b"other_dcs".to_vec(), s("SRGB_DISP")),
            (b"OTHER_DCS".to_vec(), s("SRGB_DISP")),
            (b"dcs_alias".to_vec(), s("SRGB_DISP")),
            (b"DCS_ALIAS".to_vec(), s("SRGB_DISP")),
            (b"alias_role".to_vec(), s("")),
            (b"ALIAS_ROLE".to_vec(), s("")),
            (b"not a display".to_vec(), s("")),
        ],
        "{srgb}"
    );
    // The view by the color space's own name, upper-cased.
    let v = &srgb["views"][0];
    let by_other = v["by_display"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| bytes(&d[0]) == b"OTHER_DCS")
        .unwrap();
    assert_eq!(bytes(&by_other[1][2][1]), b"v", "{v}");
    assert_eq!(bytes(&by_other[1][2][2]), b"other_dcs", "{v}");
    let aces = displays
        .iter()
        .find(|d| bytes(&d["display"]) == b"ACES")
        .unwrap();
    assert_eq!(
        probe_pairs(&aces["display_probes"]),
        [
            (b"aces".to_vec(), s("ACES")),
            (b"not a display".to_vec(), s("")),
        ],
        "{aces}"
    );
    let raw = &aces["views"][0];
    assert_eq!(
        probe_pairs(&raw["hasDisplayViewAlias"]),
        [
            (b"RAW".to_vec(), json!(false)),
            (b"raw".to_vec(), json!(false)),
            (b"RAWZ".to_vec(), json!(true)),
            (b"rawz".to_vec(), json!(true)),
            (b"MiX".to_vec(), json!(true)),
            (b"MIX".to_vec(), json!(true)),
            (b"mix".to_vec(), json!(true)),
            (b"not an alias".to_vec(), json!(false)),
        ],
        "{raw}"
    );
}

/// A display and its color space named with bytes the binding can't decode: the role naming
/// that color space is still probed.
#[test]
fn the_dump_probes_a_role_of_an_undecodable_display() {
    let config = b"ocio_profile_version: 2.6\nroles:\n  default: raw\n  r: \"d\xff\"\n\
                   use_display_aliases: true\ndisplays:\n  \"d\xff\":\n    - !<View> {name: v, \
                   colorspace: \"d\xff\"}\ncolorspaces:\n  - !<ColorSpace> {name: raw, isdata: \
                   true}\ndisplay_colorspaces:\n  - !<ColorSpace> {name: \"d\xff\", aliases: [x, Q\xff]}\n";
    let response =
        call(json!({"config": {"yaml": bytes_arg(config)}, "calls": [{"dump": "config"}]}));
    let dump = result(&calls(&response)[0]);
    let display = &dump["displays"][0];
    let probes: Vec<Vec<u8>> = probe_pairs(&display["display_probes"])
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert_eq!(
        probes,
        [
            b"D\xff".to_vec(),
            b"x".to_vec(),
            b"X".to_vec(),
            b"Q\xff".to_vec(),
            b"q\xff".to_vec(),
            b"r".to_vec(),
            b"R".to_vec(),
            b"not a display".to_vec(),
        ],
        "{display}"
    );
}

/// A view alias the binding can't decode is probed as its bytes, as they are and with their ASCII
/// letters upper-cased, and found either way (OCIO ignores the case of ASCII letters only).
#[test]
fn the_dump_probes_an_undecodable_view_alias() {
    let config = b"ocio_profile_version: 2.6\nroles: {default: raw}\ncolorspaces:\n  - \
                   !<ColorSpace> {name: raw}\ndisplays:\n  d:\n    - !<View> {name: v, \
                   colorspace: raw, aliases: [u\xffv]}\n";
    let response =
        call(json!({"config": {"yaml": bytes_arg(config)}, "calls": [{"dump": "config"}]}));
    let dump = result(&calls(&response)[0]);
    let view = &dump["displays"][0]["views"][0];
    let probes: Vec<(Vec<u8>, Value)> = view["hasDisplayViewAlias"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), p[1].clone()))
        .collect();
    assert_eq!(
        probes,
        [
            (b"v".to_vec(), json!(false)),
            (b"V".to_vec(), json!(false)),
            (b"u\xffv".to_vec(), json!(true)),
            (b"U\xffV".to_vec(), json!(true)),
            (b"not an alias".to_vec(), json!(false)),
        ],
        "{view}"
    );
}

/// A config whose names the binding can't decode dumps whole: the names come back as their
/// bytes, and the getters that take them are called with those bytes.
#[test]
fn the_dump_reads_undecodable_names() {
    let config = yaml_bytes(b"  - !<ColorSpace>\n    name: a\xffb\n    aliases: [x\xfey]\n");
    let response =
        call(json!({"config": {"yaml": bytes_arg(&config)}, "calls": [{"dump": "config"}]}));
    let dump = result(&calls(&response)[0]);
    let spaces = dump["color_spaces"].as_array().unwrap();
    assert_eq!(bytes(&spaces[1]["name"]), b"a\xffb");
    assert_eq!(spaces[1]["isColorSpaceUsed"], json!(false), "{}", spaces[1]);
    let lookup = dump["lookups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| bytes(&l["name"]) == b"x\xfey")
        .unwrap();
    assert_eq!(lookup["getColorSpace"]["undecodable"], "61ff62", "{lookup}");
}

/// Objects made by a call are stored and reached by later calls, constructors and copies
/// included, with values of every kind.
#[test]
fn objects_are_stored_and_reached() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "Context", "kwargs": {"stringVars": {"map": [["V", "x"]]},
                                          "environmentMode": {"enum": "ENV_ENVIRONMENT_LOAD_PREDEFINED"}},
             "as": "context"},
            {"copy": "context", "as": "copy"},
            {"call": "setStringVar", "on": "copy", "args": ["V", "y"]},
            {"call": "resolveStringVar", "on": "context", "args": ["$V"]},
            {"call": "resolveStringVar", "on": "copy", "args": ["${V}"]},
            {"new": "ColorSpace", "kwargs": {"name": "made"}, "as": "cs"},
            {"call": "setTransform", "on": "cs", "args": [
                {"transform": {"class": "MatrixTransform"}}, {"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "addColorSpace", "args": [{"ref": "cs"}]},
            {"call": "getColorSpace", "args": ["made"], "as": "found"},
            {"call": "getTransform", "on": "found", "args": [{"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "setDefaultLumaCoefs", "args": [[{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, 0.5]]},
            {"call": "getDefaultLumaCoefs"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result_bytes(&c[3]), b"x");
    assert_eq!(result_bytes(&c[4]), b"y");
    assert_eq!(result(&c[9])["class"], "MatrixTransform");
    assert!(bytes(&result(&c[9])["repr"]).starts_with(b"<MatrixTransform"));
    assert_eq!(
        *result(&c[11]),
        json!([{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, {"f64": 0x3fe0_0000_0000_0000u64}])
    );
}

/// Requests it can't run exactly are refused: unknown keys and calls, methods it may not
/// reach, GroupTransform.write without a config (the process's current config would outlive
/// the request), the variables OCIO reads once per process (in any case on Windows), file paths
/// outside the directory, bad value specs.
#[test]
fn bad_requests_are_refused() {
    let mut cases = vec![
        json!({"config": "raw", "unknown": 1}),
        json!({"config": "nope"}),
        json!({"config": {"yaml": 1}}),
        json!({"config": "raw", "calls": [{"frobnicate": 1}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "extra": 1}]}),
        json!({"config": "raw", "calls": [{"call": "_pybind11_conduit_v1_"}]}),
        json!({"config": "raw", "calls": [{"call": "noSuchMethod"}]}),
        json!({"config": "raw", "calls": [{"call": "SetCurrentConfig", "on": "OCIO",
                                           "args": [{"ref": "config"}]}]}),
        json!({"config": "raw", "calls": [{"call": "GetCurrentConfig", "on": "OCIO"}]}),
        json!({"config": "raw", "calls": [{"new": "GroupTransform", "as": "g"},
                                          {"call": "write", "on": "g",
                                           "args": ["Color Transform Format"]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "on": "missing"}]}),
        json!({"config": "raw", "calls": [{"call": "setName", "args": [1]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "args": [{"what": 1}]}]}),
        json!({"config": "raw", "calls": [{"new": "Context", "kwargs": {"stringVars": {"map": "x"}}}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "args": [{"f64": -1}]}]}),
        json!({"config": "raw", "calls": [{"call": "getProcessor", "args": [
            {"transform": {"class": "NoSuchTransform"}}]}]}),
        json!({"config": "raw", "env": {"OCIO_LOGGING_LEVEL": "debug"}}),
        json!({"config": "raw", "env": {"OCIO_DISABLE_ALL_CACHES": "1"}}),
        json!({"config": "raw", "env": ["x"]}),
        json!({"config": "raw", "files": {"../escape.ocio": "x"}}),
        json!({"config": "raw", "files": {"/abs.ocio": "x"}}),
    ];
    if cfg!(windows) {
        cases.push(json!({"config": "raw", "env": {"ocio_logging_level": "debug"}}));
        cases.push(json!({"config": "raw", "files": {"a\\..\\b": "x"}}));
    }
    for args in cases {
        assert!(
            Oracle::get()
                .try_call("config_calls", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}

/// On Linux a backslash is a file name's byte, not a separator.
#[cfg(target_os = "linux")]
#[test]
fn a_backslash_is_a_name_on_linux() {
    let response = call(json!({
        "config": {"file": "a\\b.ocio"},
        "files": {"a\\b.ocio": yaml("")},
        "calls": [{"call": "getWorkingDir"}],
    }));
    assert_eq!(
        result_bytes(&calls(&response)[0]),
        response["dir"].as_str().unwrap().as_bytes()
    );
}

/// ColorSpaceSet's operators are reached (`__or__`, `__and__`, `__sub__`, `__eq__`,
/// `__ne__`): the sets of upstream's `OCIO_ADD_TEST(ColorSpaceSet, operations_on_set)`
/// (tests/cpu/ColorSpaceSet_tests.cpp:205-316 @ v2.5.2), with its counts and names.
#[test]
fn color_space_set_operators_are_reached() {
    let mut requests = vec![json!({"new": "Config", "as": "c"})];
    for (name, categories) in [
        ("cs1", &[][..]),
        ("cs2", &["linear", "rendering"][..]),
        ("cs3", &["log", "rendering"][..]),
    ] {
        requests.push(json!({"new": "ColorSpace", "as": name}));
        requests.push(json!({"call": "setName", "on": name, "args": [name]}));
        for category in categories {
            requests.push(json!({"call": "addCategory", "on": name, "args": [category]}));
        }
        requests.push(json!({"call": "addColorSpace", "on": "c", "args": [{"ref": name}]}));
    }
    let set = |name: &str, category: Value| json!({"call": "getColorSpaces", "on": "c", "args": [category], "as": name});
    requests.push(set("css1", Value::Null));
    requests.push(set("css2", json!("linear")));
    requests.push(set("css3", json!("log")));
    requests.push(set("css5", json!("rendering")));
    let op = |op: &str, a: &str, b: &str, store: &str| json!({"call": op, "on": a, "args": [{"ref": b}], "as": store});
    let first = requests.len();
    let ops = [
        op("__or__", "css2", "css3", "u23"),
        op("__or__", "css1", "css2", "u12"),
        op("__and__", "css2", "css3", "i23"),
        op("__and__", "css2", "css1", "i21"),
        op("__sub__", "css1", "css3", "d13"),
        op("__sub__", "css1", "css2", "d12"),
        op("__sub__", "css1", "u23", "d1u"),
        op("__sub__", "css1", "css5", "d15"),
        op("__and__", "d15", "u23", "nested"),
        op("__eq__", "css2", "i21", "eq"),
        op("__ne__", "css2", "i21", "ne"),
    ];
    requests.extend(ops);
    let named = [
        "u23", "u12", "i23", "i21", "d13", "d12", "d1u", "css5", "nested",
    ];
    for set in named {
        requests.push(json!({"call": "getColorSpaceNames", "on": set}));
    }
    let response = call(json!({"config": "raw", "calls": requests}));
    let c = calls(&response);
    let names = |k: usize| -> Vec<Vec<u8>> {
        result(&c[first + 11 + k])
            .as_array()
            .unwrap()
            .iter()
            .map(bytes)
            .collect()
    };
    for k in 0..9 {
        assert_eq!(result(&c[first + k])["class"], "ColorSpaceSet", "{k}");
    }
    assert_eq!(names(0).len(), 2);
    assert_eq!(names(1).len(), 3);
    assert_eq!(names(2).len(), 0);
    assert_eq!(names(3), [b"cs2".to_vec()]);
    assert_eq!(names(4), [b"cs1".to_vec(), b"cs2".to_vec()]);
    assert_eq!(names(5), [b"cs1".to_vec(), b"cs3".to_vec()]);
    assert_eq!(names(6), [b"cs1".to_vec()]);
    assert_eq!(names(7), [b"cs2".to_vec(), b"cs3".to_vec()]);
    assert_eq!(names(8).len(), 0);
    let eq = result(&c[first + 9]).as_bool().unwrap();
    assert_eq!(*result(&c[first + 10]), json!(!eq));
}

/// The dump of a named transform calls `NamedTransform.GetTransform` in both directions only
/// when it has a transform: for one with none, the wheel would dereference null
/// (NamedTransform.cpp:182-221 @ v2.5.2) and end the oracle.
#[test]
fn a_named_transform_without_transforms_dumps() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "NamedTransform", "as": "empty"},
            {"dump": "empty"},
            {"new": "NamedTransform", "as": "one"},
            {"call": "setTransform", "on": "one", "args": [
                {"transform": {"class": "MatrixTransform"}}, {"enum": "TRANSFORM_DIR_FORWARD"}]},
            {"dump": "one"},
        ],
    }));
    let c = calls(&response);
    let get_transforms = |dump: &Value| -> Vec<Value> {
        dump["keyed"]
            .as_array()
            .unwrap_or_else(|| panic!("{dump}"))
            .iter()
            .filter(|k| k[0] == "GetTransform")
            .map(|k| k[2].clone())
            .collect()
    };
    let empty = result(&c[1]);
    assert_eq!(empty["class"], "NamedTransform");
    assert!(get_transforms(empty).is_empty(), "{empty}");
    let one = get_transforms(result(&c[4]));
    assert_eq!(one.len(), 2);
    for got in one {
        assert_eq!(got["class"], "MatrixTransform", "{got}");
    }
}
