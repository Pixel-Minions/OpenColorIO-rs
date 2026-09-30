// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! A test-only replay of OCIO 2.5.2's config writer (`OCIOYaml.cpp` `save` functions and
//! `OCIOYaml::Write`, lines 219-230, 350-372, 480-535, 577-3663, 3721-4068, 4206-4393,
//! 5033-5448) onto the ported yaml-cpp emitter, driven by a parsed `serialize()` output
//! instead of a `Config`.
//!
//! It makes the calls OCIO makes: the same sections in the same order, the same `Newline`s,
//! `Literal` for multi-line descriptions and interchange attributes (after
//! `SanitizeNewlines`), and each value written with the C++ type OCIO writes it as: `bool`,
//! `char` (`family_separator`), `float` (allocation variables and curve points) at precision
//! 7, `double` (every other number) at precision 15, and strings. Collection styles come
//! from the tree, which records what OCIO requested. The config model itself is Phase 3;
//! this proves the emitter.

use std::collections::VecDeque;

use ocio::yaml_cpp::{Emitter, EmitterManip::*, verbatim_tag};

use super::yaml_tree::{Node, Style};

/// How OCIO types a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Str,
    Bool,
    Char,
    Float,
    Double,
}

/// A number with the C++ type OCIO writes it as.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Number {
    F32(f32),
    F64(f64),
}

/// A number as yaml-cpp writes it (`.nan`, `.inf`, `-.inf` or decimal text), read back as
/// the nearest value of its type: the value written when the exact ones are not given.
fn number<T: std::str::FromStr>(text: &str) -> T {
    let rust = match text {
        ".nan" => "NaN",
        ".inf" => "inf",
        "-.inf" => "-inf",
        other => other,
    };
    rust.parse()
        .ok()
        .unwrap_or_else(|| panic!("not a number: {text:?}"))
}

/// Port of `SanitizeNewlines` (OCIOYaml.cpp:37-61): drops trailing newlines.
pub(crate) fn sanitize_newlines(input: &str) -> String {
    input.trim_end_matches('\n').to_string()
}

/// The type OCIO writes a transform's key with (the transforms' `save` functions).
fn transform_kind(key: &str) -> Kind {
    match key {
        "vars" | "control_points" | "slopes" => Kind::Float,
        "data_bypass" | "looks_bypass" | "lintolog_bypass" => Kind::Bool,
        "slope" | "offset" | "power" | "sat" | "value" | "gamma" | "exposure" | "contrast"
        | "pivot" | "log_exposure_step" | "log_midway_gray" | "params" | "brightness"
        | "saturation" | "lift" | "gain" | "black" | "white" | "rgb" | "master" | "start"
        | "center" | "width" | "s_contrast" | "base" | "log_side_slope" | "log_side_offset"
        | "lin_side_slope" | "lin_side_offset" | "lin_side_break" | "linear_slope" | "matrix"
        | "min_in_value" | "max_in_value" | "min_out_value" | "max_out_value" => Kind::Double,
        _ => Kind::Str,
    }
}

/// The replayed `YAML::Emitter out`.
pub(crate) struct OcioWriter {
    pub(crate) out: Emitter,
    /// How many numbers went through the emitter's float and double formatting.
    pub(crate) numbers: usize,
    /// The exact numbers the config holds, in the order OCIO writes them; without them the
    /// tree's text is read back.
    pub(crate) values: Option<VecDeque<Number>>,
}

impl Default for OcioWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl OcioWriter {
    /// `OCIOYaml::Write` (OCIOYaml.cpp:5441-5448): double precision `digits10` (15), float
    /// precision 7.
    pub(crate) fn new() -> OcioWriter {
        let mut out = Emitter::new();
        out.set_double_precision(15);
        out.set_float_precision(7);
        OcioWriter {
            out,
            numbers: 0,
            values: None,
        }
    }

    /// The next exact number, if they were given.
    fn next_value(&mut self, text: &str) -> Option<Number> {
        let values = self.values.as_mut()?;
        let value = values.pop_front();
        assert!(value.is_some(), "no exact number left for {text:?}");
        value
    }

