// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Translated from the Microsoft C++ Standard Library, `<regex>` as shipped with MSVC
// 14.44.35207 (https://github.com/microsoft/STL):
//     Copyright (c) Microsoft Corporation.
//     SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception

//! Matching a compiled expression as the Windows wheel does: a translation of Microsoft's STL
//! `_Matcher` (MSVC 14.44.35207 `<regex>`:1569-1704, 3156-3848), the backtracking matcher,
//! with its limits: `_REGEX_MAX_STACK_COUNT` (600 on x64) nested matches and
//! `_REGEX_MAX_COMPLEXITY_COUNT` (10,000,000) steps, past which it throws `error_stack` or
//! `error_complexity`. And `regex_match` over it (`<regex>`:2165-2203). `regex_search` and
//! `regex_replace` come with their callers (p3-rules, built-in configs).
//!
//! The text is a byte slice; positions are indices into it (the STL's iterators). The
//! branches for the other grammars and for `icase` and `collate` are left out, as in
//! [`super::msvc`].

use super::msvc::{self, FL_GREEDY, FL_NEGATE, NodeData, NodeId, NodeType, Program};
use super::{ErrorType, RegexError};

/// `regex_constants::match_flag_type` (`<regex>`:107-124): the flags the functions use.
pub(super) const MATCH_DEFAULT: u32 = 0x0000;
const MATCH_NOT_BOL: u32 = 0x0001;
const MATCH_NOT_EOL: u32 = 0x0002;
const MATCH_NOT_BOW: u32 = 0x0004;
const MATCH_NOT_EOW: u32 = 0x0008;
const MATCH_NOT_NULL: u32 = 0x0020;
const MATCH_PREV_AVAIL: u32 = 0x0100;
const MATCH_NOT_NULL_INTERNAL: u32 = 0x2000;

/// `_REGEX_MAX_COMPLEXITY_COUNT` (`<regex>`:36-38).
const MAX_COMPLEXITY_COUNT: i64 = 10_000_000;
/// `_REGEX_MAX_STACK_COUNT` on x64 (`<regex>`:40-46).
const MAX_STACK_COUNT: i64 = 600;

/// `_Meta_nl`, `_Meta_cr` (`<regex>`:77-78). `_Meta_ls` and `_Meta_ps` (U+2028, U+2029) never
/// equal a `char`.
const META_NL: u8 = b'\n';
const META_CR: u8 = b'\r';

/// `_Is_word` (`<regex>`:533-552): ASCII letters, digits and `_`.
fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `_Bt_state_t` (`<regex>`:1570-1575): the state needed for backtracking.
#[derive(Debug, Clone)]
struct BtState {
    cur: usize,
    grp_valid: Vec<bool>,
}

/// `_Tgt_state_t` (`<regex>`:1577-1590): the current state of the match.
#[derive(Debug, Clone)]
struct TgtState {
    cur: usize,
    grp_valid: Vec<bool>,
    /// `_Grps`: (begin, end) of each group.
    grps: Vec<(usize, usize)>,
}

impl TgtState {
    fn bt(&self) -> BtState {
        BtState {
            cur: self.cur,
            grp_valid: self.grp_valid.clone(),
        }
    }

    /// `_Tgt_state_t::operator=(const _Bt_state_t &)`.
    fn restore(&mut self, st: &BtState) {
        self.cur = st.cur;
        self.grp_valid.clone_from(&st.grp_valid);
    }
}

/// `_Loop_vals_t` (`<regex>`:1488-1491). `_Loop_iter` points at a `_Do_rep` frame's
/// `_Cur_iter`, which that frame never changes, so the position itself is kept.
#[derive(Debug, Clone, Copy, Default)]
struct LoopVals {
    loop_idx: i32,
    loop_iter: Option<usize>,
}

type MatchResult<T> = Result<T, RegexError>;

