// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Platform helpers: a port of `src/OpenColorIO/Platform.cpp` @ v2.5.2.
//!
//! **The environment** goes through an injectable [`EnvProvider`] (PLAN.md §9), so tests can
//! give OCIO an environment without touching the process's. Names and values are bytes, as
//! OCIO's C strings hold them, and each platform's rules apply (`EnvState`, docs/improvements.md
//! I-113 and I-117): the C runtime's rules for setting a variable (`setenv`, `_wputenv_s`), the
//! system's for reading one (`getenv`, `GetEnvironmentVariableW`, which on Windows folds names
//! by the system's table), and the C runtime's list for the whole environment (`environ`,
//! `_wenviron`, which folds names by ASCII and leaves out the system's `=C:` entries). The
//! Windows wheel reads and writes UTF-16 and converts with `MultiByteToWideChar` and
//! `WideCharToMultiByte`, which replace what they can't convert with U+FFFD; the Linux wheel
//! passes the bytes.
//!
//! **The process environment** ([`ProcessEnv`]) is read through `std::env`. Rust 2024 makes
//! changing it `unsafe`, which this crate forbids, so what OCIO sets and unsets
//! (`SetEnvVariable`, `UnsetEnvVariable`) is recorded and replayed on the process environment
//! when OCIO reads it, and other code in the process doesn't see it (deviation D-5,
//! `docs/deviations.md`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::exception::{Exception, Result};
use crate::utils::string_utils::c_str;

#[path = "platform_nls_upcase.rs"]
mod nls_upcase;

/// `RtlUpcaseUnicodeChar(c)`, as Windows folds environment variable names (the reference
/// machine's table, docs/improvements.md I-117). For `tests/platform_crt.rs`.
#[doc(hidden)]
pub fn windows_upcase(c: u16) -> u16 {
    nls_upcase::upcase(c)
}

/// Where OCIO reads and writes environment variables. [`setenv`] and [`unsetenv`] apply the
/// C runtime's rules to their arguments first (a name holding `=`, Windows' empty value), so a
/// provider receives a variable's final name and value.
pub trait EnvProvider: Send + Sync {
    /// The value the system gives for `name` (non-empty, no NUL): `getenv` (Linux) or
    /// `GetEnvironmentVariableW` (Windows), or `None` if it has none. A variable set to the
    /// empty string is `Some("")`.
    fn var(&self, name: &[u8]) -> Option<Vec<u8>>;

    /// The environment's entries, `NAME=value`, as the C runtime lists them: `environ` (Linux)
    /// or `_wenviron` (Windows, converted to UTF-8).
    fn entries(&self) -> Vec<Vec<u8>>;

    /// Sets the variable `name` (non-empty, no NUL, no `=`) to `value` (no NUL; on Windows not
    /// empty), as the C runtime's `setenv` or `_wputenv_s` stores it.
    fn set_var(&self, name: &[u8], value: &[u8]);

    /// Removes the variable `name` (non-empty, no NUL, no `=`), as the C runtime's `unsetenv` or
    /// `_wputenv_s` with an empty value does.
    fn remove_var(&self, name: &[u8]);
}

/// A variable: its name (the entry up to its first `=` after the first character) and value.
type Var = (Vec<u8>, Vec<u8>);

/// `name=value`.
fn line((name, value): &Var) -> Vec<u8> {
    [name.as_slice(), b"=", value].concat()
}

