// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Matching as the Linux wheel does: a depth-first backtracking matcher over the parsed
//! expression, which tries alternatives and repetitions in ECMAScript's order (ECMA-262 3rd
//! edition, 15.10.2) and departs from it where the Linux wheel does (I-122):
//! - a back reference to a group that matched nothing fails (ECMAScript: matches empty);
//! - a repetition doesn't clear its groups' captures before each iteration;
//! - an iteration may match empty: a loop is entered at most twice at the same position
//!   (ECMAScript refuses an empty iteration);
//! - a brace copies its body: `{n,m}` is `n` copies, then `m - n` nested optional ones, and
//!   `{n,}` is `n` copies, then a loop of another copy; each copy's loops count their entries
//!   apart (`*`, `+` and `?` don't copy);
//! - a lookahead matches as an expression of its own that starts where it stands: there, `^`
//!   matches and `\b` sees no character before.
//!
//! The wheel's matcher recurses once per state it goes through, and its stack overflows (the
//! process ends) about 75,000 states deep. The port keeps a conservative estimate of that
//! depth along the path it tries ([`cost`](Matcher::cost)), and refuses a match that would go
//! past half of it, or a text longer than 8,192 bytes (U-54).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::libstdcxx::{Node, Program, RepeatForm};
use super::{ErrorType, RegexError};

/// A capture: its span, or `None` while it matched nothing.
type Captures = Vec<Option<(usize, usize)>>;

/// What a loop remembers: the position it was last entered at, and how many times.
type LoopState = (Option<usize>, u32);

/// A loop's identity: its repetition node, the copy it is in (a hash of the copies of the
/// braces around it), and which of the repetition's loops it is.
type LoopKey = (usize, u64, u64);

// The wheel's stack, in hundredths of the frames a character match takes there. Measured with
// the Linux wheel (2026-10-05), from the path length at which matching crashes it: `a*` at
// 32,719 characters, `(a)*` at 17,460, `((((a))))*` at 7,272, `(a|b)*` at 14,150,
// `(?:b|a)*` at 32,726, `a*?` at 58,197, `(?:aaaa)*` at 14,156, `(?:a?)*` at 20,943, all
// about 74,900 frames; non-capture groups, lookaheads and copies of a brace's body add none.

/// A character or set matched, an assertion, a back reference.
const COST_ATOM: u32 = 100;
/// A capture group: its start and end states.
const COST_CAPTURE: u32 = 200;
/// An alternative other than the last (the last is reached without a frame).
const COST_ALTERNATIVE: u32 = 100;
/// An iteration of a greedy loop (`*`, `+`, `?`, a brace's loop or optional copy).
const COST_GREEDY_ITERATION: u32 = 129;
/// An iteration of a lazy loop.
const COST_LAZY_ITERATION: u32 = 29;
/// The deepest the wheel's stack goes, about 74,900 frames: the lowest of the measured
/// crashes is at 74,829 (`((((a))))*`).
const WHEEL_STACK: u32 = 7_482_900;
/// The port refuses a match whose path would go past this: under half the wheel's stack.
const MAX_COST: u32 = 3_700_000;

/// The longest text the port matches (U-54).
const MAX_TEXT: usize = 8192;

/// The stack the port's own recursion may use; the thread has a margin over it. The cost
/// bound keeps a match far below it; it guards against expressions whose nesting costs the
/// wheel nothing (thousands of non-capture groups).
const STACK_BUDGET: usize = 192 * 1024 * 1024;
const STACK_SIZE: usize = 256 * 1024 * 1024;

const _: () = assert!(2 * MAX_COST < WHEEL_STACK);