    /// `Config::serialize`: `ostream << out.c_str()` after writing the whole config.
    pub(crate) fn serialize(config: &Node) -> Vec<u8> {
        let mut w = OcioWriter::new();
        w.save_config(config);
        w.out.c_str().to_vec()
    }

    /// The text written, which is UTF-8 whenever every string written was.
    pub(crate) fn text(&self) -> &str {
        std::str::from_utf8(self.out.c_str()).expect("the output is UTF-8")
    }

    fn scalar(&mut self, kind: Kind, node: &Node) {
        let text = node.text();
        match kind {
            Kind::Str => {
                self.out.put(text);
            }
            Kind::Bool => {
                let b = match text {
                    "true" => true,
                    "false" => false,
                    other => panic!("not a bool: {other:?}"),
                };
                self.out.put(b);
            }
            Kind::Char => {
                assert_eq!(text.len(), 1, "a char: {text:?}");
                self.out.put(text.as_bytes()[0]);
            }
            Kind::Float => {
                let v = match self.next_value(text) {
                    None => number::<f32>(text),
                    Some(Number::F32(v)) => v,
                    Some(other) => panic!("{text:?} is a float, the exact value is {other:?}"),
                };
                self.out.put(v);
                self.numbers += 1;
            }
            Kind::Double => {
                let v = match self.next_value(text) {
                    None => number::<f64>(text),
                    Some(Number::F64(v)) => v,
                    Some(other) => panic!("{text:?} is a double, the exact value is {other:?}"),
                };
                self.out.put(v);
                self.numbers += 1;
            }
        }
    }

    /// A vector written with `out << vec` (after `YAML::Flow` when the tree says so).
    fn seq(&mut self, kind: Kind, node: &Node) {
        if node.style() == Style::Flow {
            self.out.put(Flow);
        }
        self.out.put(BeginSeq);
        for item in node.items() {
            self.scalar(kind, item);
        }
        self.out.put(EndSeq);
    }

    fn key(&mut self, key: &str) {
        self.out.put(Key).put(key).put(Value);
    }

    fn key_value(&mut self, key: &str, kind: Kind, node: &Node) {
        self.key(key);
        match node {
            Node::Scalar { .. } => self.scalar(kind, node),
            Node::Seq { .. } => self.seq(kind, node),
            Node::Map { .. } => panic!("{key} is a map"),
        }
    }

    /// `saveDescription` (OCIOYaml.cpp:219-230).
    ///
    /// The tree holds a description only where OCIO's `desc && *desc` held, and after
    /// `SanitizeNewlines`: a description of newlines alone was written as `""`.
    fn save_description(&mut self, desc: &str) {
        let desc = sanitize_newlines(desc);
        self.out.put(Key).put("description").put(Value);
        if desc.contains('\n') {
            self.out.put(Literal);
        }
        self.out.put(&desc);
    }

    /// `saveInterchangeAttributes` (OCIOYaml.cpp:350-372).
    fn save_interchange(&mut self, map: &Node) {
        if map.entries().is_empty() {
            return;
        }
        self.out
            .put(Key)
            .put("interchange")
            .put(Value)
            .put(BeginMap);
        for (k, v) in map.entries() {
            let value = sanitize_newlines(v.text());
            self.out.put(Key).put(k).put(Value);
            if value.contains('\n') {
                self.out.put(Literal);
            }
            self.out.put(&value);
        }
        self.out.put(EndMap);
    }

    /// `save(YAML::Emitter &, ConstTransformRcPtr, ...)` (OCIOYaml.cpp:3353-3420) and the
    /// per-type `save` functions (OCIOYaml.cpp:577-3192): a verbatim tag, a flow map
    /// (GroupTransform, and the grading transforms with non-default values, are block), the
    /// keys in order.
    fn save_transform(&mut self, t: &Node) {
        let tag = t.tag().expect("a transform tag");
        self.out.put(verbatim_tag(tag));
        if tag == "GroupTransform" {
            assert_eq!(t.style(), Style::Block, "GroupTransform is a block map");
        } else if !tag.starts_with("Grading") {
            assert_eq!(t.style(), Style::Flow, "{tag} is a flow map");
        }
        self.transform_body(t);
    }

