// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The C++ standard streams of the tools: `std::cout`, `std::cerr` and `std::ofstream`.
//!
//! - They are text streams: the C runtime writes each `\n` as `\r\n` on Windows, in the tools'
//!   output and in the files they write, also after a `\r` that is already there. Rust writes
//!   bytes as they are, so this module does the translation ([`text_mode`]).
//! - `std::cout` keeps what it is given until `std::endl`, a flush, or the program's end;
//!   `std::cerr` writes each insertion at once, and flushes `std::cout` first (`cerr.tie()`
//!   is `&cout` on both platforms). What differs is only visible when both go to one pipe:
//!   each stream's own bytes are the same however they are buffered.
//!
//! The library writes to stderr as it is: that is right for the Python module, whose process
//! has set its streams to binary, and wrong for a tool. So a tool starts with [`init`], which
//! sends what the library writes to stderr through `std::cerr` here.
//!
//! An error of a write is ignored, as a C++ stream keeps it in its state bits and the tools
//! don't look.

use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex, PoisonError};

use ocio::{Exception, ExceptionKind, GroupTransform, Transform};
use ocio_ops::logging::set_stderr_sink;

/// How much `std::cout` holds before it writes: only to keep memory small while a bake prints.
const COUT_BUFFER: usize = 1 << 16;

/// What `std::cout` holds, already translated.
static COUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// `bytes` as a text stream writes them to the file: on Windows, `\r\n` for each `\n`.
pub fn text_mode(bytes: &[u8]) -> Cow<'_, [u8]> {
    if !cfg!(windows) || !bytes.contains(&b'\n') {
        return Cow::Borrowed(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
    for &c in bytes {
        if c == b'\n' {
            out.push(b'\r');
        }
        out.push(c);
    }
    Cow::Owned(out)
}

/// A standard stream as a plain file: writes go to the handle as bytes.
///
/// Rust's `Stdout` and `Stderr` turn what goes to a console into UTF-16, and refuse bytes that
/// aren't UTF-8, which loses the rest of the write. The C runtime writes bytes, and the console
/// reads them in its code page. So a tool writes through a duplicate of the handle, as a `File`,
/// whatever the handle is. `None` where the process has no such stream.
#[cfg(windows)]
fn stream_file(stream: &impl std::os::windows::io::AsHandle) -> Option<File> {
    let handle = stream.as_handle().try_clone_to_owned().ok()?;
    Some(File::from(handle))
}

fn write_stdout(bytes: &[u8]) {
    #[cfg(windows)]
    {
        static STDOUT: OnceLock<Option<File>> = OnceLock::new();
        if let Some(mut file) = STDOUT
            .get_or_init(|| stream_file(&std::io::stdout()))
            .as_ref()
        {
            let _ = file.write_all(bytes);
        }
    }
    #[cfg(not(windows))]
    {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(bytes);
        let _ = stdout.flush();
    }
}

fn write_stderr(bytes: &[u8]) {
    #[cfg(windows)]
    {
        static STDERR: OnceLock<Option<File>> = OnceLock::new();
        if let Some(mut file) = STDERR
            .get_or_init(|| stream_file(&std::io::stderr()))
            .as_ref()
        {
            let _ = file.write_all(bytes);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::io::stderr().lock().write_all(bytes);
    }
}

/// `std::cout << text`.
pub fn cout(text: impl AsRef<[u8]>) {
    let mut held = COUT.lock().unwrap_or_else(PoisonError::into_inner);
    held.extend_from_slice(&text_mode(text.as_ref()));
    if held.len() >= COUT_BUFFER {
        write_stdout(&held);
        held.clear();
    }
}

/// `std::cout << c`.
pub fn cout_char(c: u8) {
    cout([c]);
}

/// `std::cout << std::flush`.
pub fn cout_flush() {
    let mut held = COUT.lock().unwrap_or_else(PoisonError::into_inner);
    write_stdout(&held);
    held.clear();
}

/// `std::cout << std::endl`.
pub fn cout_endl() {
    cout(b"\n");
    cout_flush();
}

/// `std::cout << text << std::endl`.
pub fn cout_line(text: impl AsRef<[u8]>) {
    cout(text);
    cout_endl();
}

/// `std::cerr << text`.
pub fn cerr(text: impl AsRef<[u8]>) {
    cout_flush();
    write_stderr(&text_mode(text.as_ref()));
}

/// `std::cerr << text << std::endl`.
pub fn cerr_line(text: impl AsRef<[u8]>) {
    cerr(text);
    cerr(b"\n");
}

/// `std::cout`, for what writes to an `std::ostream &`.
#[derive(Debug)]
pub struct CoutWriter;

impl Write for CoutWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        cout(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        cout_flush();
        Ok(())
    }
}

/// `std::ofstream`: a text stream on a file.
#[derive(Debug)]
pub struct OutputFile {
    file: File,
}

impl OutputFile {
    /// `std::ofstream f(path)`: creates the file, or empties it. `path` is as the command line
    /// gave it.
    ///
    /// On Windows the C runtime opens with `CREATE_ALWAYS` and normal attributes, which the
    /// system refuses for a hidden or system file that exists. Rust's `File::create` opens
    /// another way, which it allows, so the attributes are looked at first.
    pub fn create(path: &[u8]) -> std::io::Result<OutputFile> {
        let path = path_from_bytes(path);
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
            let refused = FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;
            if let Ok(metadata) = std::fs::metadata(&path)
                && metadata.is_file()
                && metadata.file_attributes() & refused != 0
            {
                return Err(std::io::ErrorKind::PermissionDenied.into());
            }
        }
        Ok(OutputFile {
            file: File::create(path)?,
        })
    }
}

