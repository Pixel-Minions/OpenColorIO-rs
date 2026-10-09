// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in configs (`src/OpenColorIO/builtinconfigs` @ v2.5.2): their registry, the
//! `ocio://` URIs, and the eight embedded config texts.
//!
//! Upstream's build embeds each `builtinconfigs/configs/*.ocio` file byte for byte, as its
//! checkout wrote it (`src/OpenColorIO/CMakeLists.txt:262-283` @ v2.5.2). The Windows wheel's
//! checkout wrote the files with CR LF line ends and the Linux wheel's with LF, so the texts
//! differ by platform (docs/improvements.md I-148). The port embeds upstream's files from the
//! submodule and gives each line the end of the wheel of its platform, whatever line ends the
//! submodule's checkout has.

pub mod builtin_config_registry;
pub(crate) mod cg;
pub(crate) mod cg_config;
pub(crate) mod studio;
pub(crate) mod studio_config;

/// Whether the wheel of this platform ends the lines of a built-in config's text with CR LF.
const CRLF_LINE_ENDS: bool = cfg!(windows);

/// The length of the embedded text of `source`, a config file as a checkout wrote it: each line
/// ends with CR LF on Windows and LF elsewhere.
pub(crate) const fn embedded_len(source: &[u8]) -> usize {
    let mut len = 0;
    let mut i = 0;
    while i < source.len() {
        if source[i] == b'\r' && i + 1 < source.len() && source[i + 1] == b'\n' {
            // A CR LF line end counts as its LF.
            i += 1;
            continue;
        }
        len += if source[i] == b'\n' && CRLF_LINE_ENDS {
            2
        } else {
            1
        };
        i += 1;
    }
    len
}

/// The embedded text of `source`, of length `N` ([`embedded_len`]): each line ends with CR LF
/// on Windows and LF elsewhere, as in the wheel of the platform.
pub(crate) const fn embedded_text<const N: usize>(source: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    let mut o = 0;
    while i < source.len() {
        if source[i] == b'\r' && i + 1 < source.len() && source[i + 1] == b'\n' {
            i += 1;
            continue;
        }
        if source[i] == b'\n' && CRLF_LINE_ENDS {
            out[o] = b'\r';
            o += 1;
        }
        out[o] = source[i];
        o += 1;
        i += 1;
    }
    out
}

#[cfg(test)]
#[path = "builtin_config_tests.rs"]
mod tests;
