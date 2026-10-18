// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported from minizip-ng 4.0.10 (see `mod.rs` for its notice): an altered version.

//! minizip-ng's streams, as OpenColorIO reads an archive with them: the stream interface
//! (`mz_strm.c`), the memory stream (`mz_strm_mem.c`), and the stack it opens an archive file
//! with, a split stream over a buffered stream over the system's file (`mz_strm_split.c`,
//! `mz_strm_buf.c`, `mz_strm_os_posix.c` and `mz_strm_os_win32.c`). The buffered stream reads
//! the file 32,767 bytes at a time and keeps its own idea of the file position, which a seek
//! that fails leaves behind the file's (`mz_zip_recover_cd` reads on from there).

use std::io::Read;

use ocio_ops::platform::create_input_file_stream;

use super::mz_os::mz_os_file_exists;

/// minizip-ng's results (mz.h:21-47 @ 4.0.10).
pub(crate) const MZ_OK: i32 = 0;
pub(crate) const MZ_STREAM_ERROR: i32 = -1;
pub(crate) const MZ_BUF_ERROR: i32 = -5;
pub(crate) const MZ_END_OF_LIST: i32 = -100;
pub(crate) const MZ_END_OF_STREAM: i32 = -101;
pub(crate) const MZ_PARAM_ERROR: i32 = -102;
pub(crate) const MZ_FORMAT_ERROR: i32 = -103;
pub(crate) const MZ_INTERNAL_ERROR: i32 = -104;
pub(crate) const MZ_CRC_ERROR: i32 = -105;
pub(crate) const MZ_EXIST_ERROR: i32 = -107;
pub(crate) const MZ_SUPPORT_ERROR: i32 = -109;
pub(crate) const MZ_OPEN_ERROR: i32 = -111;
pub(crate) const MZ_SEEK_ERROR: i32 = -113;
pub(crate) const MZ_READ_ERROR: i32 = -115;
pub(crate) const MZ_WRITE_ERROR: i32 = -116;
#[cfg(windows)]
pub(crate) const MZ_SYMLINK_ERROR: i32 = -118;

/// `MZ_SEEK_SET`, `MZ_SEEK_CUR` and `MZ_SEEK_END` (mz.h:58-60 @ 4.0.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Set,
    Cur,
    End,
}

/// A stream minizip-ng reads: the operations of its `mz_stream_vtbl` that reading reaches.
pub(crate) trait Stream {
    /// Reads up to `buf.len()` bytes: how many it read, or an error (negative).
    fn read(&mut self, buf: &mut [u8]) -> i32;
    /// Moves the position: `MZ_OK` or an error.
    fn seek(&mut self, offset: i64, origin: Origin) -> i32;
    /// The position, or an error (negative).
    fn tell(&mut self) -> i64;
    /// The stream's error state (`mz_stream_error`): nonzero after a failed operation.
    fn error(&self) -> i32;
}

/// `mz_stream_seek`'s check of a negative absolute offset, then the stream's seek.
///
/// Port of `mz_stream_seek` (mz_strm.c:223-232 @ 4.0.10).
pub(crate) fn mz_stream_seek(stream: &mut dyn Stream, offset: i64, origin: Origin) -> i32 {
    if origin == Origin::Set && offset < 0 {
        return MZ_SEEK_ERROR;
    }
    stream.seek(offset, origin)
}

/// A little-endian value of `len` bytes, or `MZ_END_OF_STREAM` (`MZ_STREAM_ERROR` where the
/// stream is in error) when fewer bytes are left.
///
/// Port of `mz_stream_read_value` (mz_strm.c:43-58 @ 4.0.10).
fn mz_stream_read_value(stream: &mut dyn Stream, len: usize) -> Result<u64, i32> {
    let mut buf = [0u8; 8];
    if stream.read(&mut buf[..len]) == len as i32 {
        let mut value = 0u64;
        for (n, b) in buf[..len].iter().enumerate() {
            value += u64::from(*b) << (8 * n);
        }
        Ok(value)
    } else if stream.error() != 0 {
        Err(MZ_STREAM_ERROR)
    } else {
        Err(MZ_END_OF_STREAM)
    }
}

/// Port of `mz_stream_read_uint16` (mz_strm.c:71-80 @ 4.0.10).
pub(crate) fn mz_stream_read_uint16(stream: &mut dyn Stream) -> Result<u16, i32> {
    mz_stream_read_value(stream, 2).map(|v| v as u16)
}

