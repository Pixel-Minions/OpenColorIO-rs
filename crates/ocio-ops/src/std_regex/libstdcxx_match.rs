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

/// The longest text the port matches: the wheel's matcher recurses per character, and its
/// stack overflows (the process ends) at about 14,000 to 58,000 characters (U-54). Longer
/// texts are refused with `error_stack` (as `regex_error`, the text of a code without a
/// message).
const MAX_TEXT: usize = 8192;

/// The deepest the port's matching recursion goes before it refuses the match the same way: a
/// match the wheel's recursion would take about as deep as its stack allows (U-54).
const MAX_DEPTH: u32 = 100_000;

/// The stack the matching recursion needs per level, with a margin.
const STACK_PER_DEPTH: usize = 1024;

struct Matcher<'a> {
    text: &'a [u8],
    /// Each loop's state.
    loops: RefCell<HashMap<LoopKey, LoopState>>,
    /// Where the expression being matched starts: the text's start, or a lookahead's.
    begin: Cell<usize>,
    /// How deep the matching recursion is, and whether it went past `MAX_DEPTH`.
    depth: Cell<u32>,
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

type Cont<'c> = &'c mut dyn FnMut(usize, &mut Captures) -> bool;

/// A repetition's loops, by kind, in [`LoopKey`]s.
const STAR_LOOP: u64 = u64::MAX;

impl<'a> Matcher<'a> {
    /// Matches `node` at `pos` in the copy `ctx`, then the continuation `k`.
    fn m(&self, node: &Node, pos: usize, caps: &mut Captures, ctx: u64, k: Cont<'_>) -> bool {
        if self.aborted.get() {
            return false;
        }
        let depth = self.depth.get() + 1;
        if depth > MAX_DEPTH {
            self.aborted.set(true);
            return false;
        }
        self.depth.set(depth);
        let matched = self.m_inner(node, pos, caps, ctx, k);
        self.depth.set(depth - 1);
        matched && !self.aborted.get()
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
                for n in nodes {
                    let saved = caps.clone();
                    if self.m(n, pos, caps, ctx, &mut *k) {
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
            self.m(
                r.body,
                pos,
                caps,
                r.copy(r.min),
                &mut |p, caps: &mut Captures| self.star(r, p, caps, k),
            )
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
            self.m(
                r.body,
                pos,
                caps,
                r.copy(copy),
                &mut |p, caps: &mut Captures| self.optionals(r, remaining - 1, p, caps, k),
            )
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
            depth: Cell::new(0),
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
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(1024 * 1024 + MAX_DEPTH as usize * STACK_PER_DEPTH)
            .spawn_scoped(scope, run)
            .expect("a thread to match the expression")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}
