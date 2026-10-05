// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Platform helpers: a port of `src/OpenColorIO/Platform.cpp` @ v2.5.2.
//!
//! **The environment** goes through an injectable [`EnvProvider`] (PLAN.md §9), so tests can
//! give OCIO an environment without touching the process's. Names and values are bytes, as
//! OCIO's C strings hold them, and each platform's rules apply:
//! - Windows names are case-insensitive (`GetEnvironmentVariableW`), and setting a variable to
//!   the empty string removes it (`_wputenv_s`); Linux names are exact, and an empty value is a
//!   value.
//! - The Windows wheel reads and writes UTF-16 and converts with `MultiByteToWideChar` and
//!   `WideCharToMultiByte`, which replace what they can't convert with U+FFFD; the Linux wheel
//!   passes the bytes.
//! - [`EnvProvider::entries`] is the process's environment as `environ` (Linux) or the C
//!   runtime's `_wenviron` (Windows, converted to UTF-8) lists it: `NAME=value` entries, in
//!   order. `_wenviron` leaves out the entries the process block hides behind a leading `=`
//!   (`=C:=C:\dir`, `=ExitCode=...`), as the wheel shows.
//!
//! **The process environment** ([`ProcessEnv`]) is read through `std::env`. Rust 2024 makes
//! changing it `unsafe`, which this crate forbids, so what OCIO sets and unsets
//! (`SetEnvVariable`, `UnsetEnvVariable`) lands in an overlay that OCIO reads on top of the
//! process environment, and that other code in the process doesn't see (deviation D-5,
//! `docs/deviations.md`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::exception::{Exception, Result};
use crate::utils::string_utils::c_str;

/// Where OCIO reads and writes environment variables.
pub trait EnvProvider: Send + Sync {
    /// The value of `name` (non-empty, no NUL), or `None` if it is not set. A variable set to
    /// the empty string is `Some("")`.
    fn var(&self, name: &[u8]) -> Option<Vec<u8>>;

    /// The environment's entries, `NAME=value`, in the order the C runtime lists them.
    fn entries(&self) -> Vec<Vec<u8>>;

    /// Sets `name` (non-empty, no NUL) to `value` (no NUL), as the platform's C runtime does.
    fn set_var(&self, name: &[u8], value: &[u8]);

    /// Removes `name` (non-empty, no NUL).
    fn remove_var(&self, name: &[u8]);
}

/// Whether two names are the same variable: ignoring case on Windows, where
/// `GetEnvironmentVariableW` and `_wputenv_s` fold it; exactly on Linux. The fold is ASCII's
/// here; Windows folds every letter of the Unicode basic plane, which only [`ProcessEnv`] does
/// (through the system).
fn same_name(a: &[u8], b: &[u8]) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Whether `name` is one the C runtime accepts for setting: `_wputenv_s` and `setenv` both
/// refuse a name holding `=` (`EINVAL`), and OCIO ignores the error.
fn settable(name: &[u8]) -> bool {
    !name.contains(&b'=')
}

/// A list of `NAME=value` entries, kept as the C runtime keeps `environ`: setting an existing
/// variable replaces its entry in place (renamed as given), a new one is appended, and removing
/// one takes its entry out.
#[derive(Debug, Default, Clone)]
struct Entries(Vec<(Vec<u8>, Vec<u8>)>);

impl Entries {
    fn get(&self, name: &[u8]) -> Option<Vec<u8>> {
        self.0
            .iter()
            .find(|(n, _)| same_name(n, name))
            .map(|(_, v)| v.clone())
    }

    fn set(&mut self, name: &[u8], value: &[u8]) {
        if cfg!(windows) && value.is_empty() {
            self.remove(name);
            return;
        }
        match self.0.iter_mut().find(|(n, _)| same_name(n, name)) {
            Some(entry) => *entry = (name.to_vec(), value.to_vec()),
            None => self.0.push((name.to_vec(), value.to_vec())),
        }
    }

    fn remove(&mut self, name: &[u8]) {
        self.0.retain(|(n, _)| !same_name(n, name));
    }

    fn lines(&self) -> Vec<Vec<u8>> {
        self.0
            .iter()
            .map(|(n, v)| {
                let mut line = n.clone();
                line.push(b'=');
                line.extend_from_slice(v);
                line
            })
            .collect()
    }
}

/// The process environment, read through `std::env`, with OCIO's own changes on top.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnv;

/// A change OCIO made: a name, and its value or `None` when removed.
type Change = (Vec<u8>, Option<Vec<u8>>);

/// OCIO's changes to the process environment, one per name, in the order OCIO made them.
static OVERLAY: RwLock<Vec<Change>> = RwLock::new(Vec::new());

impl ProcessEnv {
    fn overlay(name: &[u8]) -> Option<Option<Vec<u8>>> {
        OVERLAY
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .rev()
            .find(|(n, _)| same_name(n, name))
            .map(|(_, v)| v.clone())
    }