/// Port of `mz_stream_read_uint32` (mz_strm.c:82-91 @ 4.0.10).
pub(crate) fn mz_stream_read_uint32(stream: &mut dyn Stream) -> Result<u32, i32> {
    mz_stream_read_value(stream, 4).map(|v| v as u32)
}

/// Port of `mz_stream_read_int64` (mz_strm.c:93-95 @ 4.0.10).
pub(crate) fn mz_stream_read_int64(stream: &mut dyn Stream) -> Result<i64, i32> {
    mz_stream_read_value(stream, 8).map(|v| v as i64)
}

/// Port of `mz_stream_read_uint64` (mz_strm.c:97-99 @ 4.0.10).
pub(crate) fn mz_stream_read_uint64(stream: &mut dyn Stream) -> Result<u64, i32> {
    mz_stream_read_value(stream, 8)
}

/// `MZ_STREAM_FIND_SIZE` (mz_strm.c:16 @ 4.0.10).
const MZ_STREAM_FIND_SIZE: usize = 1024;

/// Searches forwards from the stream's position, over at most `max_seek` bytes, for `find`:
/// its position (and the stream left there), or `MZ_EXIST_ERROR`.
///
/// Port of `mz_stream_find` (mz_strm.c:234-292 @ 4.0.10), buffer for buffer.
pub(crate) fn mz_stream_find(
    stream: &mut dyn Stream,
    find: &[u8],
    max_seek: i64,
) -> Result<i64, i32> {
    let mut buf = [0u8; MZ_STREAM_FIND_SIZE];
    let mut buf_pos: i32 = 0;
    let mut read_size: i32 = MZ_STREAM_FIND_SIZE as i32;
    let mut read_pos: i64 = 0;
    let mut first = true;
    let find_size = find.len() as i32;

    let start_pos = stream.tell();

    while read_pos < max_seek {
        if read_size as i64 > max_seek - read_pos - buf_pos as i64
            && (max_seek - read_pos - buf_pos as i64) < MZ_STREAM_FIND_SIZE as i64
        {
            read_size = (max_seek - read_pos - buf_pos as i64) as i32;
        }

        let start = buf_pos as usize;
        let mut read = stream.read(&mut buf[start..start + read_size as usize]);
        if read <= 0 || read + buf_pos < find_size {
            break;
        }

        let mut i = 0;
        while i <= read + buf_pos - find_size {
            let at = i as usize;
            if &buf[at..at + find.len()] != find {
                i += 1;
                continue;
            }

            let disk_pos = stream.tell();

            // Seek to position on disk where the data was found
            let err = mz_stream_seek(
                stream,
                disk_pos - (read as i64 + buf_pos as i64 - i as i64),
                Origin::Set,
            );
            if err != MZ_OK {
                return Err(MZ_EXIST_ERROR);
            }

            return Ok(start_pos + read_pos + i as i64);
        }

        if first {
            read -= find_size;
            read_size -= find_size;
            buf_pos = find_size;
            first = false;
        }

        buf.copy_within(read as usize..read as usize + find.len(), 0);
        read_pos += read as i64;
    }

    Err(MZ_EXIST_ERROR)
}