/// An environment as the platform keeps it, and its rules.
///
/// - Linux: one list, `environ`. `setenv` replaces the first entry of the name in place (and
///   its name) or appends one; `unsetenv` removes every entry of the name; `getenv(name)` is
///   the first entry that starts with `name` followed by `=`. Names compare exactly.
/// - Windows: two lists. The C runtime's `_wenviron`, which `LoadEnvironment` reads, compares
///   names ignoring ASCII case only (`_wcsnicoll` in the "C" locale), replaces the first entry
///   of the name in place (with the new name) or appends one, removes the first entry of the
///   name, and leaves out the entries whose name starts with `=` (`=C:`). The system's block,
///   which `GetEnvironmentVariableW` reads, compares names as `RtlUpcaseUnicodeChar` folds
///   them, code unit by code unit ([`nls_upcase`]), and gives for `name` the entry that starts
///   with it (folded) followed by `=`. Everything is UTF-16 there: names and values reach it
///   through `Utf8ToUtf16` and come back through `Utf16ToUtf8`, so bytes that aren't UTF-8 come
///   back as U+FFFD.
#[derive(Debug, Default, Clone)]
struct EnvState {
    /// `environ` (Linux), `_wenviron` (Windows).
    crt: Vec<Var>,
    /// The system's block (Windows); unused on Linux.
    os: Vec<Var>,
}

impl EnvState {
    /// An environment holding `vars`, the system's block in its order.
    fn new(vars: Vec<Var>) -> EnvState {
        if cfg!(windows) {
            let vars: Vec<Var> = vars
                .into_iter()
                .map(|(n, v)| (round_trip(&n), round_trip(&v)))
                .collect();
            EnvState {
                crt: vars
                    .iter()
                    .filter(|(n, _)| n.first() != Some(&b'='))
                    .cloned()
                    .collect(),
                os: vars,
            }
        } else {
            EnvState {
                crt: vars,
                os: Vec::new(),
            }
        }
    }

    fn set(&mut self, name: &[u8], value: &[u8]) {
        let var = (name.to_vec(), value.to_vec());
        match self.crt.iter_mut().find(|(n, _)| same_crt_name(n, name)) {
            Some(entry) => *entry = var.clone(),
            None => self.crt.push(var.clone()),
        }
        if cfg!(windows) {
            match self.os.iter_mut().find(|(n, _)| same_os_name(n, name)) {
                Some(entry) => entry.1 = var.1,
                None => self.os.push(var),
            }
        }
    }

    fn remove(&mut self, name: &[u8]) {
        if cfg!(windows) {
            if let Some(i) = self.crt.iter().position(|(n, _)| same_crt_name(n, name)) {
                self.crt.remove(i);
            }
            self.os.retain(|(n, _)| !same_os_name(n, name));
        } else {
            self.crt.retain(|(n, _)| n != name);
        }
    }

    fn var(&self, name: &[u8]) -> Option<Vec<u8>> {
        if cfg!(windows) {
            let name = utf8_to_utf16_lossy(name);
            self.os.iter().find_map(|var| {
                let entry = utf8_to_utf16_lossy(&line(var));
                (entry.len() > name.len()
                    && entry[name.len()] == u16::from(b'=')
                    && nls_upcase::equal_ignoring_case(&entry[..name.len()], &name))
                .then(|| utf16_to_utf8_lossy(&entry[name.len() + 1..]))
            })
        } else {
            self.crt.iter().find_map(|var| {
                let entry = line(var);
                (entry.len() > name.len()
                    && entry[name.len()] == b'='
                    && &entry[..name.len()] == name)
                    .then(|| entry[name.len() + 1..].to_vec())
            })
        }
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        self.crt.iter().map(line).collect()
    }
}

/// UTF-8 bytes as they come back from the system's UTF-16: `Utf16ToUtf8(Utf8ToUtf16(s))`.
fn round_trip(s: &[u8]) -> Vec<u8> {
    utf16_to_utf8_lossy(&utf8_to_utf16_lossy(s))
}

/// Whether the C runtime's list takes `a` and `b` for the same name: exactly on Linux,
/// ignoring ASCII case on Windows (`_wcsnicoll` in the "C" locale).
fn same_crt_name(a: &[u8], b: &[u8]) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Whether Windows' block takes `a` and `b` for the same name (`RtlUpcaseUnicodeChar`).
fn same_os_name(a: &[u8], b: &[u8]) -> bool {
    nls_upcase::equal_ignoring_case(&utf8_to_utf16_lossy(a), &utf8_to_utf16_lossy(b))
}

