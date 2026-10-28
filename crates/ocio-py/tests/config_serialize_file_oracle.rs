// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Config.serialize(fileName), PyConfig.cpp:242-249 @ v2.5.2: open a text-mode
//! ofstream before serialization, close it afterwards, and return None. Exact
//! output bytes/errors come only from the wheel. This dedicated test binary owns
//! its process CWD; no other Config test binary's CWD or files are changed.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, i, n, none, r, ra, s};

const CONFIG: &str = r#"ocio_profile_version: 2.1

environment:
  SHOT: "042"
  SEQ: ab

search_path: [luts, "shots/$SHOT"]
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]
name: probe
description: A probe config.

roles:
  default: raw
  scene_linear: lin
  color_timing: log

file_rules:
  - !<Rule> {name: LogRule, colorspace: log, pattern: "*_log*", extension: "*"}
  - !<Rule> {name: Default, colorspace: raw}

viewing_rules:
  - !<Rule> {name: scene, colorspaces: [lin, log]}
  - !<Rule> {name: enc, encodings: [log]}

shared_views:
  - !<View> {name: shared1, view_transform: vt1, display_colorspace: <USE_DISPLAY_NAME>}
  - !<View> {name: shared2, colorspace: raw}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Film, view_transform: vt1, display_colorspace: disp, looks: +look1, rule: scene, description: A film view.}
    - !<Views> [shared2]
  disp:
    - !<View> {name: Log, colorspace: log}
    - !<Views> [shared1]

virtual_display:
  - !<View> {name: VRaw, colorspace: raw}
  - !<View> {name: VFilm, view_transform: vt1, display_colorspace: <USE_DISPLAY_NAME>, looks: look1, rule: enc, description: virtual}
  - !<Views> [shared2]

active_displays: [sRGB, disp]
active_views: [Film, Raw, Log, shared1]
inactive_colorspaces: [nt2, hidden]
default_view_transform: vt1

looks:
  - !<Look>
    name: look1
    process_space: log
    description: A look.
    transform: !<MatrixTransform> {offset: [0.1, 0.2, 0.3, 0]}
  - !<Look>
    name: look2
    process_space: lin
    inverse_transform: !<ExponentTransform> {value: [2, 2, 2, 1]}

view_transforms:
  - !<ViewTransform>
    name: vt1
    family: vts
    categories: [a, b]
    from_scene_reference: !<MatrixTransform> {offset: [0.01, 0, 0, 0]}
  - !<ViewTransform>
    name: vt2
    from_display_reference: !<RangeTransform> {min_in_value: 0, min_out_value: 0}

display_colorspaces:
  - !<ColorSpace>
    name: disp
    aliases: [display, d]
    family: display
    categories: [file-io]
    encoding: sdr-video
    from_display_reference: !<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}

colorspaces:
  - !<ColorSpace>
    name: raw
    family: utility
    categories: [file-io, working-space]
    isdata: true
    encoding: data
  - !<ColorSpace>
    name: lin
    aliases: [linear, scene-linear]
    family: scene/linear
    categories: [working-space]
    encoding: scene-linear
    bitdepth: 32f
  - !<ColorSpace>
    name: log
    aliases: [lg]
    family: scene/log
    encoding: log
    to_scene_reference: !<LogTransform> {base: 2, direction: inverse}
  - !<ColorSpace>
    name: hidden
    categories: [file-io]
    from_scene_reference: !<MatrixTransform> {offset: [0.5, 0, 0, 0]}

named_transforms:
  - !<NamedTransform>
    name: nt1
    aliases: [n1]
    categories: [working-space]
    encoding: log
    transform: !<LogTransform> {base: 10}
  - !<NamedTransform>
    name: nt2
    inverse_transform: !<MatrixTransform> {offset: [0, 0.1, 0, 0]}
"#;

fn read(sc: &mut Script, path: &str, expect: Expect) {
    sc.call_kw(
        n("numpy.fromfile"),
        vec![s(path)],
        vec![("dtype", s("u1"))],
        expect,
    );
}