/// Searches backwards from the stream's position, over at most `max_seek` bytes, for `find`:
/// its position (and the stream left there), or `MZ_EXIST_ERROR`.
///
/// Port of `mz_stream_find_reverse` (mz_strm.c:294-357 @ 4.0.10), buffer for buffer.
pub(crate) fn mz_stream_find_reverse(
    stream: &mut dyn Stream,
    find: &[u8],
    max_seek: i64,
) -> Result<i64, i32> {
    let mut buf = [0u8; MZ_STREAM_FIND_SIZE];
    let mut buf_pos: i32 = 0;
    let mut read_size: i32 = MZ_STREAM_FIND_SIZE as i32;
    let mut read_pos: i64 = 0;
    let mut first = true;
    let find_size = find.len() as i32;
    let size = MZ_STREAM_FIND_SIZE as i32;

    let start_pos = stream.tell();

    while read_pos < max_seek {
        if read_size as i64 > max_seek - read_pos && (max_seek - read_pos) < size as i64 {
            read_size = (max_seek - read_pos) as i32;
        }

        if mz_stream_seek(
            stream,
            start_pos - (read_pos + read_size as i64),
            Origin::Set,
        ) != MZ_OK
        {
            break;
        }
        let mut read = stream.read(&mut buf[..read_size as usize]);
        if read <= 0 || read + buf_pos < find_size {
            break;
        }
        if read + buf_pos < size {
            let dest = (size - (read + buf_pos)) as usize;
            buf.copy_within(0..read as usize, dest);
        }

        let mut i = find_size;
        while i <= read + buf_pos {
            let at = (size - i) as usize;
            if &buf[at..at + find.len()] != find {
                i += 1;
                continue;
            }

            let disk_pos = stream.tell();

            // Seek to position on disk where the data was found
            let err = mz_stream_seek(stream, disk_pos + buf_pos as i64 - i as i64, Origin::Set);
            if err != MZ_OK {
                return Err(MZ_EXIST_ERROR);
            }

            return Ok(start_pos - (read_pos - buf_pos as i64 + i as i64));
        }

        if first {
            read -= find_size;
            read_size -= find_size;
            buf_pos = find_size;
            first = false;
        }

        if read == 0 {
            break;
        }

        buf.copy_within(0..find.len(), read_size as usize);
        read_pos += read as i64;
    }

    Err(MZ_EXIST_ERROR)
}

/// A memory stream opened to grow (`MZ_OPEN_MODE_CREATE`), or over a given buffer: what it
/// holds up to its limit, and its position.
///
/// Port of `mz_stream_mem` (mz_strm_mem.c @ 4.0.10). Its allocated size only matters where
/// a read past the limit could reach it, which the limit prevents, so the port keeps the
/// bytes up to the limit; a write past them extends them.
#[derive(Debug, Default)]
pub(crate) struct MemStream {
    /// The bytes up to `limit`.
    data: Vec<u8>,
    position: usize,
    /// `MZ_OPEN_MODE_CREATE`: seeks past the end grow the stream.
    grows: bool,
}

impl MemStream {
    /// A stream opened with `MZ_OPEN_MODE_CREATE`.
    ///
    /// Port of `mz_stream_mem_open` (mz_strm_mem.c:71-87 @ 4.0.10).
    pub(crate) fn create() -> MemStream {
        MemStream {
            data: Vec::new(),
            position: 0,
            grows: true,
        }
    }

    /// A stream reading `buf`, whose limit is its size.
    ///
    /// Port of `mz_stream_mem_set_buffer` with `mz_stream_mem_open` for reading
    /// (mz_strm_mem.c:199-204, 71-87 @ 4.0.10).
    pub(crate) fn over(buf: &[u8]) -> MemStream {
        MemStream {
            data: buf.to_vec(),
            position: 0,
            grows: false,
        }
    }

    /// Sets the limit reads stop at.
    ///
    /// Port of `mz_stream_mem_set_buffer_limit` (mz_strm_mem.c:228-231 @ 4.0.10).
    pub(crate) fn set_buffer_limit(&mut self, limit: usize) {
        self.data.resize(limit, 0);
    }

    /// The bytes from `at` up to the limit (`mz_stream_mem_get_buffer_at`).
    pub(crate) fn bytes_at(&self, at: usize) -> &[u8] {
        self.data.get(at..).unwrap_or(&[])
    }

    /// Writes `buf` at the position.
    ///
    /// Port of `mz_stream_mem_write` (mz_strm_mem.c:113-144 @ 4.0.10) for a stream that grows.
    pub(crate) fn write(&mut self, buf: &[u8]) -> i32 {
        if buf.is_empty() {
            return 0;
        }
        let end = self.position + buf.len();
        if self.data.len() < end {
            self.data.resize(end, 0);
        }
        self.data[self.position..end].copy_from_slice(buf);
        self.position = end;
        buf.len() as i32
    }

    /// Port of `mz_stream_write_uint8` (mz_strm.c:133-135, through `mz_stream_write_value`,
    /// 112-131 @ 4.0.10).
    pub(crate) fn write_uint8(&mut self, value: u8) -> i32 {
        if self.write(&[value]) != 1 {
            return MZ_STREAM_ERROR;
        }
        MZ_OK
    }
}

impl Stream for MemStream {
    /// Port of `mz_stream_mem_read` (mz_strm_mem.c:96-111 @ 4.0.10).
    fn read(&mut self, buf: &mut [u8]) -> i32 {
        let size = buf.len().min(self.data.len().saturating_sub(self.position));
        if size == 0 {
            return 0;
        }
        buf[..size].copy_from_slice(&self.data[self.position..self.position + size]);
        self.position += size;
        size as i32
    }