/// The process's environment, as `std::env` reads it.
fn process_vars() -> Vec<Var> {
    std::env::vars_os()
        .map(|(n, v)| (process::bytes(&n), process::bytes(&v)))
        .collect()
}

/// The process environment, read through `std::env`, with OCIO's own changes on top: what OCIO
/// sets and unsets is recorded, and replayed on the process's environment when OCIO reads it
/// (deviation D-5).
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnv;

/// A change OCIO made: a name, and its value or `None` when removed.
type Change = (Vec<u8>, Option<Vec<u8>>);

/// OCIO's changes to the process environment, in the order OCIO made them.
static OVERLAY: RwLock<Vec<Change>> = RwLock::new(Vec::new());

impl ProcessEnv {
    /// The process's environment with OCIO's changes, or `None` when OCIO changed nothing.
    fn changed() -> Option<EnvState> {
        let overlay = OVERLAY.read().unwrap_or_else(|e| e.into_inner());
        if overlay.is_empty() {
            return None;
        }
        let mut state = EnvState::new(process_vars());
        for (name, value) in overlay.iter() {
            match value {
                Some(value) => state.set(name, value),
                None => state.remove(name),
            }
        }
        Some(state)
    }

    fn record(name: &[u8], value: Option<Vec<u8>>) {
        OVERLAY
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push((name.to_vec(), value));
    }
}

#[cfg(windows)]
mod process {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    /// A UTF-8 name, as `Utf8ToUtf16` passes it to the system.
    pub(super) fn os(name: &[u8]) -> OsString {
        OsString::from_wide(&super::utf8_to_utf16_lossy(name))
    }

    /// A system string, as `Utf16ToUtf8` gives it to OCIO.
    pub(super) fn bytes(s: &OsStr) -> Vec<u8> {
        super::utf16_to_utf8_lossy(&s.encode_wide().collect::<Vec<u16>>())
    }
}

#[cfg(not(windows))]
mod process {
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    pub(super) fn os(name: &[u8]) -> OsString {
        OsString::from_vec(name.to_vec())
    }

    pub(super) fn bytes(s: &OsStr) -> Vec<u8> {
        s.as_bytes().to_vec()
    }
}

impl EnvProvider for ProcessEnv {
    fn var(&self, name: &[u8]) -> Option<Vec<u8>> {
        if let Some(state) = Self::changed() {
            return state.var(name);
        }
        // std::env::var_os asks GetEnvironmentVariableW (Windows) or getenv (Linux), as
        // upstream does, with the same name.
        std::env::var_os(process::os(name)).map(|v| process::bytes(&v))
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        Self::changed()
            .unwrap_or_else(|| EnvState::new(process_vars()))
            .entries()
    }

    fn set_var(&self, name: &[u8], value: &[u8]) {
        Self::record(name, Some(value.to_vec()));
    }

    fn remove_var(&self, name: &[u8]) {
        Self::record(name, None);
    }
}

/// A fixed environment, for tests, with the platform's rules (as the process's environment
/// has them).
#[derive(Debug, Default)]
pub struct MapEnv(RwLock<EnvState>);

impl MapEnv {
    /// An environment of these `(name, value)` entries, in order, as the system's block would
    /// hold them (on Windows, through UTF-16).
    pub fn from_entries<N: AsRef<[u8]>, V: AsRef<[u8]>>(entries: &[(N, V)]) -> MapEnv {
        MapEnv(RwLock::new(EnvState::new(
            entries
                .iter()
                .map(|(n, v)| (n.as_ref().to_vec(), v.as_ref().to_vec()))
                .collect(),
        )))
    }
}

impl From<BTreeMap<String, String>> for MapEnv {
    fn from(map: BTreeMap<String, String>) -> MapEnv {
        let entries: Vec<(String, String)> = map.into_iter().collect();
        MapEnv::from_entries(&entries)
    }
}

impl EnvProvider for MapEnv {
    fn var(&self, name: &[u8]) -> Option<Vec<u8>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).var(name)
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).entries()
    }

    fn set_var(&self, name: &[u8], value: &[u8]) {
        self.0
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .set(name, value);
    }

    fn remove_var(&self, name: &[u8]) {
        self.0
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
    }
}