/// Port of `_Matcher` (`<regex>`:1592-1704) for `char`, without `icase`, `collate` or the
/// other grammars' longest-match rule.
struct Matcher<'a> {
    nodes: &'a [msvc::Node],
    text: &'a [u8],
    tgt_state: TgtState,
    res: TgtState,
    loop_vals: Vec<LoopVals>,
    begin: usize,
    end: usize,
    first: usize,
    mflags: u32,
    matched: bool,
    cap: bool,
    ncap: usize,
    full: bool,
    max_complexity_count: i64,
    max_stack_count: i64,
}

impl<'a> Matcher<'a> {
    /// Port of `_Matcher::_Matcher` (`<regex>`:1594-1601).
    fn new(
        text: &'a [u8],
        first: usize,
        last: usize,
        program: &'a Program,
        mflags: u32,
    ) -> Matcher<'a> {
        let ncap = program.mark_count();
        Matcher {
            nodes: &program.nodes,
            text,
            tgt_state: TgtState {
                cur: 0,
                grp_valid: Vec::new(),
                grps: Vec::new(),
            },
            res: TgtState {
                cur: 0,
                grp_valid: Vec::new(),
                grps: Vec::new(),
            },
            loop_vals: vec![LoopVals::default(); program.loops()],
            begin: 0,
            end: last,
            first,
            mflags,
            matched: false,
            cap: false,
            ncap,
            full: false,
            max_complexity_count: 0,
            max_stack_count: 0,
        }
    }

    /// Port of `_Matcher::_Match(match_results *, bool)` (`<regex>`:1618-1668), without
    /// results: `regex_match(text, re)` asks for none.
    fn match_(&mut self, full_match: bool) -> MatchResult<bool> {
        self.begin = self.first;
        self.tgt_state.cur = self.first;
        self.tgt_state.grp_valid.resize(self.ncap, false);
        self.tgt_state.grps.resize(self.ncap, (0, 0));
        self.cap = false;
        self.full = full_match;
        self.max_complexity_count = MAX_COMPLEXITY_COUNT;
        self.max_stack_count = MAX_STACK_COUNT;

        self.matched = false;

        self.match_pat(Some(0))
    }

    /// Port of `_Matcher::_Do_if` (`<regex>`:3162-3207): the first alternative that matches
    /// (no longest-match rule in ECMAScript).
    fn do_if(&mut self, node: NodeId) -> MatchResult<bool> {
        let st = self.tgt_state.clone();

        // look for the first match
        let mut branch = Some(node);
        while let Some(b) = branch {
            // process one branch of if
            self.tgt_state = st.clone(); // rewind to where the alternation starts in input
            if self.match_pat(self.nodes[b].next)? {
                // try to match this branch
                return Ok(true);
            }
            branch = match self.nodes[b].data {
                NodeData::If { child, .. } => child,
                _ => unreachable!("an if node"),
            };
        }

        // if none of the if branches matched, fail to match
        Ok(false)
    }

    /// The repetition node's parameters.
    fn rep(&self, node: NodeId) -> (i32, i32, NodeId, u32, i32) {
        match self.nodes[node].data {
            NodeData::Rep {
                min,
                max,
                end_rep,
                loop_number,
                simple_loop,
            } => (min, max, end_rep, loop_number, simple_loop),
            _ => unreachable!("a repetition node"),
        }
    }

    /// Port of `_Matcher::_Do_rep0` (`<regex>`:3209-3270): a repetition with no nested
    /// alternation or repetition, iteratively.
    fn do_rep0(&mut self, node: NodeId, greedy: bool) -> MatchResult<bool> {
        let (min, max, end_rep, _, _) = self.rep(node);
        let body = self.nodes[node].next;
        let tail = self.nodes[end_rep].next;
        let mut ix = 0;
        let st = self.tgt_state.clone();

        while ix < min {
            // do minimum number of reps
            let cur = self.tgt_state.cur;
            if !self.match_pat(body)? {
                // didn't match minimum number of reps, fail
                self.tgt_state = st;
                return Ok(false);
            } else if cur == self.tgt_state.cur {
                ix = min - 1; // skip matches that don't change state
            }
            ix += 1;
        }

        let mut final_ = self.tgt_state.clone();
        let mut matched0 = false;
        let mut saved_pos = self.tgt_state.cur;

        if self.match_pat(tail)? {
            if !greedy {
                return Ok(true); // go with current match
            }

            // record an acceptable match and continue
            final_ = self.tgt_state.clone();
            matched0 = true;
        }

        loop {
            // try another rep/tail match
            let more = max == -1 || {
                let more = ix < max;
                ix += 1;
                more
            };
            if !more {
                break;
            }
            self.tgt_state.cur = saved_pos;
            self.tgt_state.grp_valid.clone_from(&st.grp_valid);
            if !self.match_pat(body)? {
                break; // rep match failed, quit loop
            }

            let mid = self.tgt_state.cur;
            if self.match_pat(tail)? {
                if !greedy {
                    return Ok(true); // go with current match
                }

                // record match and continue
                final_ = self.tgt_state.clone();
                matched0 = true;
            }

            if saved_pos == mid {
                break; // rep match ate no additional elements, quit loop
            }

            saved_pos = mid;
        }

        self.tgt_state = if matched0 { final_ } else { st };
        Ok(matched0)
    }

    /// Port of `_Matcher::_Do_rep` (`<regex>`:3272-3337): a repetition, recursively, from its
    /// `init_idx`-th iteration.
    fn do_rep(&mut self, node: NodeId, greedy: bool, init_idx: i32) -> MatchResult<bool> {
        let (min, max, end_rep, loop_number, simple_loop) = self.rep(node);
        if simple_loop == 1 {
            return self.do_rep0(node, greedy);
        }

        let body = self.nodes[node].next;
        let tail = self.nodes[end_rep].next;
        let mut matched0 = false;
        let st = self.tgt_state.clone();
        let psav = loop_number as usize;
        let loop_idx_sav = self.loop_vals[psav].loop_idx;
        let loop_iter_sav = self.loop_vals[psav].loop_iter;
        let cur_iter = self.tgt_state.cur;

        let progress = init_idx == 0
            || loop_iter_sav.expect("the iteration's start, set by the outer frame") != cur_iter;

        if 0 <= max && max <= init_idx {
            matched0 = self.match_pat(tail)?; // reps done, try tail
        } else if init_idx < min {
            // try a required rep
            if !progress {
                matched0 = self.match_pat(tail)?; // empty, try tail
            } else {
                // try another required match
                self.loop_vals[psav].loop_idx = init_idx + 1;
                self.loop_vals[psav].loop_iter = Some(cur_iter);
                matched0 = self.match_pat(body)?;
            }
        } else if !greedy {
            // not greedy, favor minimum number of reps
            matched0 = self.match_pat(tail)?;
            if !matched0 && progress {
                // tail failed, try another rep
                self.tgt_state = st.clone();
                self.loop_vals[psav].loop_idx = init_idx + 1;
                self.loop_vals[psav].loop_iter = Some(cur_iter);
                matched0 = self.match_pat(body)?;
            }
        } else {
            // greedy, favor maximum number of reps
            if progress {
                // try another rep
                self.loop_vals[psav].loop_idx = init_idx + 1;
                self.loop_vals[psav].loop_iter = Some(cur_iter);
                matched0 = self.match_pat(body)?;
            }

            if (progress || 1 >= init_idx) && !matched0 {
                // rep failed, try tail
                self.loop_vals[psav].loop_idx = loop_idx_sav;
                self.loop_vals[psav].loop_iter = loop_iter_sav;
                self.tgt_state = st.clone();
                matched0 = self.match_pat(tail)?;
            }
        }

        if !matched0 {
            self.tgt_state = st;
        }

        self.loop_vals[psav].loop_idx = loop_idx_sav;
        self.loop_vals[psav].loop_iter = loop_iter_sav;
        Ok(matched0)
    }

    /// Port of `_Matcher::_Do_class` (`<regex>`:3408-3449) and `_Lookup_coll`
    /// (`<regex>`:3385-3405): a bracket expression at the current position.
    fn do_class(&mut self, nx: NodeId) -> bool {
        let class = match &self.nodes[nx].data {
            NodeData::Class(class) => class,
            _ => unreachable!("a bracket expression"),
        };
        let ch = self.text[self.tgt_state.cur];
        let mut res0 = self.tgt_state.cur + 1;
        let found;
        if let Some(resx) = lookup_coll(self.text, self.tgt_state.cur, self.end, &class.coll) {
            // check for collation element
            res0 = resx;
            found = true;
        } else {
            found = class.small.as_ref().is_some_and(|s| s.find(u32::from(ch)));
        }

        let negated = self.nodes[nx].flags & FL_NEGATE != 0;

        if found == negated {
            false
        } else {
            // record result
            self.tgt_state.cur = res0;
            true
        }
    }

    /// Port of `_Matcher::_Better_match` (`<regex>`:3451-3466): whether the current match is
    /// better than the recorded one under UNIX rules (an earlier start, then a later end, of
    /// the first group that differs).
    fn better_match(&self) -> bool {
        for ix in 0..self.ncap {
            // check each capture group
            if self.res.grp_valid[ix] && self.tgt_state.grp_valid[ix] {
                let (rb, re) = self.res.grps[ix];
                let (tb, te) = self.tgt_state.grps[ix];
                if rb != tb {
                    return rb - self.begin < tb - self.begin;
                }

                if re != te {
                    return re - self.begin < te - self.begin;
                }
            }
        }
        false
    }

    /// Port of `_Matcher::_Is_wbound` (`<regex>`:3468-3485).
    fn is_wbound(&self) -> bool {
        let cur = self.tgt_state.cur;
        if self.mflags & MATCH_PREV_AVAIL != 0 || cur != self.begin {
            // if --_Cur is valid, check for preceding word character
            if cur == self.end {
                self.mflags & MATCH_NOT_EOW == 0 && is_word(self.text[cur - 1])
            } else {
                is_word(self.text[cur - 1]) != is_word(self.text[cur])
            }
        } else {
            // --_Cur is not valid
            if cur == self.end {
                self.mflags & (MATCH_NOT_BOW | MATCH_NOT_EOW) == 0
            } else {
                self.mflags & MATCH_NOT_BOW == 0 && is_word(self.text[cur])
            }
        }
    }

    /// Port of `_Matcher::_Match_pat` (`<regex>`:3492-3700): matches the list from `nx` on.
    fn match_pat(&mut self, mut nx: Option<NodeId>) -> MatchResult<bool> {
        if 0 < self.max_stack_count {
            self.max_stack_count -= 1;
            if self.max_stack_count <= 0 {
                return Err(msvc::error(ErrorType::Stack));
            }
        }

        if 0 < self.max_complexity_count {
            self.max_complexity_count -= 1;
            if self.max_complexity_count <= 0 {
                return Err(msvc::error(ErrorType::Complexity));
            }
        }

        let mut failed = false;
        while let Some(n) = nx {
            // match current node
            let node = &self.nodes[n];
            match node.kind {
                NodeType::Nop | NodeType::Group | NodeType::EndGroup | NodeType::Endif => {}

                NodeType::Bol => {
                    if self.mflags & MATCH_PREV_AVAIL != 0 || self.tgt_state.cur != self.begin {
                        // if --_Cur is valid, check for preceding newline
                        failed = self.text[self.tgt_state.cur - 1] != META_NL;
                    } else {
                        failed = self.mflags & MATCH_NOT_BOL != 0;
                    }
                }

                NodeType::Eol => {
                    if self.tgt_state.cur == self.end {
                        failed = self.mflags & MATCH_NOT_EOL != 0;
                    } else {
                        failed = self.text[self.tgt_state.cur] != META_NL;
                    }
                }

                NodeType::Wbound => {
                    failed = self.is_wbound() == (node.flags & FL_NEGATE != 0);
                }

                NodeType::Dot => {
                    if self.tgt_state.cur == self.end {
                        failed = true;
                    } else {
                        let ch = self.text[self.tgt_state.cur];
                        if ch == META_NL || ch == META_CR {
                            // ECMAScript
                            failed = true;
                        } else {
                            self.tgt_state.cur += 1;
                        }
                    }
                }

                NodeType::Str => {
                    // check for string match
                    let data = match &node.data {
                        NodeData::Str(data) => data,
                        _ => unreachable!("a string node"),
                    };
                    let res0 = compare(self.text, self.tgt_state.cur, self.end, data);
                    if res0 != self.tgt_state.cur {
                        self.tgt_state.cur = res0;
                    } else {
                        failed = true;
                    }
                }

                NodeType::Class => {
                    // check for bracket expression match
                    failed = self.tgt_state.cur == self.end || !self.do_class(n);
                }

                NodeType::NegAssert | NodeType::Assert => {
                    // check assert
                    let ch = self.tgt_state.cur;
                    let neg = node.kind == NodeType::NegAssert;
                    let child = match node.data {
                        NodeData::Assert { child } => child,
                        _ => unreachable!("an assertion"),
                    };
                    let st = self.tgt_state.bt();
                    if self.match_pat(child)? == neg {
                        // restore initial state and indicate failure
                        self.tgt_state.restore(&st);
                        failed = true;
                    } else {
                        self.tgt_state.cur = ch;
                    }
                }

                NodeType::EndAssert => {
                    nx = None;
                    continue;
                }

                NodeType::Capture => {
                    // record current position
                    let idx = match node.data {
                        NodeData::Index(idx) => idx as usize,
                        _ => unreachable!("a capture node"),
                    };
                    self.tgt_state.grps[idx].0 = self.tgt_state.cur;
                    for valid in &mut self.tgt_state.grp_valid[idx + 1..] {
                        *valid = false;
                    }
                }

                NodeType::EndCapture => {
                    // record successful capture
                    let back = match node.data {
                        NodeData::EndGroup { back } => back,
                        _ => unreachable!("an end of capture"),
                    };
                    let idx = match self.nodes[back].data {
                        NodeData::Index(idx) => idx as usize,
                        _ => unreachable!("a capture node"),
                    };
                    if self.cap || idx != 0 {
                        // update capture data
                        self.tgt_state.grp_valid[idx] = true;
                        self.tgt_state.grps[idx].1 = self.tgt_state.cur;
                    }
                }

                NodeType::Back => {
                    // check back reference
                    let idx = match node.data {
                        NodeData::Index(idx) => idx as usize,
                        _ => unreachable!("a back reference"),
                    };
                    if self.tgt_state.grp_valid[idx] {
                        // check for match
                        let (bx, ex) = self.tgt_state.grps[idx];
                        let res0 =
                            compare(self.text, self.tgt_state.cur, self.end, &self.text[bx..ex]);
                        if bx != ex && res0 == self.tgt_state.cur {
                            // _Bx == _Ex for zero-length match
                            failed = true;
                        } else {
                            self.tgt_state.cur = res0;
                        }
                    }
                }

                NodeType::If => {
                    if !self.do_if(n)? {
                        failed = true;
                    }

                    nx = None;
                    continue;
                }

                NodeType::Rep => {
                    let greedy = node.flags & FL_GREEDY != 0;
                    if !self.do_rep(n, greedy, 0)? {
                        failed = true;
                    }

                    nx = None;
                    continue;
                }

                NodeType::EndRep => {
                    let nr = match node.data {
                        NodeData::EndRep { begin_rep } => begin_rep,
                        _ => unreachable!("an end of repetition"),
                    };
                    let (_, _, _, loop_number, simple_loop) = self.rep(nr);
                    let greedy = self.nodes[nr].flags & FL_GREEDY != 0;
                    let loop_idx = self.loop_vals[loop_number as usize].loop_idx;

                    if simple_loop == 0 && !self.do_rep(nr, greedy, loop_idx)? {
                        failed = true; // recurse only if loop contains if/do
                    }

                    nx = None;
                    continue;
                }

                NodeType::Begin => {}

                NodeType::End => {
                    if (self.mflags & (MATCH_NOT_NULL | MATCH_NOT_NULL_INTERNAL) != 0
                        && self.begin == self.tgt_state.cur)
                        || (self.full && self.tgt_state.cur != self.end)
                    {
                        failed = true;
                    } else if !self.matched || self.better_match() {
                        // record successful match
                        self.res = self.tgt_state.clone();
                        self.matched = true;
                    }
                    nx = None;
                    continue;
                }
            }

            if failed {
                nx = None;
            } else {
                nx = self.nodes[n].next;
            }
        }

        if 0 < self.max_stack_count {
            self.max_stack_count += 1;
        }

        Ok(!failed)
    }
}

