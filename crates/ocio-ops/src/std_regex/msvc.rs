// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Translated from the Microsoft C++ Standard Library, `<regex>` as shipped with MSVC
// 14.44.35207 (https://github.com/microsoft/STL):
//     Copyright (c) Microsoft Corporation.
//     SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception

//! `std::regex` as the Windows wheel compiles it: a translation of Microsoft's STL `<regex>`
//! (MSVC 14.44.35207), for `char`, `regex_traits<char>` in the classic locale, and the
//! ECMAScript grammar with no other flag, as OCIO constructs every `std::regex`.
//!
//! The parser (`_Parser`) builds a node graph (`_Builder`): a doubly linked list of nodes,
//! with alternatives and assertions hanging off their own nodes. The list is kept as in the
//! STL, with indices into an arena in place of pointers. The branches of the parser and the
//! builder that only the other grammars (basic, extended, awk, grep, egrep) or the `icase`,
//! `collate` and `nosubs` flags reach are left out; each function says which.
//!
//! Line numbers are those of `include/regex` in MSVC 14.44.35207.

use super::{ErrorType, RegexError};

/// `regex_error(code)`, as `_Xregex_error` throws it, with its `what()`.
///
/// Port of `_Xregex_error` and `regex_error::_Stringify` (`<regex>`:468-523). `_Xregex_error`
/// is exported by `msvcp140.dll`, so the text is the runtime library's; it is the same in the
/// headers of 14.44 and in the `msvcp140.dll` the wheel loads.
pub(super) fn error(code: ErrorType) -> RegexError {
    let what = match code {
        ErrorType::Collate => {
            "regex_error(error_collate): The expression contained an invalid collating element \
             name."
        }
        ErrorType::Ctype => {
            "regex_error(error_ctype): The expression contained an invalid character class name."
        }
        ErrorType::Escape => {
            "regex_error(error_escape): The expression contained an invalid escaped character, \
             or a trailing escape."
        }
        ErrorType::Backref => {
            "regex_error(error_backref): The expression contained an invalid back reference."
        }
        ErrorType::Brack => {
            "regex_error(error_brack): The expression contained mismatched [ and ]."
        }
        ErrorType::Paren => {
            "regex_error(error_paren): The expression contained mismatched ( and )."
        }
        ErrorType::Brace => {
            "regex_error(error_brace): The expression contained mismatched { and }."
        }
        ErrorType::Badbrace => {
            "regex_error(error_badbrace): The expression contained an invalid range in a {} \
             expression."
        }
        ErrorType::Range => {
            "regex_error(error_range): The expression contained an invalid character range, \
             such as [b-a] in most encodings."
        }
        ErrorType::Space => {
            "regex_error(error_space): There was insufficient memory to convert the expression \
             into a finite state machine."
        }
        ErrorType::Badrepeat => {
            "regex_error(error_badrepeat): One of *?+{ was not preceded by a valid regular \
             expression."
        }
        ErrorType::Complexity => {
            "regex_error(error_complexity): The complexity of an attempted match against a \
             regular expression exceeded a pre-set level."
        }
        ErrorType::Stack => {
            "regex_error(error_stack): There was insufficient memory to determine whether the \
             regular expression could match the specified character sequence."
        }
        ErrorType::Syntax => "regex_error(error_syntax)",
    };
    RegexError { code, what }
}

// ---------------------------------------------------------------------------------------------
// regex_traits<char> in the classic locale

/// `ctype_base::mask`, a `short` in MSVC's STL: the bits of the UCRT's `_ctype` table.
pub(super) type Mask = i16;

// The UCRT's classification bits (ucrt/corecrt_wctype.h:48-56, Windows SDK 10.0.22000.0).
const UPPER: Mask = 0x01;
const LOWER: Mask = 0x02;
const DIGIT: Mask = 0x04;
const SPACE: Mask = 0x08;
const PUNCT: Mask = 0x10;
const CONTROL: Mask = 0x20;
const BLANK: Mask = 0x40;
const HEX: Mask = 0x80;
/// `_XA` (`<xlocale>`:2418), "extra alphabetic"; `_ALPHA` in the UCRT's table.
const XA: Mask = 0x100;

// `ctype_base`'s masks (`<xlocale>`:2432-2443).
const CT_ALNUM: Mask = DIGIT | LOWER | UPPER | XA;
const CT_ALPHA: Mask = LOWER | UPPER | XA;
const CT_CNTRL: Mask = CONTROL;
const CT_DIGIT: Mask = DIGIT;
const CT_GRAPH: Mask = DIGIT | LOWER | PUNCT | UPPER | XA;
const CT_LOWER: Mask = LOWER;
const CT_PRINT: Mask = DIGIT | LOWER | PUNCT | BLANK | UPPER | XA | HEX;
const CT_PUNCT: Mask = PUNCT;
const CT_SPACE: Mask = SPACE | BLANK;
const CT_UPPER: Mask = UPPER;
const CT_XDIGIT: Mask = HEX;
const CT_BLANK: Mask = SPACE | BLANK;
/// The class of `\w` and `[:w:]`: `static_cast<ctype_base::mask>(-1)`, which `isctype` reads
/// as `_` or alphanumeric.
const CT_WORD: Mask = -1;

/// The classic "C" locale's classification of a byte: `ctype<char>::classic_table()`, the
/// UCRT's `_ctype` table of the "C" locale. Bytes 0x80-0xFF have no class.
fn classic_table(c: u8) -> Mask {
    match c {
        0x09..=0x0D => SPACE | CONTROL,
        0x00..=0x1F | 0x7F => CONTROL,
        b' ' => SPACE | BLANK,
        b'0'..=b'9' => DIGIT | HEX,
        b'A'..=b'F' => UPPER | HEX | XA,
        b'G'..=b'Z' => UPPER | XA,
        b'a'..=b'f' => LOWER | HEX | XA,
        b'g'..=b'z' => LOWER | XA,
        0x21..=0x7E => PUNCT,
        _ => 0,
    }
}

/// `ctype<char>::is(mask, c)` in the classic locale: `table[(unsigned char)c] & mask`.
fn ctype_is(mask: Mask, c: u8) -> bool {
    classic_table(c) & mask != 0
}

/// `ctype<char>::tolower(c)` in the classic locale: ASCII only.
fn tolower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}

/// Port of `_Regex_traits::isctype` (`<regex>`:330-337).
pub(super) fn isctype(c: u8, class: Mask) -> bool {
    if class != CT_WORD {
        ctype_is(class, c)
    } else {
        c == b'_' || ctype_is(CT_ALNUM, c)
    }
}