    /// Port of `mz_stream_mem_seek` (mz_strm_mem.c:151-183 @ 4.0.10).
    fn seek(&mut self, offset: i64, origin: Origin) -> i32 {
        let new_pos = match origin {
            Origin::Cur => self.position as i64 + offset,
            Origin::End => self.data.len() as i64 + offset,
            Origin::Set => offset,
        };
        // Past the end, a stream that doesn't grow fails; one that grows grows.
        if (new_pos > self.data.len() as i64 && !self.grows) || new_pos < 0 {
            return MZ_SEEK_ERROR;
        }
        self.position = new_pos as usize;
        MZ_OK
    }

    /// Port of `mz_stream_mem_tell` (mz_strm_mem.c:146-149 @ 4.0.10).
    fn tell(&mut self) -> i64 {
        self.position as i64
    }

    /// Port of `mz_stream_mem_error` (mz_strm_mem.c:192-197 @ 4.0.10): never an error.
    fn error(&self) -> i32 {
        MZ_OK
    }
}

/// A file opened for reading by the system stream: its bytes, read whole, and the position,
/// which a seek moves only where the system's own seek of the file succeeds (a position past
/// the largest file the file system holds fails on Linux, as `fseeko`'s `lseek` does: 16 TiB
/// on ext4). A file that opens but can't be read (a directory on Linux) fails each read.
///
/// Port of the reading of `mz_stream_os` (mz_strm_os_posix.c, mz_strm_os_win32.c @ 4.0.10).
/// The file opens as OpenColorIO's own file streams do: through its UTF-16 conversion on
/// Windows, as the bytes on Linux.
#[derive(Debug)]
struct OsFile {
    file: std::fs::File,
    data: Option<Vec<u8>>,
    position: i64,
    error: i32,
}

impl OsFile {
    /// Port of `mz_stream_os_open` for reading (mz_strm_os_posix.c:67-93 @ 4.0.10).
    fn open(path: &[u8]) -> Option<OsFile> {
        let mut file = create_input_file_stream(path).ok()?;
        let mut data = Vec::new();
        let data = file.read_to_end(&mut data).ok().map(|_| data);
        Some(OsFile {
            file,
            data,
            position: 0,
            error: 0,
        })
    }

    fn size(&self) -> i64 {
        self.data.as_ref().map_or(0, |d| d.len() as i64)
    }

    /// Port of `mz_stream_os_read` (mz_strm_os_posix.c:102-110 @ 4.0.10).
    fn read(&mut self, buf: &mut [u8]) -> i32 {
        let Some(data) = &self.data else {
            self.error = MZ_READ_ERROR;
            return MZ_READ_ERROR;
        };
        let start = usize::try_from(self.position)
            .unwrap_or(usize::MAX)
            .min(data.len());
        let size = buf.len().min(data.len() - start);
        buf[..size].copy_from_slice(&data[start..start + size]);
        self.position += size as i64;
        size as i32
    }

    /// Port of `mz_stream_os_seek` (mz_strm_os_posix.c:132-156, mz_strm_os_win32.c:210-243 @
    /// 4.0.10): a position before the start fails, and so does one the system refuses
    /// (`fseeko`'s `lseek` past the file system's largest file, `SetFilePointerEx`), which the
    /// port asks the system itself; one past the end is allowed.
    fn seek(&mut self, offset: i64, origin: Origin) -> i32 {
        use std::io::Seek;
        let base = match origin {
            Origin::Set => 0,
            Origin::Cur => self.position,
            Origin::End => self.size(),
        };
        match base.checked_add(offset) {
            Some(position)
                if position >= 0
                    && self
                        .file
                        .seek(std::io::SeekFrom::Start(position as u64))
                        .is_ok() =>
            {
                self.position = position;
                MZ_OK
            }
            _ => {
                self.error = MZ_SEEK_ERROR;
                MZ_SEEK_ERROR
            }
        }
    }
}

/// The size of the buffered stream's read buffer (`char readbuf[INT16_MAX]`, mz_strm_buf.c:37
/// @ 4.0.10).
const READBUF_SIZE: i32 = i16::MAX as i32;