static ENV: RwLock<Option<Arc<dyn EnvProvider>>> = RwLock::new(None);

/// Replaces the environment OCIO reads (`None` restores the process environment).
pub fn set_env_provider(provider: Option<Arc<dyn EnvProvider>>) {
    *ENV.write().unwrap_or_else(|e| e.into_inner()) = provider;
}

/// The environment OCIO reads.
pub fn env_provider() -> Arc<dyn EnvProvider> {
    ENV.read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| Arc::new(ProcessEnv))
}

/// Port of `Platform::Getenv` (src/OpenColorIO/Platform.cpp:48-97 @ v2.5.2): `None` when
/// `name` is empty or the variable does not exist; otherwise its value, which may be empty.
/// `name` ends at its first NUL, as the C string upstream takes.
pub fn getenv(name: impl AsRef<[u8]>) -> Option<Vec<u8>> {
    let name = c_str(name.as_ref());
    if name.is_empty() {
        return None;
    }
    env_provider().var(name)
}

/// The UCRT's `_MAX_ENV`: `_wputenv_s` refuses a name or a value of this many UTF-16 code
/// units or more by its parameter validation, which ends the process.
#[cfg(windows)]
const MAX_ENV: usize = 32_767;

/// What `Setenv(name, value)` (`value` given) and `Unsetenv(name)` do to the environment.
/// Arguments end at their first NUL; an empty name changes nothing.
///
/// Windows (`_wputenv_s(Utf8ToUtf16(name), Utf8ToUtf16(value))`, Unsetenv with `L""`): the C
/// runtime builds `name=value` and splits it at its first `=`, so a name holding `=` sets the
/// variable named by its part before the `=` (`A=B`, `C` sets `A` to `B=C`); a name that starts
/// with `=` is refused; an empty value after the split removes the variable. A name or a value
/// of `_MAX_ENV` UTF-16 code units or more ends the wheel's process: the port returns an error
/// (docs/improvements.md, U-46). Linux (`setenv`, `unsetenv`): a name holding `=` is refused.
fn put(name: &[u8], value: Option<&[u8]>) -> Result<()> {
    let name = c_str(name);
    if name.is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let wname = utf8_to_utf16_lossy(name);
        let wvalue = utf8_to_utf16_lossy(value.map_or(&[][..], c_str));
        if wname.len() >= MAX_ENV || wvalue.len() >= MAX_ENV {
            return Err(Exception::new(format!(
                "Environment variable names and values must be shorter than {MAX_ENV} UTF-16 \
                 code units (_MAX_ENV)."
            )));
        }
        let option = [wname.as_slice(), &[u16::from(b'=')], &wvalue].concat();
        let equal = option
            .iter()
            .position(|&c| c == u16::from(b'='))
            .unwrap_or(0);
        if equal == 0 {
            // EINVAL, which OCIO ignores.
            return Ok(());
        }
        let name = utf16_to_utf8_lossy(&option[..equal]);
        let value = utf16_to_utf8_lossy(&option[equal + 1..]);
        if value.is_empty() {
            env_provider().remove_var(&name);
        } else {
            env_provider().set_var(&name, &value);
        }
    }
    #[cfg(not(windows))]
    {
        if name.contains(&b'=') {
            // EINVAL, which OCIO ignores.
            return Ok(());
        }
        match value {
            Some(value) => env_provider().set_var(name, c_str(value)),
            None => env_provider().remove_var(name),
        }
    }
    Ok(())
}

/// Port of `Platform::Setenv` (Platform.cpp:99-121 @ v2.5.2): `_wputenv_s` on Windows, which
/// removes the variable for an empty value, and `setenv` on Linux, which keeps it empty; each
/// C runtime's rules as `put` describes them, and an error where the Windows wheel's process
/// ends.
pub fn setenv(name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) -> Result<()> {
    put(name.as_ref(), Some(value.as_ref()))
}