struct Matcher<'a> {
    text: &'a [u8],
    /// Each loop's state.
    loops: RefCell<HashMap<LoopKey, LoopState>>,
    /// Where the expression being matched starts: the text's start, or a lookahead's.
    begin: Cell<usize>,
    /// The estimated depth of the wheel's stack along the current path.
    cost: Cell<u32>,
    /// The address of a variable at the start of the thread, which the stack grows from.
    stack_base: usize,
    /// Whether the match went past a limit.
    aborted: Cell<bool>,
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The copy of a brace's body: a hash of the enclosing copies `ctx`, the brace and the copy's
/// index (splitmix64).
fn mix(ctx: u64, id: usize, index: u64) -> u64 {
    let mut z = ctx
        ^ (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ index.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The address of a local variable of the caller: how deep its stack is.
#[inline(always)]
fn stack_address() -> usize {
    let marker = 0u8;
    std::ptr::addr_of!(marker) as usize
}

type Cont<'c> = &'c mut dyn FnMut(usize, &mut Captures) -> bool;

/// A repetition's loops, by kind, in [`LoopKey`]s.
const STAR_LOOP: u64 = u64::MAX;

impl<'a> Matcher<'a> {
    /// Runs `f` with `weight` added to the path's cost, unless that goes past the bound or the
    /// port's stack budget (the match is then aborted).
    fn weighted(&self, weight: u32, f: impl FnOnce() -> bool) -> bool {
        if self.aborted.get() {
            return false;
        }
        let cost = self.cost.get() + weight;
        if cost > MAX_COST || self.stack_base.abs_diff(stack_address()) > STACK_BUDGET {
            self.aborted.set(true);
            return false;
        }
        self.cost.set(cost);
        let matched = f();
        self.cost.set(cost - weight);
        matched && !self.aborted.get()
    }

    /// Matches `node` at `pos` in the copy `ctx`, then the continuation `k`.
    fn m(&self, node: &Node, pos: usize, caps: &mut Captures, ctx: u64, k: Cont<'_>) -> bool {
        let weight = match node {
            Node::Char(_)
            | Node::Set(_)
            | Node::LineBegin
            | Node::LineEnd
            | Node::WordBoundary { .. }
            | Node::Backref(_) => COST_ATOM,
            Node::Group { index: Some(_), .. } => COST_CAPTURE,
            _ => 0,
        };
        self.weighted(weight, || self.m_inner(node, pos, caps, ctx, k))
    }

    fn m_inner(&self, node: &Node, pos: usize, caps: &mut Captures, ctx: u64, k: Cont<'_>) -> bool {
        match node {
            Node::Empty => k(pos, caps),
            Node::Char(c) => pos < self.text.len() && self.text[pos] == *c && k(pos + 1, caps),
            Node::Set(set) => {
                pos < self.text.len() && set.contains(self.text[pos]) && k(pos + 1, caps)
            }
            Node::LineBegin => pos == self.begin.get() && k(pos, caps),
            Node::LineEnd => pos == self.text.len() && k(pos, caps),
            Node::WordBoundary { negated } => {
                let before = pos > self.begin.get() && is_word(self.text[pos - 1]);
                let after = pos < self.text.len() && is_word(self.text[pos]);
                ((before != after) != *negated) && k(pos, caps)
            }
            Node::Backref(n) => match caps[*n] {
                Some((b, e)) => {
                    let len = e - b;
                    pos + len <= self.text.len()
                        && self.text[pos..pos + len] == self.text[b..e]
                        && k(pos + len, caps)
                }
                None => false,
            },
            Node::Group { index, body } => match index {
                None => self.m(body, pos, caps, ctx, k),
                Some(i) => {
                    let i = *i;
                    let saved = caps[i];
                    let start = pos;
                    let ok = self.m(body, pos, caps, ctx, &mut |end, caps: &mut Captures| {
                        let inner = caps[i];
                        caps[i] = Some((start, end));
                        if k(end, caps) {
                            return true;
                        }
                        caps[i] = inner;
                        false
                    });
                    if !ok {
                        caps[i] = saved;
                    }
                    ok
                }
            },
            Node::Lookahead { negated, body } => {
                // An expression of its own, which starts here.
                let mut inner = caps.clone();
                let outer_begin = self.begin.replace(pos);
                let matched = self.m(body, pos, &mut inner, ctx, &mut |_, _| true);
                self.begin.set(outer_begin);
                if matched == *negated || self.aborted.get() {
                    return false;
                }
                if *negated {
                    k(pos, caps)
                } else {
                    let saved = std::mem::replace(caps, inner);
                    if k(pos, caps) {
                        return true;
                    }
                    *caps = saved;
                    false
                }
            }
            Node::Concat(nodes) => self.concat(nodes, pos, caps, ctx, k),
            Node::Alternation(nodes) => {
                let last = nodes.len() - 1;
                for (i, n) in nodes.iter().enumerate() {
                    let saved = caps.clone();
                    let weight = if i < last { COST_ALTERNATIVE } else { 0 };
                    if self.weighted(weight, || self.m(n, pos, caps, ctx, &mut *k)) {
                        return true;
                    }
                    if self.aborted.get() {
                        return false;
                    }
                    *caps = saved;
                }
                false
            }
            Node::Repeat {
                body,
                min,
                max,
                greedy,
                form,
                ..
            } => {
                let repeat = Repeat {
                    id: node as *const Node as usize,
                    body,
                    min: *min,
                    max: *max,
                    greedy: *greedy,
                    copies: *form == RepeatForm::Brace,
                    ctx,
                };
                self.copies(&repeat, 0, pos, caps, k)
            }
        }
    }

    fn concat(
        &self,
        nodes: &[Node],
        pos: usize,
        caps: &mut Captures,
        ctx: u64,
        k: Cont<'_>,
    ) -> bool {
        match nodes.split_first() {
            None => k(pos, caps),
            Some((first, rest)) => self.m(first, pos, caps, ctx, &mut |p, caps: &mut Captures| {
                self.concat(rest, p, caps, ctx, k)
            }),
        }
    }

    /// The required copies of a repetition, from copy `j`, then its loop or optional copies.
    fn copies(&self, r: &Repeat<'_>, j: u64, pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        if j == r.min {
            return match r.max {
                None => self.star(r, pos, caps, k),
                Some(max) => self.optionals(r, max - r.min, pos, caps, k),
            };
        }
        self.m(
            r.body,
            pos,
            caps,
            r.copy(j),
            &mut |p, caps: &mut Captures| self.copies(r, j + 1, p, caps, k),
        )
    }

    /// Runs `f` as another iteration of loop `key` at `pos`, unless the loop was already
    /// entered twice there.
    fn once_more(&self, key: LoopKey, pos: usize, f: impl FnOnce() -> bool) -> bool {
        let state = self.loops.borrow().get(&key).copied().unwrap_or((None, 0));
        let next = if state.0 != Some(pos) {
            (Some(pos), 1)
        } else if state.1 < 2 {
            (Some(pos), state.1 + 1)
        } else {
            return false;
        };
        self.loops.borrow_mut().insert(key, next);
        let result = f();
        self.loops.borrow_mut().insert(key, state);
        result
    }

    /// The loop of a repetition without a maximum.
    fn star(&self, r: &Repeat<'_>, pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        let saved = caps.clone();
        if !r.greedy {
            if k(pos, caps) {
                return true;
            }
            if self.aborted.get() {
                return false;
            }
            *caps = saved.clone();
        }
        let iterated = self.once_more((r.id, r.ctx, STAR_LOOP), pos, || {
            self.weighted(r.iteration_cost(), || {
                self.m(
                    r.body,
                    pos,
                    caps,
                    r.copy(r.min),
                    &mut |p, caps: &mut Captures| self.star(r, p, caps, k),
                )
            })
        });
        if iterated {
            return true;
        }
        if self.aborted.get() {
            return false;
        }
        *caps = saved;
        r.greedy && k(pos, caps)
    }

    /// `remaining` nested optional copies of a repetition's body.
    fn optionals(
        &self,
        r: &Repeat<'_>,
        remaining: u64,
        pos: usize,
        caps: &mut Captures,
        k: Cont<'_>,
    ) -> bool {
        if remaining == 0 {
            return k(pos, caps);
        }
        let saved = caps.clone();
        if !r.greedy {
            if k(pos, caps) {
                return true;
            }
            if self.aborted.get() {
                return false;
            }
            *caps = saved.clone();
        }
        let copy = r.max.unwrap_or(r.min) - remaining;
        let taken = self.once_more((r.id, r.ctx, remaining), pos, || {
            self.weighted(r.iteration_cost(), || {
                self.m(
                    r.body,
                    pos,
                    caps,
                    r.copy(copy),
                    &mut |p, caps: &mut Captures| self.optionals(r, remaining - 1, p, caps, k),
                )
            })
        });
        if taken {
            return true;
        }
        if self.aborted.get() {
            return false;
        }
        *caps = saved;
        r.greedy && k(pos, caps)
    }
}

/// A repetition being matched.
struct Repeat<'n> {
    /// The repetition node's identity.
    id: usize,
    body: &'n Node,
    min: u64,
    max: Option<u64>,
    greedy: bool,
    /// Whether it copies its body (a brace).
    copies: bool,
    /// The copy the repetition itself is in.
    ctx: u64,
}