/// The buffered stream between the split stream and the system's file: what it last read of
/// the file, where the reader is in it, and the file position it believes is at its end.
///
/// Port of `mz_stream_buffered` (mz_strm_buf.c:34-48 @ 4.0.10) as minizip-ng reads with it: no
/// write buffer (`writebuf_len` stays 0, so the flushes write nothing), and no hit and miss
/// counts, which only its debug output reads.
#[derive(Debug)]
struct Buffered {
    base: OsFile,
    readbuf: Vec<u8>,
    readbuf_len: i32,
    readbuf_pos: i32,
    /// The file position at the end of the read buffer, as the stream keeps it: reads add to
    /// it, a seek within the file sets it, a tell reads it from the file, and a seek forwards
    /// past the buffer adds the offset to it, wrapping (docs/improvements.md, I-214).
    position: i64,
}

impl Buffered {
    /// Port of `mz_stream_buffered_open` (mz_strm_buf.c:72-77 @ 4.0.10), over the opened file:
    /// `mz_stream_buffered_reset` (60-70).
    fn open(base: OsFile) -> Buffered {
        Buffered {
            base,
            readbuf: vec![0u8; READBUF_SIZE as usize],
            readbuf_len: 0,
            readbuf_pos: 0,
            position: 0,
        }
    }

    /// Port of `mz_stream_buffered_read` (mz_strm_buf.c:117-183 @ 4.0.10).
    fn read(&mut self, buf: &mut [u8]) -> i32 {
        let size = buf.len() as i32;
        let mut buf_len = 0usize;
        let mut bytes_left_to_read = size;

        while bytes_left_to_read > 0 {
            if self.readbuf_len == 0 || self.readbuf_pos == self.readbuf_len {
                if self.readbuf_len == READBUF_SIZE {
                    self.readbuf_pos = 0;
                    self.readbuf_len = 0;
                }

                let bytes_to_read = READBUF_SIZE - (self.readbuf_len - self.readbuf_pos);
                // Upstream reads `bytes_to_read` bytes at `readbuf + readbuf_pos`, which passes
                // the buffer's end where a short read at the file's end left it partly filled
                // and read; the file is then still at its end (every seek of the file empties
                // the buffer), and reads nothing.
                let start = self.readbuf_pos as usize;
                let end = (start + bytes_to_read as usize).min(self.readbuf.len());
                let bytes_read = self.base.read(&mut self.readbuf[start..end]);
                if bytes_read < 0 {
                    return bytes_read;
                }

                self.readbuf_len += bytes_read;
                self.position = self.position.wrapping_add(i64::from(bytes_read));

                if bytes_read == 0 {
                    break;
                }
            }

            if self.readbuf_len - self.readbuf_pos > 0 {
                let bytes_to_copy = (self.readbuf_len - self.readbuf_pos).min(bytes_left_to_read);
                let from = self.readbuf_pos as usize;
                buf[buf_len..buf_len + bytes_to_copy as usize]
                    .copy_from_slice(&self.readbuf[from..from + bytes_to_copy as usize]);

                buf_len += bytes_to_copy as usize;
                bytes_left_to_read -= bytes_to_copy;
                self.readbuf_pos += bytes_to_copy;
            }
        }

        size - bytes_left_to_read
    }

    /// Port of `mz_stream_buffered_tell` (mz_strm_buf.c:247-261 @ 4.0.10).
    fn tell(&mut self) -> i64 {
        let mut position = self.base.position;
        self.position = position;
        if self.readbuf_len > 0 {
            position -= i64::from(self.readbuf_len - self.readbuf_pos);
        }
        position
    }

    /// Port of `mz_stream_buffered_seek` (mz_strm_buf.c:263-332 @ 4.0.10), with `mz_stream_seek`'s
    /// check of the file's seek (mz_strm.c:223-232).
    fn seek(&mut self, offset: i64, origin: Origin) -> i32 {
        let mut offset = offset;
        match origin {
            Origin::Set => {
                if self.readbuf_len > 0
                    && offset < self.position
                    && offset >= self.position.wrapping_sub(i64::from(self.readbuf_len))
                {
                    self.readbuf_pos = offset
                        .wrapping_sub(self.position.wrapping_sub(i64::from(self.readbuf_len)))
                        as i32;
                    return MZ_OK;
                }
                self.position = offset;
            }
            Origin::Cur => {
                if self.readbuf_len > 0 {
                    let left = i64::from(self.readbuf_len - self.readbuf_pos);
                    if offset <= left {
                        // `readbuf_pos += (uint32_t)offset`
                        self.readbuf_pos = self.readbuf_pos.wrapping_add(offset as u32 as i32);
                        return MZ_OK;
                    }
                    offset -= left;
                    self.position = self.position.wrapping_add(offset);
                }
            }
            Origin::End => {}
        }

        self.readbuf_len = 0;
        self.readbuf_pos = 0;

        if origin == Origin::Set && offset < 0 {
            return MZ_SEEK_ERROR;
        }
        self.base.seek(offset, origin)
    }
}