    fn transform_body(&mut self, t: &Node) {
        if t.style() == Style::Flow {
            self.out.put(Flow);
        }
        self.out.put(BeginMap);
        for (key, value) in t.entries() {
            if key == "children" {
                // GroupTransform (OCIOYaml.cpp:2569-2577).
                self.out.put(Key).put("children").put(Value).put(BeginSeq);
                for child in value.items() {
                    self.save_transform(child);
                }
                self.out.put(EndSeq);
                continue;
            }
            match value {
                Node::Map { .. } => {
                    // The grading parameter maps and curves.
                    self.out.put(Key).put(key.as_str()).put(Value);
                    self.transform_body(value);
                }
                Node::Scalar { .. }
                    if matches!(key.as_str(), "style" | "direction" | "allocation") =>
                {
                    // `out << YAML::Value << YAML::Flow << ...` (OCIOYaml.cpp:518, 583, 637,
                    // 1009, 1185, ...): a local flow setting the scalar then clears.
                    self.key(key);
                    self.out.put(Flow);
                    self.scalar(Kind::Str, value);
                }
                _ => self.key_value(key, transform_kind(key), value),
            }
        }
        self.out.put(EndMap);
    }

    /// `save(YAML::Emitter &, const View &)` (OCIOYaml.cpp:480-505).
    fn save_view(&mut self, view: &Node) {
        assert_eq!(view.tag(), Some("View"));
        self.out.put(verbatim_tag("View")).put(Flow).put(BeginMap);
        for (key, value) in view.entries() {
            if key == "description" {
                self.save_description(value.text());
            } else {
                self.key_value(key, Kind::Str, value);
            }
        }
        self.out.put(EndMap);
    }

    /// `out << VerbatimTag("Views"); out << Flow << sharedViews;` (OCIOYaml.cpp:5222-5228).
    fn save_views(&mut self, views: &Node) {
        assert_eq!(views.tag(), Some("Views"));
        self.out.put(verbatim_tag("Views"));
        self.out.put(Flow);
        self.out.put(BeginSeq);
        for v in views.items() {
            self.scalar(Kind::Str, v);
        }
        self.out.put(EndSeq);
    }

    /// A display's or the virtual display's list of views.
    fn save_view_list(&mut self, list: &Node) {
        self.out.put(BeginSeq);
        for item in list.items() {
            match item.tag() {
                Some("View") => self.save_view(item),
                Some("Views") => self.save_views(item),
                other => panic!("unexpected view entry tag {other:?}"),
            }
        }
        self.out.put(EndSeq);
    }

    /// `save(YAML::Emitter &, ConstFileRulesRcPtr &, size_t)` and the viewing rules'
    /// (OCIOYaml.cpp:4206-4247, 4340-4393).
    fn save_rule(&mut self, rule: &Node) {
        assert_eq!(rule.tag(), Some("Rule"));
        self.out.put(verbatim_tag("Rule")).put(Flow).put(BeginMap);
        for (key, value) in rule.entries() {
            if key == "custom" {
                self.out.put(Key).put("custom").put(Value).put(BeginMap);
                for (k, v) in value.entries() {
                    self.out.put(Key).put(k).put(Value).put(v.text());
                }
                self.out.put(EndMap);
            } else {
                self.key_value(key, Kind::Str, value);
            }
        }
        self.out.put(EndMap);
    }

    /// The reference transforms and `transform`/`inverse_transform`.
    fn save_transform_entry(&mut self, key: &str, value: &Node) {
        self.out.put(Key).put(key).put(Value);
        self.save_transform(value);
    }