/// Port of `Platform::Unsetenv` (Platform.cpp:123-142 @ v2.5.2): `_wputenv_s(name, L"")` on
/// Windows (so `Q=R` sets `Q` to `R=`), `unsetenv` on Linux.
pub fn unsetenv(name: impl AsRef<[u8]>) -> Result<()> {
    if cfg!(windows) {
        put(name.as_ref(), Some(b""))
    } else {
        put(name.as_ref(), None)
    }
}

/// Port of `Platform::isEnvPresent` (Platform.cpp:144-153 @ v2.5.2).
pub fn is_env_present(name: impl AsRef<[u8]>) -> bool {
    getenv(name).is_some()
}

/// The value of the environment variable `name`, or empty if it isn't set.
///
/// Port of `GetEnvVariable` (src/OpenColorIO/Platform.cpp:23-28 @ v2.5.2).
#[doc(alias = "GetEnvVariable")]
pub fn get_env_variable(name: impl AsRef<[u8]>) -> Vec<u8> {
    getenv(name).unwrap_or_default()
}

/// Sets the environment variable `name` to `value` (`None` is upstream's null pointer, the
/// empty string). Errors where the Windows wheel's process ends ([`setenv`]).
///
/// Port of `SetEnvVariable` (src/OpenColorIO/Platform.cpp:30-33 @ v2.5.2).
#[doc(alias = "SetEnvVariable")]
pub fn set_env_variable(name: impl AsRef<[u8]>, value: Option<&[u8]>) -> Result<()> {
    setenv(name, value.unwrap_or_default())
}

/// Removes the environment variable `name`. Errors where the Windows wheel's process ends
/// ([`unsetenv`]).
///
/// Port of `UnsetEnvVariable` (src/OpenColorIO/Platform.cpp:35-38 @ v2.5.2).
#[doc(alias = "UnsetEnvVariable")]
pub fn unset_env_variable(name: impl AsRef<[u8]>) -> Result<()> {
    unsetenv(name)
}

/// Whether the environment variable `name` exists (its value may be empty).
///
/// Port of `IsEnvVariablePresent` (src/OpenColorIO/Platform.cpp:40-43 @ v2.5.2).
#[doc(alias = "IsEnvVariablePresent")]
pub fn is_env_variable_present(name: impl AsRef<[u8]>) -> bool {
    is_env_present(name)
}

