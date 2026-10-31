// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Parsing of the tools' command lines: a port of `src/apputils/argparse.h` and `argparse.cpp`
//! @ v2.5.2 (Copyright 2008 Larry Gritz and the other authors and contributors, BSD-3-Clause;
//! THIRD-PARTY-NOTICES.md).
//!
//! `ArgParse::options` takes a C varargs list: an option's format string, a pointer per
//! parameter, its description, and so on to a NULL. Here each entry of that list is a call of
//! [`ArgParse::option`], [`ArgParse::sublist`] or [`ArgParse::separator`]. Several options may
//! store in one variable, as they do in `ociowrite`, so the variables are `Cell`s and
//! `RefCell`s. `Strutil::vformat` (`src/apputils/strutil.cpp` @ v2.5.2) only ever formats
//! `ArgParse::error`'s messages, of `%s` and `%d`, whose bytes [`ArgParse::parse`] puts together
//! directly.
//!
//! Left out, since no ported tool calls them: `command_line`, `found` (with the count it
//! reads) and `error_message`.
//!
//! The quirks of this parser are part of the tools' behaviour and are ported; they are listed
//! in `docs/improvements.md`, I-600.

use std::cell::{Cell, RefCell};

use crate::c_numbers::{atof, atoi};
use crate::stdio::{cout, cout_char};

/// Where an option stores a parameter: the types `ArgOption::set_parameter` knows.
///
/// Port of the codes of `ArgOption::set_parameter` (argparse.cpp:214-254 @ v2.5.2).
#[derive(Debug, Clone, Copy)]
pub enum Param<'a> {
    /// The flag of an option without parameters: `bool`.
    Flag(&'a Cell<bool>),
    /// `%d`: `int`.
    Int(&'a Cell<i32>),
    /// `%f` and `%g`: `float`.
    Float(&'a Cell<f32>),
    /// `%s`: `std::string`.
    Text(&'a RefCell<Vec<u8>>),
}

/// The function that receives the arguments that aren't options, one at a time.
pub type Sublist<'a> = &'a dyn Fn(&[u8]) -> i32;

/// Port of `ArgOption::OptionType` (argparse.cpp:85 @ v2.5.2).
#[derive(Clone, Copy, PartialEq, Eq)]
enum OptionType {
    None,
    Regular,
    Flag,
    Sublist,
}

/// One option.
///
/// Port of `ArgOption` (argparse.cpp:45-98 @ v2.5.2), without the members that only `found`
/// reads and without the sublist's list of arguments, which nothing fills.
struct ArgOption<'a> {
    format: Vec<u8>,
    flag: Vec<u8>,
    option_type: OptionType,
    count: usize,
    params: Vec<Param<'a>>,
    callback: Option<Sublist<'a>>,
    description: Vec<u8>,
}

impl<'a> ArgOption<'a> {
    /// Parses the format string (`-option %s %d %f`) to extract the flag (`-option`) and to
    /// count the parameters.
    ///
    /// Port of `ArgOption::ArgOption` and `ArgOption::initialize` (argparse.cpp:104-195 @
    /// v2.5.2). Its `assert`s aren't compiled into the wheels, and the formats are the tools'
    /// own, which they all pass.
    fn new(format: &[u8]) -> ArgOption<'a> {
        let mut option = ArgOption {
            format: format.to_vec(),
            flag: Vec::new(),
            option_type: OptionType::None,
            count: 0,
            params: Vec::new(),
            callback: None,
            description: Vec::new(),
        };
        if format.is_empty() || format == b"%*" {
            option.option_type = OptionType::Sublist;
            option.count = 1; // Sublist callback function pointer.
        } else if format != b"<SEPARATOR>" {
            // Extract the flag name.
            let at = |i: usize| format.get(i).copied().unwrap_or(0);
            let mut s = 1;
            if at(s) == b'-' {
                s += 1;
            }
            while at(s).is_ascii_alphanumeric() || at(s) == b'_' || at(s) == b'-' {
                s += 1;
            }
            if at(s) == 0 {
                option.flag = format.to_vec();
                option.option_type = OptionType::Flag;
                option.count = 1;
            } else {
                option.flag = format[..s].to_vec();
                // Parse the scanf-like parameters.
                option.option_type = OptionType::Regular;
                while at(s) != 0 {
                    if at(s) == b'%' {
                        s += 1;
                        option.count += 1; // Adding another parameter.
                        match at(s) {
                            b'd' | b'g' | b'f' | b's' => {}
                            b'*' => option.option_type = OptionType::Sublist,
                            other => {
                                // The tools' formats have none of these.
                                crate::stdio::cerr(b"Programmer error:  Unknown option ");
                                crate::stdio::cerr(b"type string \"");
                                crate::stdio::cerr([other]);
                                crate::stdio::cerr(b"\"\n");
                                std::process::abort();
                            }
                        }
                    }
                    s += 1;
                }
            }
        }
        option
    }

    /// Given a string from argv, sets the option's parameter `i`; `None` is upstream's `NULL`,
    /// a flag's call.
    ///
    /// Port of `ArgOption::set_parameter` (argparse.cpp:214-254 @ v2.5.2).
    fn set_parameter(&self, i: usize, argv: Option<&[u8]>) {
        let argv = argv.unwrap_or_default();
        match self.params[i] {
            Param::Int(p) => p.set(atoi(argv)),
            Param::Float(p) => p.set(atof(argv) as f32),
            Param::Text(p) => *p.borrow_mut() = argv.to_vec(),
            Param::Flag(p) => p.set(true),
        }
    }
}

/// A command line's description, and its parser.
///
/// Port of `ArgParse` (argparse.h:123-173 @ v2.5.2).
pub struct ArgParse<'a> {
    errmessage: RefCell<Vec<u8>>,
    global: Option<usize>,
    intro: Vec<u8>,
    option: Vec<ArgOption<'a>>,
    /// `options` returned early, on an error: what follows in its list isn't read.
    stopped: bool,
}

