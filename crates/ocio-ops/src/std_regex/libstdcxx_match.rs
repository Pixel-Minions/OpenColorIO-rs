// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Matching as the Linux wheel does: a depth-first backtracking matcher over the parsed
//! expression, which tries alternatives and repetitions in ECMAScript's order (ECMA-262 3rd
//! edition, 15.10.2) and departs from it where the Linux wheel does:
//! - a back reference to a group that matched nothing fails (ECMAScript: matches empty);
//! - a repetition doesn't clear its groups' captures before each iteration;
//! - an iteration may match empty: a loop is entered at most twice at the same position
//!   (ECMAScript refuses an empty iteration).
//!
//! `{n,m}` is `n` copies of the body, then `m - n` nested optional ones; `{n,}` is `n` copies,
//! then a loop.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::libstdcxx::{Node, Program};
use super::{ErrorType, RegexError};

/// A capture: its span, or `None` while it matched nothing.
type Captures = Vec<Option<(usize, usize)>>;

/// What a loop remembers: the position it was last entered at, and how many times.
type LoopState = (Option<usize>, u32);

struct Matcher<'a> {
    text: &'a [u8],
    /// Each loop's state, by the loop's identity (the repetition node and the copy).
    loops: RefCell<HashMap<(usize, u64), LoopState>>,
    /// How deep the matching recursion is, and whether it went past `MAX_DEPTH`.
    depth: Cell<u32>,
    aborted: Cell<bool>,
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

type Cont<'c> = &'c mut dyn FnMut(usize, &mut Captures) -> bool;

impl<'a> Matcher<'a> {
    /// Matches `node` at `pos`, then the continuation `k`.
    fn m(&self, node: &Node, pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        if self.aborted.get() {
            return false;
        }
        let depth = self.depth.get() + 1;
        if depth > MAX_DEPTH {
            self.aborted.set(true);
            return false;
        }
        self.depth.set(depth);
        let matched = self.m_inner(node, pos, caps, k);
        self.depth.set(depth - 1);
        matched && !self.aborted.get()
    }

    fn m_inner(&self, node: &Node, pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        match node {
            Node::Empty => k(pos, caps),
            Node::Char(c) => pos < self.text.len() && self.text[pos] == *c && k(pos + 1, caps),
            Node::Set(set) => {
                pos < self.text.len() && set.contains(self.text[pos]) && k(pos + 1, caps)
            }
            Node::LineBegin => pos == 0 && k(pos, caps),
            Node::LineEnd => pos == self.text.len() && k(pos, caps),
            Node::WordBoundary { negated } => {
                let before = pos > 0 && is_word(self.text[pos - 1]);
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
                None => self.m(body, pos, caps, k),
                Some(i) => {
                    let i = *i;
                    let saved = caps[i];
                    let start = pos;
                    let ok = self.m(body, pos, caps, &mut |end, caps: &mut Captures| {
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
                let mut inner = caps.clone();
                let matched = self.m(body, pos, &mut inner, &mut |_, _| true);
                if matched == *negated {
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
            Node::Concat(nodes) => self.concat(nodes, pos, caps, k),
            Node::Alternation(nodes) => {
                for n in nodes {
                    let saved = caps.clone();
                    if self.m(n, pos, caps, k) {
                        return true;
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
                ..
            } => {
                let id = node as *const Node as usize;
                let (min, max, greedy) = (*min, *max, *greedy);
                self.copies(
                    body,
                    min,
                    pos,
                    caps,
                    &mut |p, caps: &mut Captures| match max {
                        None => self.star(id, body, greedy, p, caps, k),
                        Some(max) => self.optionals(id, body, max - min, greedy, p, caps, k),
                    },
                )
            }
        }
    }

    fn concat(&self, nodes: &[Node], pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        match nodes.split_first() {
            None => k(pos, caps),
            Some((first, rest)) => self.m(first, pos, caps, &mut |p, caps: &mut Captures| {
                self.concat(rest, p, caps, k)
            }),
        }
    }

    /// `n` copies of `body` in sequence.
    fn copies(&self, body: &Node, n: u64, pos: usize, caps: &mut Captures, k: Cont<'_>) -> bool {
        if n == 0 {
            return k(pos, caps);
        }
        self.m(body, pos, caps, &mut |p, caps: &mut Captures| {
            self.copies(body, n - 1, p, caps, k)
        })
    }

    /// Runs `f` as another iteration of loop `key` at `pos`, unless the loop was already
    /// entered twice there.
    fn once_more(&self, key: (usize, u64), pos: usize, f: impl FnOnce() -> bool) -> bool {
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

    /// The loop of `body*`.
    fn star(
        &self,
        id: usize,
        body: &Node,
        greedy: bool,
        pos: usize,
        caps: &mut Captures,
        k: Cont<'_>,
    ) -> bool {
        let key = (id, u64::MAX);
        let saved = caps.clone();
        if !greedy {
            if k(pos, caps) {
                return true;
            }
            *caps = saved.clone();
        }
        let iterated = self.once_more(key, pos, || {
            self.m(body, pos, caps, &mut |p, caps: &mut Captures| {
                self.star(id, body, greedy, p, caps, k)
            })
        });
        if iterated {
            return true;
        }
        *caps = saved;
        greedy && k(pos, caps)
    }

    /// `remaining` nested optional copies of `body`.
    #[allow(clippy::too_many_arguments)]
    fn optionals(
        &self,
        id: usize,
        body: &Node,
        remaining: u64,
        greedy: bool,
        pos: usize,
        caps: &mut Captures,
        k: Cont<'_>,
    ) -> bool {
        if remaining == 0 {
            return k(pos, caps);
        }
        let key = (id, remaining);
        let saved = caps.clone();
        if !greedy {
            if k(pos, caps) {
                return true;
            }
            *caps = saved.clone();
        }
        let taken = self.once_more(key, pos, || {
            self.m(body, pos, caps, &mut |p, caps: &mut Captures| {
                self.optionals(id, body, remaining - 1, greedy, p, caps, k)
            })
        });
        if taken {
            return true;
        }
        *caps = saved;
        greedy && k(pos, caps)
    }
}

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

/// `regex_match(text, re)`: whether the whole text matches.
pub(super) fn regex_match(program: &Program, text: &[u8]) -> Result<bool, RegexError> {
    if text.len() > MAX_TEXT {
        return Err(RegexError {
            code: ErrorType::Stack,
            what: "regex_error",
        });
    }
    let run = || {
        let matcher = Matcher {
            text,
            loops: RefCell::new(HashMap::new()),
            depth: Cell::new(0),
            aborted: Cell::new(false),
        };
        let mut caps: Captures = vec![None; program.groups];
        let end = text.len();
        let matched = matcher.m(&program.root, 0, &mut caps, &mut |p, _| p == end);
        if matcher.aborted.get() {
            Err(RegexError {
                code: ErrorType::Stack,
                what: "regex_error",
            })
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