/// Port of `Platform::Strcasecmp` (Platform.cpp:155-167 @ v2.5.2): `_stricmp` / `strcasecmp`
/// in the classic "C" locale, i.e. bytes compared as unsigned after folding `A`-`Z` to
/// lowercase. The C functions take `const char *`, so each side ends at its first NUL.
/// (Upstream throws for a null pointer; a slice is never null.)
///
/// Deviation D-4 (`docs/deviations.md`): upstream folds case by the process's C locale, and
/// Python sets that locale at startup, so under Python the Windows wheel also folds non-ASCII
/// bytes by the ANSI code page. The port folds `A`-`Z` only, whatever the locale.
/// `tests/platform_crt.rs` checks this against the C runtime in the "C" locale.
pub fn strcasecmp(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Ordering {
    strncasecmp(a, b, usize::MAX)
}

/// Port of `Platform::Strncasecmp` (Platform.cpp:169-181 @ v2.5.2): [`strcasecmp`] on at
/// most the first `n` bytes of each side, as `_strnicmp` / `strncasecmp` in the classic "C"
/// locale (deviation D-4).
pub fn strncasecmp(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>, n: usize) -> Ordering {
    let lower = |s: &[u8]| {
        c_str(s)
            .iter()
            .take(n)
            .map(|c| c.to_ascii_lowercase())
            .collect::<Vec<u8>>()
    };
    lower(a.as_ref()).cmp(&lower(b.as_ref()))
}

/// UTF-8 bytes as `MultiByteToWideChar(CP_UTF8, 0, ...)` converts them: what isn't UTF-8
/// becomes U+FFFD. Windows' replacement differs from Rust's (`from_utf8_lossy`, the Unicode
/// "maximal subpart" practice) in one way: a lead byte followed by a continuation byte outside
/// the lead's range (`E0 80`, `ED A0`, `F0 8F`, `F4 90`) is one replacement for both bytes,
/// where Rust replaces the lead alone. `tests/platform_crt.rs` checks this against the system.
pub fn utf8_to_utf16_lossy(s: &[u8]) -> Vec<u16> {
    const REPLACEMENT: u16 = 0xFFFD;
    let is_cont = |c: u8| (0x80..=0xBF).contains(&c);
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c < 0x80 {
            out.push(u16::from(c));
            i += 1;
            continue;
        }
        // The continuation bytes the lead needs, and the range of the first one.
        let (need, lo, hi) = match c {
            0xC2..=0xDF => (1, 0x80, 0xBF),
            0xE0 => (2, 0xA0, 0xBF),
            0xED => (2, 0x80, 0x9F),
            0xE1..=0xEF => (2, 0x80, 0xBF),
            0xF0 => (3, 0x90, 0xBF),
            0xF4 => (3, 0x80, 0x8F),
            0xF1..=0xF3 => (3, 0x80, 0xBF),
            _ => {
                out.push(REPLACEMENT);
                i += 1;
                continue;
            }
        };
        match s.get(i + 1) {
            Some(&second) if is_cont(second) => {
                if !(lo..=hi).contains(&second) {
                    out.push(REPLACEMENT);
                    i += 2;
                    continue;
                }
            }
            _ => {
                out.push(REPLACEMENT);
                i += 1;
                continue;
            }
        }
        let mut end = i + 2;
        let mut got = 1;
        while got < need && end < s.len() && is_cont(s[end]) {
            end += 1;
            got += 1;
        }
        if got < need {
            out.push(REPLACEMENT);
        } else {
            let text = std::str::from_utf8(&s[i..end]).expect("a valid sequence");
            out.extend(text.encode_utf16());
        }
        i = end;
    }
    out
}

/// UTF-16 as `WideCharToMultiByte(CP_UTF8, 0, ...)` converts it: an unpaired surrogate
/// becomes U+FFFD.
pub fn utf16_to_utf8_lossy(s: &[u16]) -> Vec<u8> {
    String::from_utf16_lossy(s).into_bytes()
}

/// Port of `Platform::Utf8ToUtf16` (Platform.cpp:292-306 @ v2.5.2): Windows only; elsewhere
/// it throws for a non-empty string.
pub fn utf8_to_utf16(s: &[u8]) -> Result<Vec<u16>> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    if cfg!(windows) {
        Ok(utf8_to_utf16_lossy(s))
    } else {
        Err(Exception::new("Only supported by the Windows platform."))
    }
}

/// Port of `Platform::Utf16ToUtf8` (Platform.cpp:308-322 @ v2.5.2): Windows only; elsewhere
/// it throws for a non-empty string.
pub fn utf16_to_utf8(s: &[u16]) -> Result<Vec<u8>> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    if cfg!(windows) {
        Ok(utf16_to_utf8_lossy(s))
    } else {
        Err(Exception::new("Only supported by the Windows platform."))
    }
}

/// MSVC's `std::hash<std::string>`: `_Fnv1a_append_bytes` from `_FNV_offset_basis`
/// (`<type_traits>`, the 64-bit constants) over the string's bytes.
#[cfg(windows)]
fn msvc_std_hash(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
    const PRIME: u64 = 1_099_511_628_211;
    bytes.iter().fold(OFFSET_BASIS, |value, &b| {
        (value ^ u64::from(b)).wrapping_mul(PRIME)
    })
}

