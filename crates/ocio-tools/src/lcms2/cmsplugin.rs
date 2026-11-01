// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the first part of `src/cmsplugin.c` @ lcms2.17: the big-endian readers and writers
//! and the fixed-point conversions. What the file does for plug-ins and contexts isn't
//! reached: the tool registers no plug-in, and every call uses the global context.

use super::cmsio0::IoHandler;
use super::types::{CieXyz, cms_align_long, cvttsd2si};

/// Port of `_cmsReadUInt8Number` (src/cmsplugin.c:111-122 @ lcms2.17).
pub fn cms_read_u8(io: &mut IoHandler) -> Option<u8> {
    let mut tmp = [0u8; 1];
    (io.read(&mut tmp, 1, 1) == 1).then_some(tmp[0])
}

/// Port of `_cmsReadUInt16Number` (src/cmsplugin.c:124-135 @ lcms2.17).
pub fn cms_read_u16(io: &mut IoHandler) -> Option<u16> {
    let mut tmp = [0u8; 2];
    (io.read(&mut tmp, 2, 1) == 1).then(|| u16::from_be_bytes(tmp))
}

/// Port of `_cmsReadUInt16Array` (src/cmsplugin.c:137-154 @ lcms2.17).
pub fn cms_read_u16_array(io: &mut IoHandler, array: &mut [u16]) -> bool {
    for item in array {
        match cms_read_u16(io) {
            Some(value) => *item = value,
            None => return false,
        }
    }
    true
}

/// Port of `_cmsReadUInt16Array` (src/cmsplugin.c:137-154 @ lcms2.17) with a null array.
pub fn cms_skip_u16_array(io: &mut IoHandler, n: u32) -> bool {
    for _ in 0..n {
        if cms_read_u16(io).is_none() {
            return false;
        }
    }
    true
}

/// Port of `_cmsReadUInt32Number` (src/cmsplugin.c:156-167 @ lcms2.17).
pub fn cms_read_u32(io: &mut IoHandler) -> Option<u32> {
    let mut tmp = [0u8; 4];
    (io.read(&mut tmp, 4, 1) == 1).then(|| u32::from_be_bytes(tmp))
}

/// Port of `_cmsReadFloat32Number` (src/cmsplugin.c:169-203 @ lcms2.17).
pub fn cms_read_f32(io: &mut IoHandler) -> Option<f32> {
    let n = f32::from_bits(cms_read_u32(io)?);
    // Safeguard which covers against absurd values
    if n as f64 > 1E+20 || (n as f64) < -1E+20 {
        return None;
    }
    // fpclassify(): zero or normal only
    matches!(
        n.classify(),
        std::num::FpCategory::Zero | std::num::FpCategory::Normal
    )
    .then_some(n)
}

/// Port of `_cmsRead15Fixed16Number` (src/cmsplugin.c:224-238 @ lcms2.17).
pub fn cms_read_15_fixed16(io: &mut IoHandler) -> Option<f64> {
    Some(cms_15_fixed16_to_double(cms_read_u32(io)? as i32))
}

/// Port of `_cmsReadXYZNumber` (src/cmsplugin.c:241-256 @ lcms2.17).
pub fn cms_read_xyz(io: &mut IoHandler) -> Option<CieXyz> {
    let mut xyz = [0u8; 12];
    if io.read(&mut xyz, 12, 1) != 1 {
        return None;
    }
    let number = |at: usize| {
        let bytes = [xyz[at], xyz[at + 1], xyz[at + 2], xyz[at + 3]];
        cms_15_fixed16_to_double(i32::from_be_bytes(bytes))
    };
    Some(CieXyz {
        x: number(0),
        y: number(4),
        z: number(8),
    })
}

/// Port of `_cmsWriteUInt8Number` (src/cmsplugin.c:258-266 @ lcms2.17).
pub fn cms_write_u8(io: &mut IoHandler, n: u8) -> bool {
    io.write(&[n])
}

/// Port of `_cmsWriteUInt16Number` (src/cmsplugin.c:268-279 @ lcms2.17).
pub fn cms_write_u16(io: &mut IoHandler, n: u16) -> bool {
    io.write(&n.to_be_bytes())
}

/// Port of `_cmsWriteUInt32Number` (src/cmsplugin.c:295-306 @ lcms2.17).
pub fn cms_write_u32(io: &mut IoHandler, n: u32) -> bool {
    io.write(&n.to_be_bytes())
}

/// Port of `_cmsWrite15Fixed16Number` (src/cmsplugin.c:337-348 @ lcms2.17).
pub fn cms_write_15_fixed16(io: &mut IoHandler, n: f64) -> bool {
    cms_write_u32(io, cms_double_to_15_fixed16(n) as u32)
}