    /// `save(YAML::Emitter &, ConstColorSpaceRcPtr, unsigned)` (OCIOYaml.cpp:3576-3663),
    /// `save(... ConstLookRcPtr ...)` (3721-3746), `save(... ConstViewTransformRcPtr ...)`
    /// (3881-3923) and `save(... ConstNamedTransformRcPtr ...)` (4008-4068): a tagged block
    /// map, then a `Newline`.
    fn save_element(&mut self, element: &Node) {
        let tag = element.tag().expect("a tagged element");
        assert!(matches!(
            tag,
            "ColorSpace" | "Look" | "ViewTransform" | "NamedTransform"
        ));
        assert_eq!(element.style(), Style::Block);
        self.out.put(verbatim_tag(tag)).put(BeginMap);
        for (key, value) in element.entries() {
            match key.as_str() {
                "description" => self.save_description(value.text()),
                "interchange" => self.save_interchange(value),
                "isdata" => self.key_value(key, Kind::Bool, value),
                "allocationvars" => self.key_value(key, Kind::Float, value),
                "aliases" | "categories" => {
                    assert_eq!(value.style(), Style::Flow);
                    self.key_value(key, Kind::Str, value);
                }
                "transform"
                | "inverse_transform"
                | "to_reference"
                | "from_reference"
                | "to_scene_reference"
                | "from_scene_reference"
                | "to_display_reference"
                | "from_display_reference" => self.save_transform_entry(key, value),
                _ => self.key_value(key, Kind::Str, value),
            }
        }
        self.out.put(EndMap);
        self.out.put(Newline);
    }

    /// A section holding a sequence of elements.
    fn save_elements(&mut self, key: &str, list: &Node) {
        self.out.put(Key).put(key).put(Value).put(BeginSeq);
        for element in list.items() {
            self.save_element(element);
        }
        self.out.put(EndSeq);
    }