/// `_getdrive()`: the number of the current directory's drive (`A:` is 1), or 0 when the
/// current directory is on no drive (a UNC path) or can't be read.
#[cfg(windows)]
fn current_drive_number() -> u32 {
    use std::os::windows::ffi::OsStrExt;
    let Ok(dir) = std::env::current_dir() else {
        return 0;
    };
    let wide: Vec<u16> = dir.as_os_str().encode_wide().take(2).collect();
    match wide.as_slice() {
        [letter, colon] if *colon == u16::from(b':') => match u8::try_from(*letter) {
            Ok(c) if c.is_ascii_alphabetic() => u32::from(c.to_ascii_uppercase() - b'A') + 1,
            _ => 0,
        },
        _ => 0,
    }
}

/// The drive number the UCRT's `_wstat` gives a file on a disk (`A:` is 1): the drive letter
/// the path starts with, else the current drive (0 when there is none); `None` for a path that
/// is a drive letter and a colon alone, which `_wstat` refuses. `tests/platform_crt.rs` checks
/// it against `_wstat`.
#[cfg(windows)]
fn ucrt_drive_number(path: &[u16]) -> Option<u32> {
    match path {
        [letter, colon, rest @ ..] if *colon == u16::from(b':') => match u8::try_from(*letter) {
            Ok(c) if c.is_ascii_alphabetic() => {
                if rest.is_empty() {
                    None
                } else {
                    Some(u32::from(c.to_ascii_lowercase() - b'a') + 1)
                }
            }
            _ => Some(current_drive_number()),
        },
        _ => Some(current_drive_number()),
    }
}

/// `_wstat`'s `st_dev` for `path`, or `None` when `_wstat` fails, as the UCRT computes it: it
/// opens the path for its attributes (`CreateFileW` with `FILE_READ_ATTRIBUTES`, every share
/// mode, `FILE_FLAG_BACKUP_SEMANTICS`) and fails when that fails; a device or a pipe (whose
/// file information can't be read) has `st_dev` -1, a file on a disk its drive number minus
/// one ([`ucrt_drive_number`]). No wildcard is refused: `\?\` paths open. `tests/platform_crt.rs`
/// checks it against `_wstat`.
///
/// Not reproduced: the UCRT's fallback for a root that `CreateFileW` can't open
/// (`C:\`, `\server\share\`, fabricated from `GetDriveTypeW`), which opens on the systems the
/// wheels run on; and a disk file whose information can't be read, taken here for a device.
#[cfg(windows)]
fn ucrt_wstat_dev(path: &[u16]) -> Option<u32> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x7;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    if path.is_empty() {
        return None;
    }
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(std::ffi::OsString::from_wide(path))
        .ok()?;
    if file.metadata().is_err() {
        // A character device or a pipe: `st_dev` is the file handle, -1.
        return Some(u32::MAX);
    }
    ucrt_drive_number(path).map(|drive| drive.wrapping_sub(1))
}

/// A file's identity, as a proxy for its contents: `st_dev:st_ino` of `stat` on Linux;
/// `st_dev:std::hash(filename)` of `_wstat` on Windows, where `st_ino` means nothing. Empty when
/// the file can't be found. `filename` ends at its first NUL.
///
/// Port of `Platform::CreateFileContentHash` (Platform.cpp:333-357 @ v2.5.2). On Windows,
/// `_wstat` takes the UTF-16 conversion of the name ([`ucrt_wstat_dev`]).
pub fn create_file_content_hash(filename: &[u8]) -> Vec<u8> {
    let filename = c_str(filename);
    #[cfg(windows)]
    {
        let Some(st_dev) = ucrt_wstat_dev(&utf8_to_utf16_lossy(filename)) else {
            return Vec::new();
        };
        // Treat the st_dev (i.e. device) + st_ino (i.e. inode) as a proxy for the contents.
        // The hard-linked files are then not correctly supported on Windows.
        format!("{st_dev}:{}", msvc_std_hash(filename)).into_bytes()
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::MetadataExt;
        if filename.is_empty() {
            return Vec::new();
        }
        match std::fs::metadata(std::ffi::OsStr::from_bytes(filename)) {
            Ok(info) => format!("{}:{}", info.dev(), info.ino()).into_bytes(),
            Err(_) => Vec::new(),
        }
    }
}