fn script() -> Script {
    let mut sc = Script::new();
    let cfg = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(CONFIG)]);
    sc.call(ra(cfg, "setDescription"), vec![s("two\nlines")]);
    sc.call(ra(cfg, "serialize"), vec![]);
    sc.call(ra(cfg, "serialize"), vec![s("out.ocio")]);
    read(&mut sc, "out.ocio", Expect::Value);
    sc.call_kw(
        ra(cfg, "serialize"),
        vec![],
        vec![("fileName", s("kw.ocio"))],
        Expect::Value,
    );
    read(&mut sc, "kw.ocio", Expect::Value);
    sc.call_kw(
        n("OCIO.Config.serialize"),
        vec![],
        vec![("self", r(cfg)), ("fileName", s("unbound.ocio"))],
        Expect::Value,
    );
    read(&mut sc, "unbound.ocio", Expect::Value);
    sc.call(ra(cfg, "serialize"), vec![by(b"bytes.ocio")]);
    read(&mut sc, "bytes.ocio", Expect::Value);
    sc.call(ra(cfg, "serialize"), vec![s("cut.ocio\0tail")]);
    read(&mut sc, "cut.ocio", Expect::Value);
    let back = sc.call(n("OCIO.Config.CreateFromFile"), vec![s("out.ocio")]);
    sc.call(ra(back, "getDescription"), vec![]);
    sc.call(ra(back, "serialize"), vec![]);
    // Failed-open output streams do not throw by themselves. The resulting
    // absence/read error is compared too, rather than treating None as proof.
    sc.call(ra(cfg, "serialize"), vec![s("missing/out.ocio")]);
    read(&mut sc, "missing/out.ocio", Expect::Raised);
    sc.call(ra(cfg, "serialize"), vec![s(".")]);
    // Same existing version-inconsistent input as config_oracle.rs:245-248.
    // A serialization exception comes after open/truncate, including when a
    // previously written file is the destination. Actual empty bytes are read.
    let bad = sc.call(n("OCIO.Config.CreateFromStream"), vec![s(CONFIG)]);
    sc.call(ra(bad, "setMajorVersion"), vec![i(1)]);
    sc.raises(ra(bad, "serialize"), vec![s("bad.ocio")]);
    read(&mut sc, "bad.ocio", Expect::Value);
    sc.raises(ra(bad, "serialize"), vec![s("out.ocio")]);
    read(&mut sc, "out.ocio", Expect::Value);
    sc.raises(ra(bad, "serialize"), vec![s("missing/bad.ocio")]);
    read(&mut sc, "missing/bad.ocio", Expect::Raised);
    for bad_arg in [none(), i(7)] {
        sc.raises(ra(cfg, "serialize"), vec![bad_arg]);
    }
    sc.call_kw(
        ra(cfg, "serialize"),
        vec![],
        vec![("wrong", s("unused.ocio"))],
        Expect::Raised,
    );
    sc.call_kw(
        ra(cfg, "serialize"),
        vec![s("unused.ocio")],
        vec![("fileName", s("also-unused.ocio"))],
        Expect::Raised,
    );
    sc.call(ra(cfg, "serialize"), vec![]);
    sc
}

struct OwnCwd {
    previous: std::path::PathBuf,
    own: std::path::PathBuf,
}

impl Drop for OwnCwd {
    fn drop(&mut self) {
        // Restore even on assertion failure; retain that failure's file witness.
        let _ = std::env::set_current_dir(&self.previous);
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.own);
        }
    }
}

#[test]
fn config_serialize_to_a_file_matches_the_wheel() {
    let previous = std::env::current_dir().unwrap();
    let own = std::env::temp_dir().join(format!(
        "ocio-py-config-serialize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    // create_dir must succeed: an existing path is never removed or reused.
    std::fs::create_dir(&own).unwrap();
    let _cwd = OwnCwd { previous, own };
    std::env::set_current_dir(&_cwd.own).unwrap();
    compare(&[script()]);
}