/// Port of `_Regex_traits::lookup_classname` (`<regex>`:339-378) without `icase`: the class
/// named by `name`, compared through `translate_nocase`, or 0.
fn lookup_classname(name: &[u8]) -> Mask {
    const NAMES: [(&[u8], Mask); 15] = [
        (b"alnum", CT_ALNUM),
        (b"alpha", CT_ALPHA),
        (b"blank", CT_BLANK),
        (b"cntrl", CT_CNTRL),
        (b"d", CT_DIGIT),
        (b"digit", CT_DIGIT),
        (b"graph", CT_GRAPH),
        (b"lower", CT_LOWER),
        (b"print", CT_PRINT),
        (b"punct", CT_PUNCT),
        (b"space", CT_SPACE),
        (b"s", CT_SPACE),
        (b"upper", CT_UPPER),
        (b"w", CT_WORD),
        (b"xdigit", CT_XDIGIT),
    ];
    for (n, mask) in NAMES {
        if n.len() == name.len() && n.iter().zip(name).all(|(&a, &b)| tolower(a) == tolower(b)) {
            return mask;
        }
    }
    0
}

/// Port of `_Regex_traits::transform_primary` (`<regex>`:316-328) in the classic locale:
/// `tolower`, then `collate<char>::transform`, which copies the text in the "C" locale.
pub(super) fn transform_primary(s: &[u8]) -> Vec<u8> {
    s.iter().map(|&c| tolower(c)).collect()
}

/// Port of `regex_traits<char>::value` (`<regex>`:422-440).
fn value(ch: i8, base: i32) -> i32 {
    let c = i32::from(ch);
    if (base != 8 && i32::from(b'0') <= c && c <= i32::from(b'9'))
        || (base == 8 && i32::from(b'0') <= c && c <= i32::from(b'7'))
    {
        return c - i32::from(b'0');
    }

    if base != 16 {
        return -1;
    }

    if i32::from(b'a') <= c && c <= i32::from(b'f') {
        return c - i32::from(b'a') + 10;
    }

    if i32::from(b'A') <= c && c <= i32::from(b'F') {
        return c - i32::from(b'A') + 10;
    }

    -1
}

// ---------------------------------------------------------------------------------------------
// The node graph

/// A node's index in [`Program::nodes`]: a `_Node_base *`.
pub(super) type NodeId = usize;

/// Port of `_Node_type` (`<regex>`:1211-1234).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeType {
    Nop,
    Bol,
    Eol,
    Wbound,
    Dot,
    Str,
    Class,
    Group,
    EndGroup,
    Assert,
    NegAssert,
    EndAssert,
    Capture,
    EndCapture,
    Back,
    If,
    Endif,
    Rep,
    EndRep,
    Begin,
    End,
}

// `_Node_flags` (`<regex>`:1201-1207).
pub(super) const FL_NEGATE: u32 = 0x01;
pub(super) const FL_GREEDY: u32 = 0x02;
pub(super) const FL_FINAL: u32 = 0x04;

/// The bitmap of a bracket expression: `_Bitmap` (`<regex>`:1321-1340), a bit per byte.
#[derive(Debug, Clone, Default)]
pub(super) struct Bitmap([u8; 32]);

impl Bitmap {
    fn mark(&mut self, ch: u32) {
        self.0[(ch >> 3) as usize] |= 1 << (ch & 7);
    }

    // The matcher (the next chunk of p3-regex) reads it.
    #[allow(dead_code)]
    pub(super) fn find(&self, ch: u32) -> bool {
        self.0[(ch >> 3) as usize] & (1 << (ch & 7)) != 0
    }
}

/// `_Sequence<char>` (`<regex>`:1343-1349): collating elements of the same length `sz`.
#[derive(Debug, Clone)]
pub(super) struct Sequence {
    pub(super) sz: usize,
    pub(super) data: Vec<u8>,
}

/// `_Node_class` (`<regex>`:1425-1450). With `char`, every byte goes to the bitmap: `_Large`,
/// `_Ranges`, `_Classes` and `_Equiv` stay empty (the builder's `_Bmp_max` covers the type).
#[derive(Debug, Clone, Default)]
pub(super) struct ClassNode {
    /// `_Coll`: collating elements (`[[.x.]]`), the longest first.
    pub(super) coll: Vec<Sequence>,
    /// `_Small`.
    pub(super) small: Option<Bitmap>,
}

/// What a node holds besides its type, flags and links.
#[derive(Debug, Clone)]
pub(super) enum NodeData {
    None,
    /// `_Root_node`: `_Loops`, `_Marks`.
    Root {
        loops: u32,
        marks: u32,
    },
    /// `_Node_end_group`: `_Back`, the node that begins the group.
    EndGroup {
        back: NodeId,
    },
    /// `_Node_assert`: `_Child`, the assertion's own list.
    Assert {
        child: Option<NodeId>,
    },
    /// `_Node_capture`, `_Node_back`: `_Idx`.
    Index(u32),
    /// `_Node_str`: `_Data`.
    Str(Vec<u8>),
    /// `_Node_class`.
    Class(Box<ClassNode>),
    /// `_Node_if`: `_Endif`, `_Child` (the next alternative).
    If {
        endif: NodeId,
        child: Option<NodeId>,
    },
    /// `_Node_rep`.
    // The matcher (the next chunk of p3-regex) reads them.
    #[allow(dead_code)]
    Rep {
        min: i32,
        max: i32,
        end_rep: NodeId,
        loop_number: u32,
        /// `_Simple_loop`: -1 undetermined, 0 contains if/do, 1 simple.
        simple_loop: i32,
    },
    /// `_Node_end_rep`: `_Begin_rep`.
    EndRep {
        begin_rep: NodeId,
    },
}

/// Port of `_Node_base` (`<regex>`:1351-1362).
#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) kind: NodeType,
    pub(super) flags: u32,
    pub(super) next: Option<NodeId>,
    pub(super) prev: Option<NodeId>,
    pub(super) data: NodeData,
}

/// A compiled expression: its node graph, whose root (`_Root_node`) is node 0.
#[derive(Debug, Clone)]
pub struct Program {
    pub(super) nodes: Vec<Node>,
}

impl Program {
    /// `_Root_node::_Marks`: the number of capture groups, group 0 included.
    pub(super) fn mark_count(&self) -> usize {
        match self.nodes[0].data {
            NodeData::Root { marks, .. } => marks as usize,
            _ => unreachable!("node 0 is the root"),
        }
    }