impl<'a> ArgParse<'a> {
    /// `ArgParse ap;` and the start of `ap.options(intro, ...)`.
    ///
    /// Port of `ArgParse::ArgParse` and the first lines of `ArgParse::options` (argparse.cpp:
    /// 288-291, 375-380 @ v2.5.2).
    pub fn new(intro: impl AsRef<[u8]>) -> ArgParse<'a> {
        ArgParse {
            errmessage: RefCell::new(Vec::new()),
            global: None,
            intro: intro.as_ref().to_vec(),
            option: Vec::new(),
            stopped: false,
        }
    }

    /// One turn of the loop of `ArgParse::options`: the option `format` and its description.
    ///
    /// Port of `ArgParse::options` (argparse.cpp:381-423 @ v2.5.2).
    fn add(&mut self, format: &[u8], mut option: ArgOption<'a>, description: &[u8]) {
        if self.stopped {
            return;
        }
        if self.find_option(format).is_some() && format != b"<SEPARATOR>" {
            let mut message = b"Option \"".to_vec();
            message.extend_from_slice(format);
            message.extend_from_slice(b"\" is multiply defined");
            *self.errmessage.borrow_mut() = message;
            self.stopped = true;
            return;
        }
        option.description = description.to_vec();
        if format.is_empty() || format == b"%*" {
            // Set default global option.
            self.global = Some(self.option.len());
        }
        self.option.push(option);
    }

    /// An entry of `options`: the option `format`, a variable per parameter, its description.
    ///
    /// Port of `ArgParse::options` (argparse.cpp:381-423 @ v2.5.2). A wrong number of variables
    /// is `options`' undefined `va_arg`; the tools' lists are right, which [`parse`](Self::parse)
    /// relies on.
    pub fn option(&mut self, format: &str, params: &[Param<'a>], description: impl AsRef<[u8]>) {
        let mut option = ArgOption::new(format.as_bytes());
        assert_eq!(option.count, params.len(), "{format}");
        option.params = params.to_vec();
        self.add(format.as_bytes(), option, description.as_ref());
    }

    /// The entry `"%*", callback, description` of `options`.
    pub fn sublist(&mut self, callback: Sublist<'a>, description: &str) {
        let mut option = ArgOption::new(b"%*");
        option.callback = Some(callback);
        self.add(b"%*", option, description.as_bytes());
    }

    /// The entry `"<SEPARATOR>", description` of `options`.
    pub fn separator(&mut self, description: &str) {
        let option = ArgOption::new(b"<SEPARATOR>");
        self.add(b"<SEPARATOR>", option, description.as_bytes());
    }

    /// Finds an option by name; the user may mix up one dash and two.
    ///
    /// Port of `ArgParse::find_option` (argparse.cpp:428-446 @ v2.5.2).
    fn find_option(&self, name: &[u8]) -> Option<usize> {
        let at = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(0);
        self.option.iter().position(|option| {
            let opt = option.flag.as_slice();
            if name == opt {
                return true;
            }
            // Match even if the user mixes up one dash or two.
            if at(name, 0) == b'-'
                && at(name, 1) == b'-'
                && at(opt, 0) == b'-'
                && at(opt, 1) != b'-'
                && name[1..] == *opt
            {
                return true;
            }
            at(name, 0) == b'-'
                && at(name, 1) != b'-'
                && at(opt, 0) == b'-'
                && at(opt, 1) == b'-'
                && *name == opt[1..]
        })
    }

    /// Port of `ArgParse::error` (argparse.cpp:460-467 @ v2.5.2), with the message's parts
    /// already put together ([module documentation](self)).
    fn error(&self, parts: &[&[u8]]) -> i32 {
        *self.errmessage.borrow_mut() = parts.concat();
        -1
    }

    /// Parses the command line `argv`, `argv[0]` being the program. Returns 0, or -1 for a
    /// malformed command line, with the message in [`geterror`](Self::geterror).
    ///
    /// Port of `ArgParse::parse` (argparse.cpp:319-363 @ v2.5.2). Its `isalpha` is the "C"
    /// locale's: ASCII letters only, on both platforms.
    pub fn parse(&self, argv: &[Vec<u8>]) -> i32 {
        let mut i = 1;
        while i < argv.len() {
            let arg = argv[i].as_slice();
            let second = arg.get(1).copied().unwrap_or(0);
            if arg.first() == Some(&b'-') && (second.is_ascii_alphabetic() || second == b'-') {
                // Flag.
                let Some(option) = self.find_option(arg) else {
                    return self.error(&[b"Invalid option \"", arg, b"\""]);
                };
                let option = &self.option[option];
                if option.option_type == OptionType::Flag {
                    option.set_parameter(0, None);
                } else {
                    for j in 0..option.count {
                        if j + i + 1 >= argv.len() {
                            return self.error(&[
                                b"Missing parameter ",
                                (j + 1).to_string().as_bytes(),
                                b" from option \"",
                                &option.flag,
                                b"\"",
                            ]);
                        }
                        option.set_parameter(j, Some(&argv[i + j + 1]));
                    }
                    i += option.count;
                }
            } else if let Some(global) = self.global {
                // Not an option nor an option parameter, glob onto global list.
                if let Some(callback) = self.option[global].callback {
                    callback(arg);
                }
            } else {
                return self.error(&[
                    b"Argument \"",
                    arg,
                    b"\" does not have an associated option",
                ]);
            }
            i += 1;
        }
        0
    }

    /// The error of the last [`parse`](Self::parse), which it clears; empty if there was none.
    ///
    /// Port of `ArgParse::geterror` (argparse.cpp:469-475 @ v2.5.2).
    pub fn geterror(&self) -> Vec<u8> {
        std::mem::take(&mut *self.errmessage.borrow_mut())
    }

    /// Prints the usage message to stdout.
    ///
    /// Port of `ArgParse::usage` (argparse.cpp:478-509 @ v2.5.2).
    pub fn usage(&self) {
        const LONGLINE: usize = 40;
        cout(&self.intro);
        cout_char(b'\n');
        let mut maxlen = 0;

        for opt in &self.option {
            let fmtlen = opt.format.len();
            // Option lists > 40 chars will be split into multiple lines.
            if fmtlen < LONGLINE {
                maxlen = maxlen.max(fmtlen);
            }
        }

        for opt in &self.option {
            if !opt.description.is_empty() {
                let fmtlen = opt.format.len();
                if opt.format == b"<SEPARATOR>" {
                    cout(&opt.description);
                    cout_char(b'\n');
                } else if fmtlen < LONGLINE {
                    cout(b"    ");
                    cout(&opt.format);
                    cout(" ".repeat(maxlen + 2 - fmtlen));
                    cout(&opt.description);
                    cout_char(b'\n');
                } else {
                    cout(b"    ");
                    cout(&opt.format);
                    cout(b"\n    ");
                    cout(" ".repeat(maxlen + 2));
                    cout(&opt.description);
                    cout_char(b'\n');
                }
            }
        }
    }
}

impl std::fmt::Debug for ArgParse<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArgParse").finish_non_exhaustive()
    }
}