    fn record(name: &[u8], value: Option<Vec<u8>>) {
        let mut overlay = OVERLAY.write().unwrap_or_else(|e| e.into_inner());
        overlay.retain(|(n, _)| !same_name(n, name));
        overlay.push((name.to_vec(), value));
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

    /// Whether the C runtime's `_wenviron` lists the entry: not the ones the process block
    /// hides behind a leading `=`.
    pub(super) fn listed(name: &OsStr) -> bool {
        name.encode_wide().next() != Some(u16::from(b'='))
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

    pub(super) fn listed(_: &OsStr) -> bool {
        true
    }
}

impl EnvProvider for ProcessEnv {
    fn var(&self, name: &[u8]) -> Option<Vec<u8>> {
        if let Some(value) = Self::overlay(name) {
            return value;
        }
        // std::env::var_os asks GetEnvironmentVariableW (Windows) or getenv (Linux), as
        // upstream does, with the same name.
        std::env::var_os(process::os(name)).map(|v| process::bytes(&v))
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        let mut entries = Entries(
            std::env::vars_os()
                .filter(|(n, _)| process::listed(n))
                .map(|(n, v)| (process::bytes(&n), process::bytes(&v)))
                .collect(),
        );
        for (name, value) in OVERLAY.read().unwrap_or_else(|e| e.into_inner()).iter() {
            match value {
                Some(value) => entries.set(name, value),
                None => entries.remove(name),
            }
        }
        entries.lines()
    }

    fn set_var(&self, name: &[u8], value: &[u8]) {
        if cfg!(windows) && value.is_empty() {
            Self::record(name, None);
        } else {
            Self::record(name, Some(value.to_vec()));
        }
    }

    fn remove_var(&self, name: &[u8]) {
        Self::record(name, None);
    }
}

/// A fixed environment, for tests: entries in order, with the platform's rules for names and
/// empty values.
#[derive(Debug, Default)]
pub struct MapEnv(RwLock<Entries>);

impl MapEnv {
    /// An environment of these `(name, value)` entries, in order.
    pub fn from_entries<N: AsRef<[u8]>, V: AsRef<[u8]>>(entries: &[(N, V)]) -> MapEnv {
        MapEnv(RwLock::new(Entries(
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
        self.0.read().unwrap_or_else(|e| e.into_inner()).get(name)
    }

    fn entries(&self) -> Vec<Vec<u8>> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).lines()
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

/// Port of `Platform::Setenv` (Platform.cpp:99-121 @ v2.5.2): `_wputenv_s` on Windows, which
/// removes the variable for an empty value, and `setenv` on Linux, which keeps it empty. An
/// empty name, or one the C runtime refuses (holding `=`), changes nothing.
pub fn setenv(name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
    let name = c_str(name.as_ref());
    if name.is_empty() || !settable(name) {
        return;
    }
    env_provider().set_var(name, c_str(value.as_ref()));
}

/// Port of `Platform::Unsetenv` (Platform.cpp:123-142 @ v2.5.2).
pub fn unsetenv(name: impl AsRef<[u8]>) {
    let name = c_str(name.as_ref());
    if name.is_empty() || !settable(name) {
        return;
    }
    env_provider().remove_var(name);
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
/// empty string).
///
/// Port of `SetEnvVariable` (src/OpenColorIO/Platform.cpp:30-33 @ v2.5.2).
#[doc(alias = "SetEnvVariable")]
pub fn set_env_variable(name: impl AsRef<[u8]>, value: Option<&[u8]>) {
    setenv(name, value.unwrap_or_default());
}

/// Removes the environment variable `name`.
///
/// Port of `UnsetEnvVariable` (src/OpenColorIO/Platform.cpp:35-38 @ v2.5.2).
#[doc(alias = "UnsetEnvVariable")]
pub fn unset_env_variable(name: impl AsRef<[u8]>) {
    unsetenv(name);
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

/// The `st_dev` the UCRT's `_wstat` gives a file that exists: the number of the drive its full
/// path is on (`A:` is 0), or `-1` as an `unsigned` for a path on no drive (a device such as
/// `nul`, a UNC path). `tests/platform_crt.rs` checks it against `_wstat`.
#[cfg(windows)]
fn ucrt_st_dev(path: &std::path::Path) -> u32 {
    let full = std::path::absolute(path).unwrap_or_default();
    let wide: Vec<u16> = std::os::windows::ffi::OsStrExt::encode_wide(full.as_os_str()).collect();
    match wide.as_slice() {
        [letter, colon, ..] if *colon == u16::from(b':') => {
            match u8::try_from(*letter).map(|c| c.to_ascii_uppercase()) {
                Ok(c @ b'A'..=b'Z') => u32::from(c - b'A'),
                _ => u32::MAX,
            }
        }
        _ => u32::MAX,
    }
}

/// A file's identity, as a proxy for its contents: `st_dev:st_ino` of `stat` on Linux;
/// `st_dev:std::hash(filename)` of `_wstat` on Windows, where `st_ino` means nothing. Empty when
/// the file can't be found. `filename` ends at its first NUL.
///
/// Port of `Platform::CreateFileContentHash` (Platform.cpp:333-357 @ v2.5.2). On Windows,
/// `_wstat` takes the UTF-16 conversion of the name and refuses wildcards (`*`, `?`).
pub fn create_file_content_hash(filename: &[u8]) -> Vec<u8> {
    let filename = c_str(filename);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let wide = utf8_to_utf16_lossy(filename);
        if wide.is_empty()
            || wide
                .iter()
                .any(|&c| c == u16::from(b'*') || c == u16::from(b'?'))
        {
            return Vec::new();
        }
        let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&wide));
        if std::fs::metadata(&path).is_err() {
            return Vec::new();
        }
        // Treat the st_dev (i.e. device) + st_ino (i.e. inode) as a proxy for the contents.
        // The hard-linked files are then not correctly supported on Windows.
        format!("{}:{}", ucrt_st_dev(&path), msvc_std_hash(filename)).into_bytes()
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