/// Port of `_Compare` and `_Cmp_chrange` (`<regex>`:3339-3364) without `icase` or `collate`:
/// the end of `pattern` matched at `begin1` in `text[..end1]`, or `begin1` when it doesn't
/// match whole.
fn compare(text: &[u8], begin1: usize, end1: usize, pattern: &[u8]) -> usize {
    let res = begin1;
    let mut b1 = begin1;
    let mut b2 = 0;
    while b1 != end1 && b2 != pattern.len() {
        let equal = text[b1] == pattern[b2];
        b1 += 1;
        b2 += 1;
        if !equal {
            return res;
        }
    }

    if b2 == pattern.len() { b1 } else { res }
}

/// Port of `_Lookup_coll` (`<regex>`:3385-3405): the end of a collating element of `seqs`
/// that `text` holds at `first`, ending at `last`. It compares each element up to its first
/// difference, past `last` when the element is longer than what is left: the STL then reads
/// the text's terminating NUL (OCIO matches C strings), which differs from every character of
/// an element; the port stops at the end of the text the same way.
fn lookup_coll(text: &[u8], first: usize, last: usize, seqs: &[msvc::Sequence]) -> Option<usize> {
    for seq in seqs {
        // look for sequence of elements that are the right size
        for elt in seq.data.chunks(seq.sz) {
            // look for character range
            let mut res = first;
            for &e in elt {
                // check current character
                let c = text.get(res).copied();
                res += 1;
                if c != Some(e) {
                    break;
                }
            }
            if res == last {
                return Some(last);
            }
        }
    }
    None
}