    /// `save(YAML::Emitter &, const Config &)` (OCIOYaml.cpp:5033-5419).
    pub(crate) fn save_config(&mut self, root: &Node) {
        let mut sections = Sections { root, next: 0 };

        self.out.put(Block).put(BeginMap);

        let version = sections
            .take("ocio_profile_version")
            .expect("ocio_profile_version");
        let major: u32 = version
            .text()
            .split('.')
            .next()
            .and_then(|m| m.parse().ok())
            .expect("a major version");
        self.key_value("ocio_profile_version", Kind::Str, version);
        self.out.put(Newline).put(Newline);

        if let Some(env) = sections.take("environment") {
            self.out.put(Key).put("environment");
            self.out.put(Value).put(BeginMap);
            for (name, value) in env.entries() {
                self.out.put(Key).put(name.as_str());
                self.out.put(Value).put(value.text());
            }
            self.out.put(EndMap);
            self.out.put(Newline);
        }

        let search_path = sections.take("search_path").expect("search_path");
        self.out.put(Key).put("search_path").put(Value);
        match search_path {
            Node::Seq { .. } => {
                // out << searchPaths: a block sequence.
                self.out.put(BeginSeq);
                for p in search_path.items() {
                    self.scalar(Kind::Str, p);
                }
                self.out.put(EndSeq);
            }
            _ => self.scalar(Kind::Str, search_path),
        }

        let strict = sections.take("strictparsing").expect("strictparsing");
        self.key_value("strictparsing", Kind::Bool, strict);

        if let Some(sep) = sections.take("family_separator") {
            self.key_value("family_separator", Kind::Char, sep);
        }

        let luma = sections.take("luma").expect("luma");
        self.out.put(Key).put("luma").put(Value).put(Flow);
        self.out.put(BeginSeq);
        for v in luma.items() {
            self.scalar(Kind::Double, v);
        }
        self.out.put(EndSeq);

        if major >= 2
            && let Some(name) = sections.take("name")
        {
            self.key_value("name", Kind::Str, name);
        }
        if let Some(desc) = sections.take("description") {
            self.save_description(desc.text());
        }

        // Roles
        self.out.put(Newline).put(Newline);
        let roles = sections.take("roles").expect("roles");
        self.out.put(Key).put("roles");
        self.out.put(Value).put(BeginMap);
        for (role, cs) in roles.entries() {
            self.out.put(Key).put(role.as_str());
            self.out.put(Value).put(cs.text());
        }
        self.out.put(EndMap);
        self.out.put(Newline);

        // File rules
        if major >= 2 {
            let rules = sections.take("file_rules").expect("file_rules");
            self.out.put(Newline);
            self.out.put(Key).put("file_rules");
            self.out.put(Value).put(BeginSeq);
            for rule in rules.items() {
                self.save_rule(rule);
            }
            self.out.put(EndSeq);
            self.out.put(Newline);
        }

        // Viewing rules
        if major >= 2
            && let Some(rules) = sections.take("viewing_rules")
        {
            self.out.put(Newline);
            self.out.put(Key).put("viewing_rules");
            self.out.put(Value).put(BeginSeq);
            for rule in rules.items() {
                self.save_rule(rule);
            }
            self.out.put(EndSeq);
            self.out.put(Newline);
        }

        // Shared views
        if let Some(views) = sections.take("shared_views") {
            self.out.put(Newline);
            self.out.put(Key).put("shared_views");
            self.out.put(Value);
            self.save_view_list(views);
            self.out.put(Newline);
        }

        // Displays
        let displays = sections.take("displays").expect("displays");
        self.out.put(Newline);
        self.out.put(Key).put("displays");
        self.out.put(Value).put(BeginMap);
        for (display, views) in displays.entries() {
            self.out.put(Key).put(display.as_str());
            self.out.put(Value);
            self.save_view_list(views);
        }
        self.out.put(EndMap);

        // Virtual display
        if major >= 2
            && let Some(views) = sections.take("virtual_display")
        {
            self.out.put(Newline).put(Newline);
            self.out.put(Key).put("virtual_display");
            self.out.put(Value);
            self.save_view_list(views);
        }

        self.out.put(Newline).put(Newline);
        for key in ["active_displays", "active_views"] {
            let list = sections.take(key).expect("active displays and views");
            self.out.put(Key).put(key);
            self.out.put(Value).put(Flow);
            self.out.put(BeginSeq);
            for v in list.items() {
                self.scalar(Kind::Str, v);
            }
            self.out.put(EndSeq);
        }
        if let Some(list) = sections.take("inactive_colorspaces") {
            self.out.put(Key).put("inactive_colorspaces");
            self.out.put(Value).put(Flow);
            self.out.put(BeginSeq);
            for v in list.items() {
                self.scalar(Kind::Str, v);
            }
            self.out.put(EndSeq);
        }
        self.out.put(Newline);

        // Looks
        if let Some(looks) = sections.take("looks") {
            self.out.put(Newline);
            self.save_elements("looks", looks);
            self.out.put(Newline);
        }

        // View transforms
        if let Some(vt) = sections.take("default_view_transform") {
            self.out.put(Newline);
            self.key_value("default_view_transform", Kind::Str, vt);
            self.out.put(Newline);
        }
        if let Some(vts) = sections.take("view_transforms") {
            self.out.put(Newline);
            self.save_elements("view_transforms", vts);
        }

        // Display color spaces, color spaces, named transforms
        if let Some(cs) = sections.take("display_colorspaces") {
            self.out.put(Newline);
            self.save_elements("display_colorspaces", cs);
        }
        let cs = sections.take("colorspaces").expect("colorspaces");
        self.out.put(Newline);
        self.save_elements("colorspaces", cs);
        if let Some(nt) = sections.take("named_transforms") {
            self.out.put(Newline);
            self.save_elements("named_transforms", nt);
        }

        self.out.put(EndMap);
        sections.finish();
    }
}

/// The top-level sections of a parsed config, taken in OCIO's order.
struct Sections<'a> {
    root: &'a Node,
    next: usize,
}

impl<'a> Sections<'a> {
    /// The section `name` if the config has it; it must be the next one.
    fn take(&mut self, name: &str) -> Option<&'a Node> {
        match self.root.entries().get(self.next) {
            Some((key, node)) if key == name => {
                self.next += 1;
                Some(node)
            }
            _ => {
                assert!(
                    self.root.get(name).is_none(),
                    "section {name} is not where OCIO writes it"
                );
                None
            }
        }
    }

    /// Checks that every section was written.
    fn finish(&self) {
        let rest: Vec<&str> = self.root.entries()[self.next..]
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert!(rest.is_empty(), "sections OCIO does not write: {rest:?}");
    }
}