/// Opens `filename` for reading: through its UTF-16 conversion on Windows, as the bytes on
/// Linux. `filename` ends at its first NUL.
///
/// Port of `Platform::CreateInputFileStream` (Platform.cpp:261-268 @ v2.5.2), which opens an
/// `std::ifstream` (the mode is the caller's business here: Rust reads bytes).
pub fn create_input_file_stream(filename: &[u8]) -> std::io::Result<std::fs::File> {
    let filename = c_str(filename);
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&utf8_to_utf16_lossy(filename))
    };
    #[cfg(not(windows))]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(filename).to_os_string()
    };
    std::fs::File::open(path)
}

/// The numbers of temporary file names: a process-wide `std::mt19937` with its default seed,
/// whose outputs are kept to the 31 bits of a non-negative `int`.
///
/// Upstream draws them with `std::uniform_int_distribution<int>{}` over the engine
/// (`GenerateRandomNumber`, Platform.cpp:210-218 @ v2.5.2), whose algorithm the C++ standard
/// leaves to the library, so the names differ from the Linux wheel's (docs/improvements.md,
/// I-112). Only tests use them.
fn generate_random_number() -> u32 {
    /// `std::mt19937` (C++ [rand.predef]: w=32, n=624, m=397, r=31, a=0x9908b0df, u=11,
    /// d=0xffffffff, s=7, b=0x9d2c5680, t=15, c=0xefc60000, l=18, f=1812433253, seed 5489).
    struct Mt19937 {
        state: [u32; 624],
        index: usize,
    }
    impl Mt19937 {
        fn new(seed: u32) -> Mt19937 {
            let mut state = [0u32; 624];
            state[0] = seed;
            for i in 1..624 {
                let prev = state[i - 1];
                state[i] = 1_812_433_253u32
                    .wrapping_mul(prev ^ (prev >> 30))
                    .wrapping_add(u32::try_from(i).expect("an index"));
            }
            Mt19937 { state, index: 624 }
        }
        fn next(&mut self) -> u32 {
            if self.index == 624 {
                for i in 0..624 {
                    let y =
                        (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % 624] & 0x7fff_ffff);
                    let mut v = self.state[(i + 397) % 624] ^ (y >> 1);
                    if y & 1 != 0 {
                        v ^= 0x9908_b0df;
                    }
                    self.state[i] = v;
                }
                self.index = 0;
            }
            let mut y = self.state[self.index];
            self.index += 1;
            y ^= y >> 11;
            y ^= (y << 7) & 0x9d2c_5680;
            y ^= (y << 15) & 0xefc6_0000;
            y ^ (y >> 18)
        }
    }
    static ENGINE: std::sync::Mutex<Option<Mt19937>> = std::sync::Mutex::new(None);
    let mut engine = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    engine.get_or_insert_with(|| Mt19937::new(5489)).next() & 0x7fff_ffff
}

/// A temporary file name with the extension `filename_ext` (which may be empty): on Linux
/// `/tmp/ocio_<number>`, on Windows a name in the system's temporary directory, as `tmpnam_s`
/// gives one (`<temp dir>\ocio_<number>` here). Each call gives a new name in practice.
///
/// Port of `Platform::CreateTempFilename` (Platform.cpp:222-259 @ v2.5.2). The names differ
/// from the wheels' (docs/improvements.md, I-112); only tests use them.
pub fn create_temp_filename(filename_ext: &[u8]) -> Vec<u8> {
    let mut filename = if cfg!(windows) {
        let dir = std::env::temp_dir();
        let mut name = dir.to_string_lossy().into_owned().into_bytes();
        if !name.ends_with(b"\\") {
            name.push(b'\\');
        }
        name.extend_from_slice(format!("ocio_{}", generate_random_number()).as_bytes());
        name
    } else {
        // Linux flavors must have a /tmp directory.
        format!("/tmp/ocio_{}", generate_random_number()).into_bytes()
    };
    filename.extend_from_slice(filename_ext);
    filename
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