/// `regex_match(first, last, re)`: whether the whole text matches.
///
/// Port of `_Regex_match1` (`<regex>`:2165-2177).
pub(super) fn regex_match(program: &Program, text: &[u8]) -> Result<bool, RegexError> {
    let mut mx = Matcher::new(text, 0, text.len(), program, MATCH_DEFAULT);
    mx.match_(true)
}

// ---------------------------------------------------------------------------------------------
// regex_search and regex_replace

/// `match_continuous` (`<regex>`:128).
const MATCH_CONTINUOUS: u32 = 0x0040;
/// `format_no_copy` (`<regex>`:132).
const FORMAT_NO_COPY: u32 = 0x0800;
/// `format_first_only` (`<regex>`:133).
const FORMAT_FIRST_ONLY: u32 = 0x1000;
/// `_Skip_zero_length` (`<regex>`:135).
const SKIP_ZERO_LENGTH: u32 = 0x4000;

impl Matcher<'_> {
    /// Port of `_Matcher::_Setf` (`<regex>`:1603-1605).
    fn setf(&mut self, mf: u32) {
        self.mflags |= mf;
    }

    /// Port of `_Matcher::_Clearf` (`<regex>`:1607-1609).
    fn clearf(&mut self, mf: u32) {
        self.mflags &= !mf;
    }

    /// `_Matcher::_Match(_Pfirst, _Matches, false)` (`<regex>`:1611-1616) with results: the
    /// whole match's span, or `None`.
    fn match_at(&mut self, pfirst: usize) -> MatchResult<Option<(usize, usize)>> {
        self.first = pfirst;
        self.match_results()
    }

    /// `_Matcher::_Match(_Matches, false)` (`<regex>`:1618-1668) with results: the span of
    /// group 0 (`_Matches->_At(0)`), or `None`.
    fn match_results(&mut self) -> MatchResult<Option<(usize, usize)>> {
        self.begin = self.first;
        self.tgt_state.cur = self.first;
        self.tgt_state.grp_valid.resize(self.ncap, false);
        self.tgt_state.grps.resize(self.ncap, (0, 0));
        self.cap = true;
        self.full = false;
        self.max_complexity_count = MAX_COMPLEXITY_COUNT;
        self.max_stack_count = MAX_STACK_COUNT;

        self.matched = false;

        if !self.match_pat(Some(0))? {
            return Ok(None);
        }

        // copy results to _Matches
        Ok(Some(if self.res.grp_valid[0] {
            self.res.grps[0]
        } else {
            (self.end, self.end)
        }))
    }

    /// The first position from `first_arg` where a match could start: skips what the
    /// expression's first nodes can't match.
    ///
    /// Port of `_Matcher::_Skip` (`<regex>`:3710-3846). It reads the character before
    /// `first_arg`, which is valid.
    fn skip(&self, mut first_arg: usize, mut last: usize, node_arg: Option<NodeId>) -> usize {
        let mut nx = Some(node_arg.unwrap_or(0));

        while first_arg != last {
            let Some(n) = nx else { break };
            // check current node
            let node = &self.nodes[n];
            match node.kind {
                NodeType::Nop => {}

                NodeType::Bol => {
                    // check for embedded newline
                    // return iterator to character just after the newline; for input like
                    // "\nabc" matching "^abc", _First_arg could be pointing at 'a', so we
                    // need to check --_First_arg for '\n'
                    if self.text[first_arg - 1] != META_NL {
                        first_arg = self.text[first_arg..last]
                            .iter()
                            .position(|&c| c == META_NL)
                            .map_or(last, |i| first_arg + i);
                        if first_arg != last {
                            first_arg += 1;
                        }
                    }

                    return first_arg;
                }

                NodeType::Eol => {
                    return self.text[first_arg..last]
                        .iter()
                        .position(|&c| c == META_NL)
                        .map_or(last, |i| first_arg + i);
                }

                NodeType::Str => {
                    // check for string match
                    let data = match &node.data {
                        NodeData::Str(data) => data,
                        _ => unreachable!("a string node"),
                    };
                    while first_arg != last {
                        // look for starting match
                        if compare(self.text, first_arg, first_arg + 1, &data[..1]) != first_arg {
                            break;
                        }
                        first_arg += 1;
                    }
                    return first_arg;
                }

                NodeType::Class => {
                    // check for string match
                    let class = match &node.data {
                        NodeData::Class(class) => class,
                        _ => unreachable!("a bracket expression"),
                    };
                    let negated = node.flags & FL_NEGATE != 0;
                    while first_arg != last {
                        // look for starting match
                        let ch = self.text[first_arg];
                        let next = first_arg + 1;

                        let found = if lookup_coll(self.text, first_arg, next, &class.coll)
                            .is_some_and(|r| r != first_arg)
                        {
                            true
                        } else {
                            class.small.as_ref().is_some_and(|s| s.find(u32::from(ch)))
                        };

                        if found != negated {
                            return first_arg;
                        }
                        first_arg += 1;
                    }
                    return first_arg;
                }

                NodeType::Group | NodeType::EndGroup => {}

                NodeType::EndAssert => {
                    nx = None;
                    continue;
                }

                NodeType::Capture | NodeType::EndCapture => {}

                NodeType::If => {
                    // check for soonest string match
                    let mut node_if = Some(n);
                    while first_arg != last {
                        let Some(i) = node_if else { break };
                        last = self.skip(first_arg, last, self.nodes[i].next);
                        node_if = match self.nodes[i].data {
                            NodeData::If { child, .. } => child,
                            _ => unreachable!("an alternative"),
                        };
                    }

                    return last;
                }

                NodeType::Begin => {}

                NodeType::End => {
                    nx = None;
                    continue;
                }

                NodeType::Wbound
                | NodeType::Dot
                | NodeType::Assert
                | NodeType::NegAssert
                | NodeType::Back
                | NodeType::Endif
                | NodeType::Rep
                | NodeType::EndRep => return first_arg,
            }
            nx = self.nodes[n].next;
        }
        first_arg
    }
}