/// `MZ_ZIP_MAGIC_DISKHEADER` (mz_strm_split.c:25 @ 4.0.10).
const MZ_ZIP_MAGIC_DISKHEADER: u32 = 0x0807_4b50;

/// The stream OpenColorIO's reader opens an archive file with: a split stream, which reads
/// the parts of a split archive (`name.z01`, `name.z02`, ...) when an entry or the central
/// directory says it is on another disk, and the archive file itself otherwise.
///
/// Port of `mz_stream_split` (mz_strm_split.c @ 4.0.10), opened for reading.
#[derive(Debug)]
pub(crate) struct SplitStream {
    path_cd: Vec<u8>,
    /// `path_disk`, a buffer of `path_disk_size` bytes holding a C string.
    path_disk: Vec<u8>,
    path_disk_size: usize,
    number_disk: i32,
    current_disk: i32,
    is_open: bool,
    /// The size of the disk it is on, as the buffered stream tells it when it opens the disk.
    current_disk_size: i64,
    base: Option<Buffered>,
}

impl SplitStream {
    /// Opens the archive `path` (a C string).
    ///
    /// Port of `mz_stream_split_open` (mz_strm_split.c:173-205 @ 4.0.10) for reading.
    pub(crate) fn open(path: &[u8]) -> Result<SplitStream, i32> {
        let path_disk_size = path.len() + 10;
        let mut split = SplitStream {
            path_cd: path.to_vec(),
            path_disk: path.to_vec(),
            path_disk_size,
            number_disk: -1,
            current_disk: 0,
            is_open: false,
            current_disk_size: 0,
            base: None,
        };
        match split.goto_disk(-1) {
            MZ_OK => Ok(split),
            err => Err(err),
        }
    }

    /// Port of `mz_stream_split_open_disk` (mz_strm_split.c:64-139 @ 4.0.10) for reading.
    fn open_disk(&mut self, number_disk: i32) -> i32 {
        let mut err = MZ_OK;
        // Check if we are reading or writing a disk part or the cd disk
        let disk_part = number_disk >= 0;

        // Construct disk path
        if disk_part {
            if let Some(i) = self.path_disk.iter().rposition(|&b| b == b'.') {
                // snprintf(&path_disk[i], path_disk_size - i, ".z%02d", number_disk + 1)
                let suffix = format!(".z{:02}", i64::from(number_disk) + 1);
                let room = self.path_disk_size - i - 1;
                self.path_disk.truncate(i);
                self.path_disk
                    .extend_from_slice(&suffix.as_bytes()[..suffix.len().min(room)]);
            }
        } else {
            self.path_disk = self.path_cd.clone();
        }

        // If disk part doesn't exist during reading then return MZ_EXIST_ERROR
        if disk_part {
            if self.path_disk == self.path_cd {
                err = MZ_EXIST_ERROR;
            } else {
                err = mz_os_file_exists(&self.path_disk);
            }
        }

        if err == MZ_OK {
            match OsFile::open(&self.path_disk) {
                Some(file) => self.base = Some(Buffered::open(file)),
                None => err = MZ_OPEN_ERROR,
            }
        }
        if err == MZ_OK {
            self.current_disk = number_disk;
            if self.current_disk == 0 {
                let base = self.base.as_mut().expect("the disk is open");
                let mut magic = [0u8; 4];
                let read = base.read(&mut magic);
                if read != 4 || u32::from_le_bytes(magic) != MZ_ZIP_MAGIC_DISKHEADER {
                    err = MZ_FORMAT_ERROR;
                }
            }
        }

        if err == MZ_OK {
            // Get the size of the current disk we are on
            let base = self.base.as_mut().expect("the disk is open");
            let position = base.tell();
            base.seek(0, Origin::End);
            self.current_disk_size = base.tell();
            base.seek(position, Origin::Set);

            self.is_open = true;
        }

        err
    }