impl Write for OutputFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write_all(&text_mode(buf))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// The bytes of an argument as a path.
#[cfg(unix)]
fn path_from_bytes(path: &[u8]) -> PathBuf {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(OsStr::from_bytes(path))
}

/// The bytes of an argument as a path: the UTF-8 of [`args`].
#[cfg(windows)]
fn path_from_bytes(path: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(path).into_owned())
}

/// `argv`, as bytes: as they are on Linux, in UTF-8 on Windows. (Upstream's `main` gets the
/// ANSI code page's bytes on Windows: docs/improvements.md, I-601.)
pub fn args() -> Vec<Vec<u8>> {
    std::env::args_os()
        .map(|arg| {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStringExt;
                arg.into_vec()
            }
            #[cfg(windows)]
            {
                arg.to_string_lossy().into_owned().into_bytes()
            }
        })
        .collect()
}

/// What a tool does before anything else ([module documentation](self)).
pub fn init() {
    set_stderr_sink(Arc::new(|text: &[u8]| cerr(text)));
}

/// The name C++ gives the type of `exception`, as libstdc++'s terminate handler prints it.
#[cfg_attr(windows, allow(dead_code))]
fn demangled_type_name(exception: &Exception) -> &'static str {
    match exception.kind() {
        ExceptionKind::MissingFile => "OpenColorIO_v2_5::ExceptionMissingFile",
        ExceptionKind::LengthError => "std::length_error",
        ExceptionKind::BadAlloc => "std::bad_alloc",
        ExceptionKind::RegexError => "std::regex_error",
        ExceptionKind::OutOfRange => "std::out_of_range",
        ExceptionKind::SystemError => "std::system_error",
        _ => "OpenColorIO_v2_5::Exception",
    }
}

/// What the C++ runtime does with an exception no `catch` takes: `std::terminate`. What
/// `std::cout` holds is lost, as neither `abort` nor Windows's fast fail flushes it.
/// - **Linux:** libstdc++'s verbose handler writes two lines to stderr, then `abort`: SIGABRT.
/// - **Windows:** nothing on stderr, and the process ends with `0xC0000409`. Rust's `abort` is
///   the same fast fail.
///
/// docs/improvements.md, I-606.
pub fn terminate(exception: &Exception) -> ! {
    if !cfg!(windows) {
        let name = demangled_type_name(exception);
        let mut text =
            format!("terminate called after throwing an instance of '{name}'\n  what():  ")
                .into_bytes();
        text.extend_from_slice(exception.what());
        text.push(b'\n');
        write_stderr(&text);
    }
    std::process::abort()
}

/// `exit(code)`, and the return of `main`: `std::cout` is flushed, then the process ends.
pub fn exit(code: i32) -> ! {
    cout_flush();
    std::process::exit(code)
}

/// `std::cout << transform`, for a tool that prints several transforms: the text to write.
///
/// `operator<<` of a `MatrixTransform` leaves the stream's precision at 16 (docs/improvements.md,
/// I-73), and `std::cout` lives as long as the tool, so what prints after a matrix prints with
/// 16 digits. The library keeps that state only inside one text, a group's: so this holds what
/// has been printed as a group, and a transform's text is what it adds to the group's.
#[derive(Debug, Default)]
pub struct TransformText {
    printed: GroupTransform,
}

impl TransformText {
    /// The text of `transform`, after everything this has been given.
    pub fn of(&mut self, transform: &Transform) -> Vec<u8> {
        const CHILD_PREFIX: usize = "\n        ".len();
        let group_text = |group: &GroupTransform| Transform::Group(group.clone()).to_bytes();
        let before = group_text(&self.printed).len();
        self.printed.append_transform(transform.clone());
        let text = group_text(&self.printed);
        // Both texts end in the group's closing `>`.
        text[before - 1 + CHILD_PREFIX..text.len() - 1].to_vec()
    }
}