/// `_Regex_search2(_First, _Last, &_Matches, _Re, _Flgs, _Org)`: the span of the first match
/// in `text[first..last]`, `None` for none. `text` holds the characters before `first`, which
/// `match_prev_avail` lets the matcher read.
///
/// Port of `_Regex_search2` (`<regex>`:2234-2276).
fn regex_search2(
    program: &Program,
    text: &[u8],
    mut first: usize,
    last: usize,
    flgs: u32,
) -> MatchResult<Option<(usize, usize)>> {
    // search for regular expression match in target text
    if flgs & SKIP_ZERO_LENGTH != 0 && first != last {
        first += 1;
    }

    let mut mx = Matcher::new(text, first, last, program, flgs);

    let mut found = mx.match_results()?;
    if found.is_none() && first != last && flgs & MATCH_CONTINUOUS == 0 {
        // try more on suffixes
        mx.setf(MATCH_PREV_AVAIL);
        mx.clearf(MATCH_NOT_NULL_INTERNAL);
        loop {
            first = mx.skip(first + 1, last, None);
            if first == last {
                break;
            }
            if let Some(span) = mx.match_at(first)? {
                // found match starting at _First
                found = Some(span);
                break;
            }
        }

        if found.is_none() {
            found = mx.match_at(last)?;
        }
    }

    Ok(found)
}