    /// Port of `mz_stream_split_goto_disk` (mz_strm_split.c:151-171 @ 4.0.10) for reading.
    fn goto_disk(&mut self, number_disk: i32) -> i32 {
        let mut err = MZ_OK;
        if number_disk != self.current_disk || self.base.is_none() {
            // mz_stream_split_close_disk
            self.base = None;
            err = self.open_disk(number_disk);
            if err == MZ_OK {
                self.number_disk = number_disk;
            }
        }
        err
    }

    /// The disk the operations go to (`MZ_STREAM_PROP_DISK_NUMBER`).
    ///
    /// Port of `mz_stream_split_get_prop_int64` (mz_strm_split.c:361-377 @ 4.0.10).
    pub(crate) fn disk_number(&self) -> i64 {
        i64::from(self.number_disk)
    }

    /// Sets the disk the next operations go to (`MZ_STREAM_PROP_DISK_NUMBER`).
    ///
    /// Port of `mz_stream_split_set_prop_int64` (mz_strm_split.c:379-392 @ 4.0.10).
    pub(crate) fn set_disk_number(&mut self, value: i64) {
        self.number_disk = value as i32;
    }
}

impl Stream for SplitStream {
    /// Port of `mz_stream_read` over `mz_stream_split_read` (mz_strm.c:34-41,
    /// mz_strm_split.c:214-250 @ 4.0.10).
    fn read(&mut self, buf: &mut [u8]) -> i32 {
        if !self.is_open {
            return MZ_STREAM_ERROR;
        }
        let size = buf.len() as i32;
        let mut bytes_left = size;
        let mut pos = 0usize;

        let err = self.goto_disk(self.number_disk);
        if err != MZ_OK {
            return err;
        }

        while bytes_left > 0 {
            let read = match self.base.as_mut() {
                Some(base) => base.read(&mut buf[pos..pos + bytes_left as usize]),
                None => MZ_STREAM_ERROR,
            };
            if read < 0 {
                return read;
            }
            if read == 0 {
                if self.current_disk < 0 {
                    // No more disks to goto
                    break;
                }
                if size != bytes_left {
                    // Report read from previous disk before switching
                    break;
                }
                let err = self.goto_disk(self.current_disk + 1);
                if err == MZ_EXIST_ERROR {
                    self.current_disk = -1;
                }
                if err != MZ_OK {
                    return err;
                }
            }
            bytes_left -= read;
            pos += read as usize;
        }
        size - bytes_left
    }

    /// Port of `mz_stream_split_seek` (mz_strm_split.c:317-345 @ 4.0.10).
    fn seek(&mut self, offset: i64, origin: Origin) -> i32 {
        if !self.is_open {
            return MZ_STREAM_ERROR;
        }
        let err = self.goto_disk(self.number_disk);
        if err != MZ_OK {
            return err;
        }

        let mut offset = offset;
        if origin == Origin::Cur && self.number_disk != -1 {
            let position = self.base.as_mut().expect("the disk is open").tell();
            let mut disk_left = self.current_disk_size - position;
            while offset > disk_left {
                let err = self.goto_disk(self.current_disk + 1);
                if err != MZ_OK {
                    return err;
                }
                offset -= disk_left;
                disk_left = self.current_disk_size;
            }
        }
        match self.base.as_mut() {
            // mz_stream_seek
            Some(_) if origin == Origin::Set && offset < 0 => MZ_SEEK_ERROR,
            Some(base) => base.seek(offset, origin),
            None => MZ_STREAM_ERROR,
        }
    }

    /// Port of `mz_stream_split_tell` (mz_strm_split.c:308-315 @ 4.0.10).
    fn tell(&mut self) -> i64 {
        if !self.is_open {
            return i64::from(MZ_STREAM_ERROR);
        }
        let err = self.goto_disk(self.number_disk);
        if err != MZ_OK {
            return i64::from(err);
        }
        self.base
            .as_mut()
            .map_or(i64::from(MZ_STREAM_ERROR), Buffered::tell)
    }

    /// Port of `mz_stream_split_error` (mz_strm_split.c:356-359 @ 4.0.10).
    fn error(&self) -> i32 {
        // mz_stream_buffered_error: the file's
        self.base.as_ref().map_or(0, |b| b.base.error)
    }
}