/// Port of `_cmsWriteXYZNumber` (src/cmsplugin.c:350-362 @ lcms2.17).
pub fn cms_write_xyz(io: &mut IoHandler, xyz: &CieXyz) -> bool {
    let mut bytes = [0u8; 12];
    for (i, value) in [xyz.x, xyz.y, xyz.z].into_iter().enumerate() {
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&cms_double_to_15_fixed16(value).to_be_bytes());
    }
    io.write(&bytes)
}

/// Port of `_cms8Fixed8toDouble` (src/cmsplugin.c:365-368 @ lcms2.17).
pub fn cms_8_fixed8_to_double(fixed8: u16) -> f64 {
    fixed8 as f64 / 256.0
}

/// Port of `_cms15Fixed16toDouble` (src/cmsplugin.c:377-380 @ lcms2.17).
pub fn cms_15_fixed16_to_double(fix32: i32) -> f64 {
    fix32 as f64 / 65536.0
}

/// Port of `_cmsDoubleTo15Fixed16` (src/cmsplugin.c:383-386 @ lcms2.17).
pub fn cms_double_to_15_fixed16(v: f64) -> i32 {
    cvttsd2si((v * 65536.0 + 0.5).floor())
}

/// Port of `_cmsReadTypeBase` (src/cmsplugin.c:421-431 @ lcms2.17). Zero is its failure, too.
pub fn cms_read_type_base(io: &mut IoHandler) -> u32 {
    let mut base = [0u8; 8];
    if io.read(&mut base, 8, 1) != 1 {
        return 0;
    }
    u32::from_be_bytes([base[0], base[1], base[2], base[3]])
}

/// Port of `_cmsWriteTypeBase` (src/cmsplugin.c:434-443 @ lcms2.17).
pub fn cms_write_type_base(io: &mut IoHandler, sig: u32) -> bool {
    let mut base = [0u8; 8];
    base[..4].copy_from_slice(&sig.to_be_bytes());
    io.write(&base)
}

/// Port of `_cmsReadAlignment` (src/cmsplugin.c:445-460 @ lcms2.17).
pub fn cms_read_alignment(io: &mut IoHandler) -> bool {
    let at = io.tell();
    let next_aligned = cms_align_long(at);
    let bytes_to_next_aligned_pos = next_aligned.wrapping_sub(at);
    if bytes_to_next_aligned_pos == 0 {
        return true;
    }
    if bytes_to_next_aligned_pos > 4 {
        return false;
    }
    let mut buffer = [0u8; 4];
    io.read(&mut buffer, bytes_to_next_aligned_pos, 1) == 1
}

/// Port of `_cmsWriteAlignment` (src/cmsplugin.c:462-478 @ lcms2.17).
pub fn cms_write_alignment(io: &mut IoHandler) -> bool {
    let at = io.tell();
    let next_aligned = cms_align_long(at);
    let bytes_to_next_aligned_pos = next_aligned.wrapping_sub(at);
    if bytes_to_next_aligned_pos == 0 {
        return true;
    }
    if bytes_to_next_aligned_pos > 4 {
        return false;
    }
    io.write(&[0u8; 4][..bytes_to_next_aligned_pos as usize])
}

/// What `_cmsEncodeDateTimeNumber` (src/cmsplugin.c:407-418 @ lcms2.17) stores, and the `struct tm` it
/// reads: the fields already as the file holds them, year and month from 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hours: u16,
    pub minutes: u16,
    pub seconds: u16,
}

/// Port of `_cmsEncodeDateTimeNumber` (src/cmsplugin.c:407-418 @ lcms2.17).
pub fn cms_encode_date_time_number(source: &DateTime) -> [u8; 12] {
    let mut bytes = [0u8; 12];
    let fields = [
        source.year,
        source.month,
        source.day,
        source.hours,
        source.minutes,
        source.seconds,
    ];
    for (i, value) in fields.into_iter().enumerate() {
        bytes[i * 2..(i + 1) * 2].copy_from_slice(&value.to_be_bytes());
    }
    bytes
}

/// Port of `_cmsGetTime` (src/cmsplugin.c:1028-1055 @ lcms2.17): the UTC time, as `gmtime` breaks it up.
pub fn cms_get_time() -> DateTime {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let days = (now / 86400) as i64;
    let rest = now % 86400;
    // Days to a civil date, in the proleptic Gregorian calendar `gmtime` uses.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    DateTime {
        year: year as u16,
        month: month as u16,
        day: day as u16,
        hours: (rest / 3600) as u16,
        minutes: (rest % 3600 / 60) as u16,
        seconds: (rest % 60) as u16,
    }
}

#[cfg(test)]
#[path = "cmsplugin_tests.rs"]
mod tests;