impl Repeat<'_> {
    /// The copy that the body's `index`-th occurrence is in.
    fn copy(&self, index: u64) -> u64 {
        if self.copies {
            mix(self.ctx, self.id, index)
        } else {
            self.ctx
        }
    }

    fn iteration_cost(&self) -> u32 {
        if self.greedy {
            COST_GREEDY_ITERATION
        } else {
            COST_LAZY_ITERATION
        }
    }
}

/// The error of a match the port refuses (U-54): `error_stack`, whose `what()` is
/// `regex_error` (a code without a message).
fn refused() -> RegexError {
    RegexError {
        code: ErrorType::Stack,
        what: "regex_error",
    }
}

/// `regex_match(text, re)`: whether the whole text matches.
pub(super) fn regex_match(program: &Program, text: &[u8]) -> Result<bool, RegexError> {
    if text.len() > MAX_TEXT {
        return Err(refused());
    }
    let run = || {
        let matcher = Matcher {
            text,
            loops: RefCell::new(HashMap::new()),
            begin: Cell::new(0),
            cost: Cell::new(0),
            stack_base: stack_address(),
            aborted: Cell::new(false),
        };
        let mut caps: Captures = vec![None; program.groups];
        let end = text.len();
        let matched = matcher.m(&program.root, 0, &mut caps, 0, &mut |p, _| p == end);
        if matcher.aborted.get() {
            Err(refused())
        } else {
            Ok(matched)
        }
    };
    super::on_stack(STACK_SIZE, run).unwrap_or(Err(RegexError {
        code: ErrorType::Space,
        what: "regex_error",
    }))
}