/// `regex_replace(s, re, fmt)` with `format_default`, for a format without `$`: `text` with
/// each match replaced by `fmt`, as the ECMAScript format rules copy a format without escapes
/// (`_Format_default`, `<regex>`:2092-2135).
///
/// Port of `_Regex_replace1` (`<regex>`:2336-2366).
pub(super) fn regex_replace(program: &Program, text: &[u8], fmt: &[u8]) -> MatchResult<Vec<u8>> {
    let flgs = MATCH_DEFAULT;
    let last = text.len();
    let mut result = Vec::new();

    // search and replace
    let mut pos = 0;
    let mut flags = flgs;
    let mut not_null = 0;

    while let Some((m_first, m_second)) = regex_search2(program, text, pos, last, flags | not_null)?
    {
        // replace at each match
        if flgs & FORMAT_NO_COPY == 0 {
            result.extend_from_slice(&text[pos..m_first]);
        }

        result.extend_from_slice(fmt);

        pos = m_second;
        if pos == last || flgs & FORMAT_FIRST_ONLY != 0 {
            break;
        }

        if m_first == m_second {
            not_null = MATCH_NOT_NULL_INTERNAL;
        } else {
            // non-null match, recognize earlier text
            not_null = 0;
            flags |= MATCH_PREV_AVAIL;
        }
    }
    if flgs & FORMAT_NO_COPY == 0 {
        result.extend_from_slice(&text[pos..last]);
    }
    Ok(result)
}