    /// `_Root_node::_Loops`: the number of repetition nodes.
    // The matcher (the next chunk of p3-regex) reads it.
    #[allow(dead_code)]
    pub(super) fn loops(&self) -> usize {
        match self.nodes[0].data {
            NodeData::Root { loops, .. } => loops as usize,
            _ => unreachable!("node 0 is the root"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// _Builder

/// Port of `_Builder` (`<regex>`:1508-1566, 2761-3159), for `char` without `icase` or
/// `collate`: `_Bmax` is 256 and `_Tmax` 4.
struct Builder {
    nodes: Vec<Node>,
    /// `_Current`.
    current: NodeId,
}

const ROOT: NodeId = 0;

impl Builder {
    /// Port of `_Builder::_Builder` (`<regex>`:2761-2765): the root, current.
    fn new() -> Builder {
        Builder {
            nodes: vec![Node {
                kind: NodeType::Begin,
                flags: 0,
                next: None,
                prev: None,
                data: NodeData::Root { loops: 0, marks: 0 },
            }],
            current: ROOT,
        }
    }

    fn alloc(&mut self, kind: NodeType, flags: u32, data: NodeData) -> NodeId {
        self.nodes.push(Node {
            kind,
            flags,
            next: None,
            prev: None,
            data,
        });
        self.nodes.len() - 1
    }

    /// Port of `_Builder::_Negate` (`<regex>`:2772-2775).
    fn negate(&mut self) {
        self.nodes[self.current].flags ^= FL_NEGATE;
    }

    /// Port of `_Builder::_Mark_final` (`<regex>`:2777-2780).
    fn mark_final(&mut self) {
        self.nodes[self.current].flags |= FL_FINAL;
    }

    /// Port of `_Builder::_Getmark` (`<regex>`:2782-2785).
    fn getmark(&self) -> NodeId {
        self.current
    }

    /// Port of `_Builder::_Link_node` (`<regex>`:2798-2808): inserts `nx` after the current
    /// node, which it becomes.
    fn link_node(&mut self, nx: NodeId) -> NodeId {
        let current = self.current;
        self.nodes[nx].prev = Some(current);
        if let Some(next) = self.nodes[current].next {
            self.nodes[nx].next = Some(next);
            self.nodes[next].prev = Some(nx);
        }
        self.nodes[current].next = Some(nx);
        self.current = nx;
        nx
    }

    /// Port of `_Builder::_Insert_node` (`<regex>`:2810-2817): inserts `to_insert` before
    /// `insert_before`.
    fn insert_node(&mut self, insert_before: NodeId, to_insert: NodeId) {
        let prev = self.nodes[insert_before]
            .prev
            .expect("a node inserted before has a predecessor");
        self.nodes[prev].next = Some(to_insert);
        self.nodes[to_insert].prev = Some(prev);
        self.nodes[insert_before].prev = Some(to_insert);
        self.nodes[to_insert].next = Some(insert_before);
    }

    /// Port of `_Builder::_New_node` (`<regex>`:2819-2822).
    fn new_node(&mut self, kind: NodeType) -> NodeId {
        let nx = self.alloc(kind, 0, NodeData::None);
        self.link_node(nx)
    }

    /// Port of `_Builder::_Add_bol` (`<regex>`:2829-2832).
    fn add_bol(&mut self) {
        self.new_node(NodeType::Bol);
    }

    /// Port of `_Builder::_Add_eol` (`<regex>`:2834-2837).
    fn add_eol(&mut self) {
        self.new_node(NodeType::Eol);
    }

    /// Port of `_Builder::_Add_wbound` (`<regex>`:2839-2842).
    fn add_wbound(&mut self) {
        self.new_node(NodeType::Wbound);
    }

    /// Port of `_Builder::_Add_dot` (`<regex>`:2844-2847).
    fn add_dot(&mut self) {
        self.new_node(NodeType::Dot);
    }

    /// Port of `_Builder::_Add_str_node` (`<regex>`:2849-2852).
    fn add_str_node(&mut self) {
        let nx = self.alloc(NodeType::Str, 0, NodeData::Str(Vec::new()));
        self.link_node(nx);
    }

    /// Port of `_Builder::_Add_char` (`<regex>`:2854-2868), without `icase` and `collate`: a
    /// new string node unless the current one is a string not yet quantified.
    fn add_char(&mut self, ch: u8) {
        let node = &self.nodes[self.current];
        if node.kind != NodeType::Str || node.flags & FL_FINAL != 0 {
            self.add_str_node();
        }

        match &mut self.nodes[self.current].data {
            NodeData::Str(data) => data.push(ch),
            _ => unreachable!("the current node is a string"),
        }
    }

    /// Port of `_Builder::_Add_class` (`<regex>`:2870-2873).
    fn add_class(&mut self) {
        let nx = self.alloc(NodeType::Class, 0, NodeData::Class(Box::default()));
        self.link_node(nx);
    }

    fn current_class(&mut self) -> &mut ClassNode {
        match &mut self.nodes[self.current].data {
            NodeData::Class(class) => class,
            _ => unreachable!("the current node is a bracket expression"),
        }
    }

    /// Port of `_Builder::_Add_char_to_class` and `_Add_char_to_bitmap` (`<regex>`:2875-2888,
    /// 2904-2911): every `char` is below `_Bmp_max`.
    fn add_char_to_class(&mut self, ch: u8) {
        self.current_class()
            .small
            .get_or_insert_with(Bitmap::default)
            .mark(u32::from(ch));
    }

    /// Port of `_Builder::_Add_range2` (`<regex>`:2913-2941): every byte of the range goes to
    /// the bitmap, since the range's end is below `_Bmp_max`.
    fn add_range2(&mut self, arg0: u8, arg1: u8) {
        let small = self
            .current_class()
            .small
            .get_or_insert_with(Bitmap::default);
        for ch in u32::from(arg0)..=u32::from(arg1) {
            small.mark(ch);
        }
    }

    /// Port of `_Builder::_Add_named_class` and `_Add_elts` (`<regex>`:2943-2967): the bytes of
    /// the class (or of its complement) into the bitmap. `_Classes` is never set for `char`
    /// (`_Bmp_max <= _Max_limit<unsigned char>()` is false).
    fn add_named_class(&mut self, class: Mask, negate: bool) {
        let small = self
            .current_class()
            .small
            .get_or_insert_with(Bitmap::default);
        for ch in 0..=255u8 {
            if isctype(ch, class) != negate {
                small.mark(u32::from(ch));
            }
        }
    }

    /// Port of `_Builder::_Char_to_elts` (`<regex>`:2969-2983): adds a collating element to
    /// the list of sequences, kept longest first.
    fn char_to_elts(seqs: &mut Vec<Sequence>, elt: &[u8]) {
        let diff = elt.len();
        let mut cur = 0;
        while cur < seqs.len() && diff < seqs[cur].sz {
            cur += 1;
        }

        if cur == seqs.len() || diff != seqs[cur].sz {
            seqs.insert(
                cur,
                Sequence {
                    sz: diff,
                    data: Vec::new(),
                },
            );
        }
        seqs[cur].data.extend_from_slice(elt);
    }

    /// Port of `_Builder::_Add_equiv` (`<regex>`:2995-3015): the bytes whose primary key is
    /// `elt`'s. With `char`, nothing goes to `_Equiv` (`_Bmp_max < _Max_limit<char>()` is
    /// false).
    fn add_equiv(&mut self, elt: &[u8]) {
        let key = transform_primary(elt);
        let small = self
            .current_class()
            .small
            .get_or_insert_with(Bitmap::default);
        for ch in 0..=255u8 {
            if transform_primary(&[ch]) == key {
                small.mark(u32::from(ch));
            }
        }
    }

    /// Port of `_Builder::_Add_coll` (`<regex>`:3017-3023).
    fn add_coll(&mut self, elt: &[u8]) {
        Builder::char_to_elts(&mut self.current_class().coll, elt);
    }

    /// Port of `_Builder::_Begin_group` (`<regex>`:3025-3028).
    fn begin_group(&mut self) -> NodeId {
        self.new_node(NodeType::Group)
    }

    /// Port of `_Builder::_End_group` (`<regex>`:3030-3042).
    fn end_group(&mut self, back: NodeId) {
        let elt = match self.nodes[back].kind {
            NodeType::Group => NodeType::EndGroup,
            NodeType::Assert | NodeType::NegAssert => NodeType::EndAssert,
            _ => NodeType::EndCapture,
        };

        let nx = self.alloc(elt, 0, NodeData::EndGroup { back });
        self.link_node(nx);
    }

    /// Port of `_Builder::_Begin_assert_group` (`<regex>`:3044-3054): the assertion node, and
    /// a no-op that starts its own list, which becomes current.
    fn begin_assert_group(&mut self, neg: bool) -> NodeId {
        let kind = if neg {
            NodeType::NegAssert
        } else {
            NodeType::Assert
        };
        let node1 = self.alloc(kind, 0, NodeData::Assert { child: None });
        let node2 = self.alloc(NodeType::Nop, 0, NodeData::None);
        self.link_node(node1);
        self.nodes[node1].data = NodeData::Assert { child: Some(node2) };
        self.nodes[node2].prev = Some(node1);
        self.current = node2;
        node1
    }

    /// Port of `_Builder::_End_assert_group` (`<regex>`:3056-3060).
    fn end_assert_group(&mut self, nx: NodeId) {
        self.end_group(nx);
        self.current = nx;
    }

    /// Port of `_Builder::_Begin_capture_group` (`<regex>`:3062-3065).
    fn begin_capture_group(&mut self, idx: u32) -> NodeId {
        let nx = self.alloc(NodeType::Capture, 0, NodeData::Index(idx));
        self.link_node(nx)
    }

    /// Port of `_Builder::_Add_backreference` (`<regex>`:3067-3070).
    fn add_backreference(&mut self, idx: u32) {
        let nx = self.alloc(NodeType::Back, 0, NodeData::Index(idx));
        self.link_node(nx);
    }

    /// Port of `_Builder::_Begin_if` (`<regex>`:3072-3083): an endif after the current node,
    /// and an if node before the first node after `start`.
    fn begin_if(&mut self, start: NodeId) -> NodeId {
        // append endif node
        let res = self.alloc(NodeType::Endif, 0, NodeData::None);
        self.link_node(res);

        // insert if_node
        let node1 = self.alloc(
            NodeType::If,
            0,
            NodeData::If {
                endif: res,
                child: None,
            },
        );
        let pos = self.nodes[start].next.expect("an alternative follows");
        self.insert_node(pos, node1);
        res
    }

    /// Port of `_Builder::_Else_if` (`<regex>`:3085-3101): moves the alternative parsed after
    /// the endif `end` into a new if node, the last child of `start`'s.
    fn else_if(&mut self, start: NodeId, end: NodeId) {
        let mut parent = self.nodes[start].next.expect("the if node");
        let first = self.nodes[end].next.expect("the alternative's first node");
        self.nodes[end].next = None;
        let last = self.current;
        self.current = end;
        self.nodes[end].next = None;
        self.nodes[last].next = Some(end);
        while let NodeData::If {
            child: Some(child), ..
        } = self.nodes[parent].data
        {
            parent = child;
        }

        let child = self.alloc(
            NodeType::If,
            0,
            NodeData::If {
                endif: end,
                child: None,
            },
        );
        match &mut self.nodes[parent].data {
            NodeData::If { child: c, .. } => *c = Some(child),
            _ => unreachable!("an if node"),
        }
        self.nodes[child].next = Some(first);
        self.nodes[first].prev = Some(child);
    }

    /// Port of `_Builder::_Add_rep` (`<regex>`:3103-3148): the last character of a string
    /// moves to a string of its own; `{0,1}` becomes an alternation with an empty group, and
    /// other repetitions a pair of rep nodes around the repeated element.
    fn add_rep(&mut self, min: i32, max: i32, greedy: bool) {
        if let NodeData::Str(data) = &self.nodes[self.current].data
            && data.len() != 1
        {
            // move final character to new string node
            let ch = match &mut self.nodes[self.current].data {
                NodeData::Str(data) => data.pop().expect("a non-empty string"),
                _ => unreachable!("a string"),
            };
            self.add_char(ch);
        }

        let mut pos = self.current;
        if matches!(
            self.nodes[pos].kind,
            NodeType::EndGroup | NodeType::EndCapture
        ) {
            pos = match self.nodes[pos].data {
                NodeData::EndGroup { back } => back,
                _ => unreachable!("an end of group"),
            };
        }

        if min == 0 && max == 1 {
            // rewrite zero-or-one quantifiers as alternations to make the
            // "simple loop" optimization more likely to engage
            let end = self.alloc(NodeType::Endif, 0, NodeData::None);
            let if_expr = self.alloc(
                NodeType::If,
                0,
                NodeData::If {
                    endif: end,
                    child: None,
                },
            );
            let if_empty_str = self.alloc(
                NodeType::If,
                0,
                NodeData::If {
                    endif: end,
                    child: None,
                },
            );
            let gbegin = self.alloc(NodeType::Group, 0, NodeData::None);
            let gend = self.alloc(NodeType::EndGroup, 0, NodeData::EndGroup { back: gbegin });

            self.nodes[if_empty_str].next = Some(gbegin);
            self.nodes[gbegin].prev = Some(if_empty_str);

            self.nodes[gbegin].next = Some(gend);
            self.nodes[gend].prev = Some(gbegin);

            self.nodes[gend].next = Some(end);

            self.nodes[if_expr].data = NodeData::If {
                endif: end,
                child: Some(if_empty_str),
            };

            self.link_node(end);
            self.insert_node(pos, if_expr);

            if !greedy {
                let expr_next = self.nodes[if_expr].next.expect("the repeated element");
                let empty_next = self.nodes[if_empty_str].next.expect("the empty group");
                let expr_next_prev = self.nodes[expr_next].prev;
                let empty_next_prev = self.nodes[empty_next].prev;
                self.nodes[expr_next].prev = empty_next_prev;
                self.nodes[empty_next].prev = expr_next_prev;
                self.nodes[if_expr].next = Some(empty_next);
                self.nodes[if_empty_str].next = Some(expr_next);
            }
        } else {
            let node0 = self.alloc(NodeType::EndRep, 0, NodeData::EndRep { begin_rep: ROOT });
            let loop_number = match &mut self.nodes[ROOT].data {
                NodeData::Root { loops, .. } => {
                    let n = *loops;
                    *loops += 1;
                    n
                }
                _ => unreachable!("node 0 is the root"),
            };
            let nx = self.alloc(
                NodeType::Rep,
                if greedy { FL_GREEDY } else { 0 },
                NodeData::Rep {
                    min,
                    max,
                    end_rep: node0,
                    loop_number,
                    simple_loop: -1,
                },
            );
            self.nodes[node0].data = NodeData::EndRep { begin_rep: nx };
            self.link_node(node0);
            self.insert_node(pos, nx);
        }
    }

    /// Port of `_Builder::_End_pattern` (`<regex>`:3150-3154).
    fn end_pattern(&mut self) {
        self.new_node(NodeType::End);
    }
}

// ---------------------------------------------------------------------------------------------
// _Parser

/// `_Meta_type` (`<regex>`:58-100): what the parser sees in a character.
const META_EOS: i32 = -1;
const META_CHR: i32 = 0;

/// The meta characters `_Trans` recognizes (`<regex>`:3865-3867).
const META_MAP: &[u8] = b"()$^.*+?[]|\\-{},:=!\n\r\x08";

/// `_Prs_ret` (`<regex>`:1705-1709): what a class atom is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrsRet {
    None,
    Chr,
    Set,
}

/// `_Do_capture_group`'s limit on the number of capture groups (`<regex>`:4196).
const MAX_GROUPS: u32 = 1000;

/// The deepest nesting of groups the port compiles: the parser recurses once per group, and
/// the Windows wheel's stack overflows (the process ends) at about 7,800 nested lookaheads
/// or 8,400 nested non-capture groups, a depth that varies with the caller's stack. Deeper
/// groups are refused with `error_stack`, the error MSVC gives its own limit on capture
/// groups (docs/improvements.md, U-53).
const MAX_NESTING: i32 = 5000;

/// The stack a compilation needs per level of nesting, with a margin: about 1.5 KiB in the
/// test profile.
const STACK_PER_LEVEL: usize = 4096;

/// Up to this many `(` the compilation runs on the caller's thread.
const INLINE_GROUPS: usize = 64;

/// Port of `_Parser` (`<regex>`:1711-1764, 3850-4677) for the ECMAScript grammar, which sets
/// the `_L_flags` `_L_ext_rep`, `_L_alt_pipe`, `_L_nex_grp`, `_L_nex_rep`, `_L_nc_grp`,
/// `_L_asrt_gen`, `_L_asrt_wrd`, `_L_bckr`, `_L_ngr_rep`, `_L_esc_uni`, `_L_esc_hex`,
/// `_L_esc_bsl`, `_L_esc_ffn`, `_L_esc_wsd`, `_L_esc_ctrl`, `_L_bzr_chr`, `_L_grp_esc`,
/// `_L_ident_ECMA` and `_L_empty_grp` (`<regex>`:4636-4639).
struct Parser<'a> {
    /// `[_Begin, _End)`.
    pat: &'a [u8],
    /// `_Pat`.
    pos: usize,
    /// `_Grp_idx`.
    grp_idx: u32,
    /// `_Disj_count`.
    disj_count: i32,
    /// `_Finished_grps`.
    finished_grps: Vec<bool>,
    /// `_Nfa`.
    nfa: Builder,
    /// `_Val`.
    val: i32,
    /// `_Char`: the current character, as a (signed) `char`.
    ch: i8,
    /// `_Mchar`.
    mchar: i32,
}

type ParseResult<T> = Result<T, RegexError>;

impl<'a> Parser<'a> {
    /// Port of `_Parser::_Parser` (`<regex>`:4630-4667) for ECMAScript.
    fn new(pat: &'a [u8]) -> Parser<'a> {
        let mut parser = Parser {
            pat,
            pos: 0,
            grp_idx: 0,
            disj_count: 0,
            finished_grps: Vec::new(),
            nfa: Builder::new(),
            val: 0,
            ch: 0,
            mchar: META_CHR,
        };
        parser.trans();
        parser
    }

    /// Port of `_Parser::_Mark_count` (`<regex>`:1719-1721).
    fn mark_count(&self) -> u32 {
        self.grp_idx + 1
    }

    /// Port of `_Parser::_Trans` (`<regex>`:3863-3949) for ECMAScript, where no character
    /// changes meaning with its context (`_Is_esc` is false, and the special cases need the
    /// other grammars' flags).
    fn trans(&mut self) {
        if self.pos == self.pat.len() {
            self.mchar = META_EOS;
            self.ch = META_EOS as i8;
        } else {
            // map current character
            let c = self.pat[self.pos];
            self.ch = c as i8;
            self.mchar = if META_MAP.contains(&c) {
                i32::from(c)
            } else {
                META_CHR
            };
        }
    }

    /// Port of `_Parser::_Next` (`<regex>`:3951-3961).
    fn next(&mut self) {
        if self.pos != self.pat.len() {
            self.pos += 1;
        }
        self.trans();
    }

    /// Port of `_Parser::_Expect` (`<regex>`:3963-3971).
    fn expect(&mut self, st: u8, code: ErrorType) -> ParseResult<()> {
        if self.mchar != i32::from(st) {
            return Err(error(code));
        }

        self.next();
        Ok(())
    }

    /// Port of `_Parser::_Do_digits` (`<regex>`:3973-3988): the value of up to `count`
    /// digits of `base` in `val`, and the count left.
    fn do_digits(&mut self, base: i32, mut count: i32, error_type: ErrorType) -> ParseResult<i32> {
        self.val = 0;
        loop {
            if count == 0 {
                break;
            }
            let chv = value(self.ch, base);
            if chv == -1 {
                break;
            }
            // append next digit
            if self.val > (i32::MAX - chv) / base {
                return Err(error(error_type));
            }
            count -= 1;
            self.val *= base;
            self.val += chv;
            self.next();
        }
        Ok(count)
    }

    /// Port of `_Parser::_DecimalDigits2` (`<regex>`:3990-3994).
    fn decimal_digits2(&mut self, error_type: ErrorType, count: i32) -> ParseResult<bool> {
        Ok(self.do_digits(10, count, error_type)? != count)
    }

    /// Port of `_Parser::_HexDigits` (`<regex>`:3996-4001).
    fn hex_digits(&mut self, count: i32) -> ParseResult<()> {
        if self.do_digits(16, count, ErrorType::Escape)? != 0 {
            return Err(error(ErrorType::Escape));
        }
        Ok(())
    }

    /// Port of `_Parser::_Do_ex_class` (`<regex>`:4008-4048): `[:name:]`, `[=x=]` or `[.x.]`
    /// inside a bracket expression, after its delimiter.
    fn do_ex_class(&mut self, end_arg: u8) -> ParseResult<()> {
        let errtype = match end_arg {
            b':' => ErrorType::Ctype,
            b'=' | b'.' => ErrorType::Collate,
            _ => ErrorType::Syntax,
        };
        let beg = self.pos;

        while self.mchar != i32::from(b':')
            && self.mchar != i32::from(b'=')
            && self.mchar != i32::from(b'.')
            && self.mchar != META_EOS
        {
            // advance to end delimiter
            self.next();
        }
        if self.mchar != i32::from(end_arg) {
            return Err(error(errtype));
        } else if end_arg == b':' {
            // handle named character class
            let cls = lookup_classname(&self.pat[beg..self.pos]);
            if cls == 0 {
                return Err(error(ErrorType::Ctype));
            }

            self.nfa.add_named_class(cls, false);
        } else if end_arg == b'=' {
            // process =
            if beg == self.pos {
                return Err(error(ErrorType::Collate));
            } else {
                let elt = self.pat[beg..self.pos].to_vec();
                self.nfa.add_equiv(&elt);
            }
        } else if end_arg == b'.' {
            // process .
            if beg == self.pos {
                return Err(error(ErrorType::Collate));
            } else {
                let elt = self.pat[beg..self.pos].to_vec();
                self.nfa.add_coll(&elt);
            }
        }
        self.next();
        self.expect(b']', errtype)
    }

    /// Port of `_Parser::_CharacterClassEscape` (`<regex>`:4050-4072): `\d`, `\s`, `\w` and
    /// their negations; with `addit`, as a bracket expression of their own.
    fn character_class_escape(&mut self, addit: bool) -> ParseResult<bool> {
        if self.pos == self.pat.len() {
            return Ok(false);
        }
        let cls = lookup_classname(&self.pat[self.pos..self.pos + 1]);
        if cls == 0 {
            return Ok(false);
        }

        let negated = isctype(self.ch as u8, CT_UPPER);
        if addit {
            self.nfa.add_class();
            // GH-992: Outside character class definitions, _Cls completely defines the
            // character class so negating _Cls and negating the entire character class are
            // equivalent. Since the former negation is defective, do the latter instead.
            if negated {
                self.nfa.negate();
            }
        }

        self.nfa.add_named_class(cls, negated && !addit);
        self.next();
        Ok(true)
    }

    /// Port of `_Parser::_ClassEscape2` (`<regex>`:4074-4090) (`_L_esc_bsl`, `_L_esc_wsd`).
    fn class_escape2(&mut self) -> ParseResult<PrsRet> {
        if self.ch == b'\\' as i8 {
            // handle escape backslash if allowed
            self.val = i32::from(b'\\');
            self.next();
            Ok(PrsRet::Chr)
        } else if self.character_class_escape(false)? {
            Ok(PrsRet::Set)
        } else if self.decimal_digits2(ErrorType::Escape, i32::MAX)? {
            // check for invalid value
            if self.val != 0 {
                return Err(error(ErrorType::Escape));
            }

            Ok(PrsRet::Chr)
        } else if self.character_escape()? {
            Ok(PrsRet::Chr)
        } else {
            Ok(PrsRet::None)
        }
    }

    /// Port of `_Parser::_ClassAtom` (`<regex>`:4092-4123) (`_L_grp_esc`).
    fn class_atom(&mut self) -> ParseResult<PrsRet> {
        if self.mchar == i32::from(b'\\') {
            // check for valid escape sequence
            self.next();
            self.class_escape2()
        } else if self.mchar == i32::from(b'[') {
            // check for valid delimited expression
            self.next();
            if self.mchar == i32::from(b':')
                || self.mchar == i32::from(b'=')
                || self.mchar == i32::from(b'.')
            {
                // handle delimited expression
                let st = self.mchar as u8;
                self.next();
                self.do_ex_class(st)?;
                Ok(PrsRet::Set)
            } else {
                // handle ordinary [
                self.val = i32::from(b'[');
                Ok(PrsRet::Chr)
            }
        } else if self.mchar == i32::from(b']') || self.mchar == META_EOS {
            Ok(PrsRet::None)
        } else {
            // handle ordinary character
            self.val = i32::from(self.ch);
            self.next();
            Ok(PrsRet::Chr)
        }
    }

    /// Port of `_Parser::_ClassRanges` (`<regex>`:4125-4174) (`_L_bzr_chr`, no `icase` or
    /// `collate`).
    fn class_ranges(&mut self) -> ParseResult<()> {
        loop {
            // process characters through end of bracket expression
            let mut ret = self.class_atom()?;
            if ret == PrsRet::None {
                return Ok(());
            }

            if self.mchar == i32::from(b'-') {
                // check for valid range
                self.next();
                let chr1 = self.val as i8;
                let set_preceding = ret == PrsRet::Set;
                ret = self.class_atom()?;
                if ret == PrsRet::None {
                    // treat - as ordinary character
                    if !set_preceding {
                        self.nfa.add_char_to_class(chr1 as u8);
                    }
                    self.nfa.add_char_to_class(b'-');
                    return Ok(());
                }

                if set_preceding || ret == PrsRet::Set {
                    // set precedes or follows dash
                    return Err(error(ErrorType::Range));
                }

                let chr2 = self.val as i8;

                if (chr2 as u8) < (chr1 as u8) {
                    return Err(error(ErrorType::Range));
                }

                self.nfa.add_range2(chr1 as u8, chr2 as u8);
            } else if ret == PrsRet::Chr {
                self.nfa.add_char_to_class(self.val as i8 as u8);
            }
        }
    }

    /// Port of `_Parser::_CharacterClass` (`<regex>`:4176-4189) (no `_L_brk_rstr`).
    fn character_class(&mut self) -> ParseResult<()> {
        self.nfa.add_class();
        if self.mchar == i32::from(b'^') {
            // negate bracket expression
            self.nfa.negate();
            self.next();
        }

        self.class_ranges()
    }

    /// Port of `_Parser::_Do_capture_group` (`<regex>`:4191-4204).
    fn do_capture_group(&mut self) -> ParseResult<()> {
        self.grp_idx += 1;

        if self.grp_idx >= MAX_GROUPS {
            // hardcoded limit
            return Err(error(ErrorType::Stack));
        }

        let pos1 = self.nfa.begin_capture_group(self.grp_idx);
        self.disjunction()?;
        self.nfa.end_group(pos1);
        self.finished_grps.resize(self.grp_idx as usize + 1, false);
        let idx = match self.nfa.nodes[pos1].data {
            NodeData::Index(idx) => idx,
            _ => unreachable!("a capture node"),
        };
        self.finished_grps[idx as usize] = true;
        Ok(())
    }

    /// Port of `_Parser::_Do_noncapture_group` (`<regex>`:4206-4211).
    fn do_noncapture_group(&mut self) -> ParseResult<()> {
        let pos1 = self.nfa.begin_group();
        self.disjunction()?;
        self.nfa.end_group(pos1);
        Ok(())
    }

    /// Port of `_Parser::_Do_assert_group` (`<regex>`:4213-4218).
    fn do_assert_group(&mut self, neg: bool) -> ParseResult<()> {
        let pos1 = self.nfa.begin_assert_group(neg);
        self.disjunction()?;
        self.nfa.end_assert_group(pos1);
        Ok(())
    }

    /// Port of `_Parser::_Wrapped_disjunction` (`<regex>`:4220-4250) (`_L_empty_grp`,
    /// `_L_nc_grp`, no `nosubs`): a group after its `(`; whether it may be quantified.
    fn wrapped_disjunction(&mut self) -> ParseResult<bool> {
        self.disj_count += 1;
        if self.disj_count > MAX_NESTING {
            // The wheel's recursion overflows the stack around here (U-53).
            return Err(error(ErrorType::Stack));
        }
        if self.mchar == i32::from(b'?') {
            // check for valid ECMAScript (?x ... ) group
            self.next();
            let ch = self.mchar;
            self.next();
            if ch == i32::from(b':') {
                self.do_noncapture_group()?;
            } else if ch == i32::from(b'!') {
                // process assert group, negating
                self.do_assert_group(true)?;
                self.disj_count -= 1;
                return Ok(false);
            } else if ch == i32::from(b'=') {
                // process assert group
                self.do_assert_group(false)?;
                self.disj_count -= 1;
                return Ok(false);
            } else {
                return Err(error(ErrorType::Syntax));
            }
        } else {
            self.do_capture_group()?;
        }

        self.disj_count -= 1;
        Ok(true)
    }

    /// Port of `_Parser::_IsIdentityEscape` (`<regex>`:4252-4295) (`_L_ident_ECMA`).
    fn is_identity_escape(&self) -> bool {
        // ECMAScript identity escape characters
        !matches!(
            self.ch as u8,
            b'c' | b'd' | b'D' | b's' | b'S' | b'w' | b'W'
        )
    }

    /// Port of `_Parser::_IdentityEscape` (`<regex>`:4297-4306).
    fn identity_escape(&mut self) -> bool {
        if self.is_identity_escape() {
            self.val = i32::from(self.ch);
            self.next();
            true
        } else {
            false
        }
    }

    /// Port of `_Parser::_Do_ffn` (`<regex>`:4308-4325).
    fn do_ffn(&mut self, ch: i8) -> bool {
        self.val = match ch as u8 {
            b'f' => 0x0C,
            b'n' => i32::from(b'\n'),
            b'r' => i32::from(b'\r'),
            b't' => i32::from(b'\t'),
            b'v' => 0x0B,
            _ => return false,
        };

        true
    }

    /// Port of `_Parser::_CharacterEscape` (`<regex>`:4340-4374) (`_L_esc_ffn`,
    /// `_L_esc_ctrl`, `_L_esc_hex`, `_L_esc_uni`; no `_L_esc_ffnx` or `_L_esc_oct`).
    fn character_escape(&mut self) -> ParseResult<bool> {
        if self.mchar == META_EOS {
            return Err(error(ErrorType::Escape));
        }

        if self.do_ffn(self.ch) {
            self.next();
        } else if self.ch == b'c' as i8 {
            // handle control escape sequence
            self.next();
            if !isctype(self.ch as u8, CT_ALPHA) {
                return Err(error(ErrorType::Escape));
            }

            self.val = i32::from((i32::from(self.ch) % 32) as i8);
            self.next();
        } else if self.ch == b'x' as i8 {
            // handle hexadecimal escape sequence
            self.next();
            self.hex_digits(2)?;
        } else if self.ch == b'u' as i8 {
            // handle Unicode escape sequence
            self.next();
            self.hex_digits(4)?;
        } else {
            return Ok(self.identity_escape());
        }

        if 255 < self.val as u32 {
            return Err(error(ErrorType::Escape));
        }

        self.val = i32::from(self.val as i8);
        Ok(true)
    }

    /// Port of `_Parser::_AtomEscape` (`<regex>`:4376-4398) (`_L_bckr`, `_L_bzr_chr`,
    /// `_L_esc_wsd`; no `_L_lim_bckr`): an escape outside a bracket expression, after its `\`.
    fn atom_escape(&mut self) -> ParseResult<()> {
        if self.decimal_digits2(ErrorType::Backref, i32::MAX)? {
            // check for valid back reference
            if self.val == 0 {
                // handle \0
                self.nfa.add_char(0);
            } else if self.grp_idx < self.val as u32
                || self.finished_grps.len() <= self.val as usize
                || !self.finished_grps[self.val as usize]
            {
                return Err(error(ErrorType::Backref));
            } else {
                self.nfa.add_backreference(self.val as u32);
            }
        } else if self.character_escape()? {
            self.nfa.add_char(self.val as i8 as u8);
        } else if !self.character_class_escape(true)? {
            return Err(error(ErrorType::Escape));
        }
        Ok(())
    }

    /// Port of `_Parser::_Quantifier` (`<regex>`:4400-4441) (`_L_ngr_rep`).
    fn quantifier(&mut self) -> ParseResult<()> {
        let mut min = 0;
        let mut max = -1;
        if self.mchar != i32::from(b'*') {
            if self.mchar == i32::from(b'+') {
                min = 1;
            } else if self.mchar == i32::from(b'?') {
                max = 1;
            } else if self.mchar == i32::from(b'{') {
                // check for valid bracketed value
                self.next();
                if !self.decimal_digits2(ErrorType::Badbrace, i32::MAX)? {
                    return Err(error(ErrorType::Badbrace));
                }

                min = self.val;
                if self.mchar != i32::from(b',') {
                    max = min;
                } else {
                    // check for decimal constant following comma
                    self.next();
                    if self.mchar != i32::from(b'}') {
                        if !self.decimal_digits2(ErrorType::Badbrace, i32::MAX)? {
                            return Err(error(ErrorType::Badbrace));
                        }

                        max = self.val;
                    }
                }

                if self.mchar != i32::from(b'}') || (max != -1 && max < min) {
                    return Err(error(ErrorType::Badbrace));
                }
            } else {
                return Ok(());
            }
        }

        self.nfa.mark_final();
        self.next();
        let greedy = self.mchar != i32::from(b'?');
        if !greedy {
            // add non-greedy repeat node
            self.next();
        }

        self.nfa.add_rep(min, max, greedy);
        Ok(())
    }

    /// Port of `_Parser::_Alternative` (`<regex>`:4443-4505) (`_L_asrt_wrd`; no
    /// `_L_paren_bal`): the terms of one alternative; whether there were any.
    fn alternative(&mut self) -> ParseResult<bool> {
        let mut found = false;
        loop {
            // concatenate valid elements
            let mut quant = true;
            if self.mchar == META_EOS
                || self.mchar == i32::from(b'|')
                || (self.mchar == i32::from(b')') && self.disj_count != 0)
            {
                return Ok(found);
            } else if self.mchar == i32::from(b')') {
                return Err(error(ErrorType::Paren));
            } else if self.mchar == i32::from(b'.') {
                // add dot node
                self.nfa.add_dot();
                self.next();
            } else if self.mchar == i32::from(b'\\') {
                // check for valid escape sequence
                self.next();
                if self.ch == b'b' as i8 {
                    // add word assert
                    self.nfa.add_wbound();
                    self.next();
                    quant = false;
                } else if self.ch == b'B' as i8 {
                    // add not-word assert
                    self.nfa.add_wbound();
                    self.nfa.negate();
                    self.next();
                    quant = false;
                } else {
                    self.atom_escape()?;
                }
            } else if self.mchar == i32::from(b'[') {
                // add bracket expression
                self.next();
                self.character_class()?;
                self.expect(b']', ErrorType::Brack)?;
            } else if self.mchar == i32::from(b'(') {
                // check for valid group
                self.next();
                quant = self.wrapped_disjunction()?;
                self.expect(b')', ErrorType::Paren)?;
            } else if self.mchar == i32::from(b'^') {
                // add bol node
                self.nfa.add_bol();
                self.next();
                quant = false;
            } else if self.mchar == i32::from(b'$') {
                // add eol node
                self.nfa.add_eol();
                self.next();
                quant = false;
            } else if self.mchar == i32::from(b'*')
                || self.mchar == i32::from(b'+')
                || self.mchar == i32::from(b'?')
                || self.mchar == i32::from(b'{')
            {
                return Err(error(ErrorType::Badrepeat));
            } else if self.mchar == i32::from(b'}') {
                return Err(error(ErrorType::Brace));
            } else if self.mchar == i32::from(b']') {
                return Err(error(ErrorType::Brack));
            } else {
                // add character
                self.nfa.add_char(self.ch as u8);
                self.next();
            }

            if quant {
                self.quantifier()?;
            }

            found = true;
        }
    }

    /// Port of `_Parser::_Disjunction` (`<regex>`:4507-4529).
    fn disjunction(&mut self) -> ParseResult<()> {
        let pos1 = self.nfa.getmark();
        if !self.alternative()? {
            if self.mchar != i32::from(b'|') {
                return Ok(()); // zero-length alternative not followed by '|'
            }

            // zero-length leading alternative
            let pos3 = self.nfa.begin_group();
            self.nfa.end_group(pos3);
        }

        let pos2 = self.nfa.begin_if(pos1);
        while self.mchar == i32::from(b'|') {
            // append terms as long as we keep finding | characters
            self.next();
            if !self.alternative()? {
                // zero-length trailing alternative
                let pos3 = self.nfa.begin_group();
                self.nfa.end_group(pos3);
            }

            self.nfa.else_if(pos1, pos2);
        }
        Ok(())
    }

    /// Port of `_Parser::_Compile` (`<regex>`:4611-4628).
    fn compile(mut self) -> ParseResult<Program> {
        let pos1 = self.nfa.begin_capture_group(0);
        self.disjunction()?;
        if self.pos != self.pat.len() {
            return Err(error(ErrorType::Syntax));
        }

        self.nfa.end_group(pos1);
        self.nfa.end_pattern();
        let marks = self.mark_count();
        match &mut self.nfa.nodes[ROOT].data {
            NodeData::Root { marks: m, .. } => *m = marks,
            _ => unreachable!("node 0 is the root"),
        }
        let mut program = Program {
            nodes: self.nfa.nodes,
        };
        calculate_loop_simplicity(&mut program.nodes, Some(ROOT), None, None);
        Ok(program)
    }
}

/// Port of `_Calculate_loop_simplicity` (`<regex>`:4531-4609): marks each repetition simple
/// (no alternation or repetition inside) or not, walking the list from `nx` to `ne`.
fn calculate_loop_simplicity(
    nodes: &mut [Node],
    mut nx: Option<NodeId>,
    ne: Option<NodeId>,
    mut outer_rep: Option<NodeId>,
) {
    while let Some(n) = nx {
        if Some(n) == ne {
            break;
        }
        match nodes[n].kind {
            NodeType::If => {
                // _Node_if inside a _Node_rep makes the rep not simple
                if let Some(rep) = outer_rep {
                    set_simple_loop(nodes, rep, 0);
                }

                // visit each branch of the if; the first is the rest of this list
                let mut branch = match nodes[n].data {
                    NodeData::If { child, .. } => child,
                    _ => unreachable!("an if node"),
                };
                while let Some(b) = branch {
                    let (endif, child) = match nodes[b].data {
                        NodeData::If { endif, child } => (endif, child),
                        _ => unreachable!("an if node"),
                    };
                    calculate_loop_simplicity(nodes, nodes[b].next, Some(endif), outer_rep);
                    branch = child;
                }
            }
            NodeType::Assert | NodeType::NegAssert => {
                // visit the assertion body
                // note _Outer_rep being reset: the assertion regex is completely independent
                let child = match nodes[n].data {
                    NodeData::Assert { child } => child,
                    _ => unreachable!("an assertion"),
                };
                calculate_loop_simplicity(nodes, child, None, None);
            }
            NodeType::Rep => {
                // _Node_rep inside another _Node_rep makes both not simple
                if let Some(rep) = outer_rep {
                    set_simple_loop(nodes, rep, 0);
                    set_simple_loop(nodes, n, 0);
                } else {
                    outer_rep = Some(n);
                }
            }
            NodeType::EndRep => {
                let begin_rep = match nodes[n].data {
                    NodeData::EndRep { begin_rep } => begin_rep,
                    _ => unreachable!("an end of repetition"),
                };
                if outer_rep == Some(begin_rep) {
                    // if the _Node_rep is still undetermined when we reach its end, it is simple
                    if let NodeData::Rep { simple_loop, .. } = &mut nodes[begin_rep].data
                        && *simple_loop == -1
                    {
                        *simple_loop = 1;
                    }
                    outer_rep = None;
                }
            }
            _ => {}
        }
        nx = nodes[n].next;
    }
}

fn set_simple_loop(nodes: &mut [Node], rep: NodeId, value: i32) {
    match &mut nodes[rep].data {
        NodeData::Rep { simple_loop, .. } => *simple_loop = value,
        _ => unreachable!("a repetition"),
    }
}

/// `basic_regex<char>(first, last)` with the ECMAScript grammar: the compiled expression, or
/// the `regex_error` the parser throws.
///
/// Port of `basic_regex::_Reset` (`<regex>`:2039-2056) and `_Parser::_Compile`.
pub(super) fn compile(pattern: &[u8]) -> Result<Program, RegexError> {
    let groups = pattern.iter().filter(|&&c| c == b'(').count();
    if groups <= INLINE_GROUPS {
        return Parser::new(pattern).compile();
    }

    // Deep nesting: on a thread with the stack it needs (at most MAX_NESTING levels).
    let levels = groups.min(MAX_NESTING as usize + 1);
    let stack = 256 * 1024 + levels * STACK_PER_LEVEL;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn_scoped(scope, || Parser::new(pattern).compile())
            .expect("a thread to compile the expression")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}
