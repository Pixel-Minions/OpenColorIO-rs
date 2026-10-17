// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
//
// Ported, and so altered, from minizip-ng 4.0.10 (https://github.com/zlib-ng/minizip-ng),
// whose sources carry:
//
//   Copyright (C) Nathan Moinvaziri
//     https://github.com/zlib-ng/minizip-ng
//   Copyright (C) 2009-2010 Mathias Svensson
//   Copyright (C) 2007-2008 Even Rouault
//   Copyright (C) 1998-2010 Gilles Vollant
//
//   This program is distributed under the terms of the same license as zlib:
//
//   This software is provided 'as-is', without any express or implied
//   warranty.  In no event will the authors be held liable for any damages
//   arising from the use of this software.
//
//   Permission is granted to anyone to use this software for any purpose,
//   including commercial applications, and to alter it and redistribute it
//   freely, subject to the following restrictions:
//
//   1. The origin of this software must not be misrepresented; you must not
//      claim that you wrote the original software. If you use this software
//      in a product, an acknowledgement in the product documentation would be
//      appreciated but is not required.
//   2. Altered source versions must be plainly marked as such, and must not be
//      misrepresented as being the original software.
//   3. This notice may not be removed or altered from any source distribution.

//! A port of the parts of minizip-ng 4.0.10 that OpenColorIO 2.5.2 reaches when it reads an
//! `.ocioz` archive (`src/OpenColorIO/OCIOZArchive.cpp`): which entries an archive lists, in
//! what order and under what name and CRC, how a name is compared with a path, and what an
//! entry's buffer holds, including where minizip-ng can't read all of it; and when it writes
//! one (`Config::archive`): the entries' headers, attributes, extra fields and order. OpenColorIO's
//! results depend on each.
//!
//! **The version.** OpenColorIO 2.5.2 builds minizip-ng 4.0.10 where it installs it
//! (`RECOMMENDED_VERSION 4.0.10`, `share/cmake/modules/FindExtPackages.cmake:114-116`), with
//! zlib and without the other compression methods, encryption or signing
//! (`share/cmake/modules/install/Installminizip-ng.cmake:73-87`, which makes minizip-ng's
//! build define `MZ_ZIP_NO_CRYPTO`). Its source is pinned as the submodule
//! `upstream/minizip-ng` (tag `4.0.10`), so the citations can be checked.
//!
//! **What isn't minizip-ng's.** zlib's `inflate` and `deflate` are `flate2`'s on its `zlib-rs`
//! backend, a port of zlib ([`inflate`], owner decision 2026-10-17), whose deflate streams
//! differ from zlib 1.3.1's past a few KiB (deviation D-6); the system file streams read the
//! file whole; a DOS date is broken down in UTC (D-6).

pub(crate) mod inflate;
pub(crate) mod mz_crypt;
pub(crate) mod mz_os;
pub(crate) mod mz_strm;
pub(crate) mod mz_zip;
pub(crate) mod mz_zip_rw;
