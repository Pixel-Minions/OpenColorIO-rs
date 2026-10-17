// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ICC reader against the wheel (`processor_ops`, from empty caches): the processors of
//! file transforms of upstream's profiles and of generated ones (both processor directions,
//! an inverse transform; the matrices, gammas and LUTs by their bits), and the reader's errors
//! and warnings: the header, the tag table, each tag type's reader (`XYZ `, `curv`, `para`,
//! `desc`, `mluc`), the parametric curves' checks, tags the file doesn't hold, and profiles
//! with bytes flipped at random. `Config::instantiateDisplayFromICCProfile` against
//! `config_calls`: the display, its views and its color space, named by the profile's
//! description.

mod common;

use common::lut_files::{LOGGING, check_file_processors, check_logs, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};
use ocio_testkit::probe::Rng;

/// A profile's tags, each a signature and its data (its type signature first).
struct Profile {
    class: u32,
    color_space: u32,
    pcs: u32,
    intent: u32,
    magic: u32,
    tags: Vec<(u32, Vec<u8>)>,
}

const MNTR: u32 = 0x6D6E_7472;
const RGB: u32 = 0x5247_4220;
const XYZ_PCS: u32 = 0x5859_5A20;
const ACSP: u32 = 0x6163_7370;

fn sig(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

/// An S15.16 number.
fn s15(v: f64) -> i32 {
    (v * 65536.0).round() as i32
}

fn xyz(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut d = b"XYZ \0\0\0\0".to_vec();
    for v in [x, y, z] {
        d.extend_from_slice(&s15(v).to_be_bytes());
    }
    d
}

fn curv(entries: &[u16]) -> Vec<u8> {
    let mut d = b"curv\0\0\0\0".to_vec();
    d.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for e in entries {
        d.extend_from_slice(&e.to_be_bytes());
    }
    d
}

fn para(function_type: u16, params: &[f64]) -> Vec<u8> {
    let mut d = b"para\0\0\0\0".to_vec();
    d.extend_from_slice(&function_type.to_be_bytes());
    d.extend_from_slice(&[0, 0]);
    for &p in params {
        d.extend_from_slice(&s15(p).to_be_bytes());
    }
    d
}

fn desc(text: &[u8], claimed: Option<u32>) -> Vec<u8> {
    let mut d = b"desc\0\0\0\0".to_vec();
    d.extend_from_slice(&claimed.unwrap_or(text.len() as u32).to_be_bytes());
    d.extend_from_slice(text);
    d
}

/// An `mluc` tag of records (language, country, text), each text read right after its record.
fn mluc(records: &[(&[u8; 2], &[u8; 2], &str)]) -> Vec<u8> {
    let mut d = b"mluc\0\0\0\0".to_vec();
    d.extend_from_slice(&(records.len() as u32).to_be_bytes());
    d.extend_from_slice(&12u32.to_be_bytes());
    for (lang, country, text) in records {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        d.extend_from_slice(*lang);
        d.extend_from_slice(*country);
        d.extend_from_slice(&((utf16.len() * 2) as u32).to_be_bytes());
        d.extend_from_slice(&28u32.to_be_bytes());
        for u in utf16 {
            d.extend_from_slice(&u.to_be_bytes());
        }
    }
    d
}

impl Profile {
    fn new(tags: Vec<(u32, Vec<u8>)>) -> Profile {
        Profile {
            class: MNTR,
            color_space: RGB,
            pcs: XYZ_PCS,
            intent: 0,
            magic: ACSP,
            tags,
        }
    }

    /// The profile's bytes: the header, the tag table, then each tag's data, 4-byte aligned.
    fn bytes(&self) -> Vec<u8> {
        let mut header = vec![0u8; 128];
        let put =
            |h: &mut Vec<u8>, at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_be_bytes());
        put(&mut header, 4, sig(b"none"));
        put(&mut header, 8, 0x0220_0000);
        put(&mut header, 12, self.class);
        put(&mut header, 16, self.color_space);
        put(&mut header, 20, self.pcs);
        put(&mut header, 36, self.magic);
        put(&mut header, 64, self.intent);
        let table = 4 + 12 * self.tags.len();
        let mut offset = 128 + table;
        let mut entries = (self.tags.len() as u32).to_be_bytes().to_vec();
        let mut data = Vec::new();
        for (s, d) in &self.tags {
            entries.extend_from_slice(&s.to_be_bytes());
            entries.extend_from_slice(&(offset as u32).to_be_bytes());
            entries.extend_from_slice(&(d.len() as u32).to_be_bytes());
            data.extend_from_slice(d);
            while data.len() % 4 != 0 {
                data.push(0);
            }
            offset = 128 + table + data.len();
        }
        let mut out = header;
        out.extend_from_slice(&entries);
        out.extend_from_slice(&data);
        let size = out.len() as u32;
        out[0..4].copy_from_slice(&size.to_be_bytes());
        out
    }
}

/// The colorants of sRGB's primaries, adapted to D50.
fn colorants() -> Vec<(u32, Vec<u8>)> {
    vec![
        (sig(b"rXYZ"), xyz(0.4361, 0.2225, 0.0139)),
        (sig(b"gXYZ"), xyz(0.3851, 0.7169, 0.0971)),
        (sig(b"bXYZ"), xyz(0.1431, 0.0606, 0.7141)),
    ]
}

/// A matrix/TRC profile with the same TRC on the three channels and a `desc` description.
fn profile_with_trc(trc: Vec<u8>) -> Profile {
    let mut tags = vec![(sig(b"desc"), desc(b"A test profile\0", None))];
    tags.extend(colorants());
    for channel in [b"rTRC", b"gTRC", b"bTRC"] {
        tags.push((sig(channel), trc.clone()));
    }
    Profile::new(tags)
}

/// The case of a profile file `name` holding `bytes`.
fn case(label: &str, name: &str, bytes: &[u8]) -> Case {
    let dir = write_files(&[file(name, bytes), file("label.txt", label)]);
    file_case(
        label,
        &format!("{dir}/{name}"),
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// Generated profiles: each part of the header, the table and the tag readers.
fn generated() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |label: &str, p: &Profile| out.push((label.to_owned(), p.bytes()));

    // TRCs.
    add(
        "curv of 1 entry (gamma 2.2)",
        &profile_with_trc(curv(&[0x0233])),
    );
    add("curv of 0 entries", &profile_with_trc(curv(&[])));
    let ramp: Vec<u16> = (0..256u32)
        .map(|i| ((i * i * 65535) / (255 * 255)) as u16)
        .collect();
    add("curv of 256 entries", &profile_with_trc(curv(&ramp)));
    add("curv of 2 entries", &profile_with_trc(curv(&[0, 65535])));
    // The largest curve SampleICC reads (65536 entries), and one entry more.
    let full: Vec<u16> = (0..65536u32).map(|i| i as u16).collect();
    add("curv of 65536 entries", &profile_with_trc(curv(&full)));
    let over: Vec<u16> = (0..65537u32).map(|i| i as u16).collect();
    add("curv of 65537 entries", &profile_with_trc(curv(&over)));
    let flat: Vec<u16> = vec![30000; 16];
    add("a flat curv", &profile_with_trc(curv(&flat)));
    let decreasing: Vec<u16> = (0..64u32).map(|i| (65535 - i * 1000) as u16).collect();
    add("a decreasing curv", &profile_with_trc(curv(&decreasing)));
    for (label, t, params) in [
        ("para 0", 0u16, vec![2.4]),
        ("para 0 of gamma 1", 0, vec![1.0]),
        ("para 0 of gamma 0", 0, vec![0.0]),
        ("para 0 of a negative gamma", 0, vec![-2.2]),
        ("para 1", 1, vec![2.4, 1.1, -0.1]),
        ("para 1 with b >= 0", 1, vec![2.4, 1.0, 0.05]),
        ("para 1 with a <= 0", 1, vec![2.4, -1.0, 0.1]),
        ("para 1, gamma below 1", 1, vec![0.8, 1.2, -0.2]),
        ("para 2", 2, vec![2.4, 1.057, -0.1, 0.1]),
        ("para 2 not reaching 1", 2, vec![2.4, 0.9, -0.1, 0.0]),
        (
            "para 3 (sRGB)",
            3,
            vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045],
        ),
        ("para 3, a negative c", 3, vec![2.4, 0.95, 0.05, -0.1, 0.04]),
        (
            "para 3, a discontinuity down",
            3,
            vec![2.4, 0.95, 0.05, 2.0, 0.3],
        ),
        (
            "para 3, a discontinuity up",
            3,
            vec![2.4, 0.95, 0.05, 0.01, 0.3],
        ),
        (
            "para 3, a negative power argument",
            3,
            vec![2.4, 0.5, -0.6, 0.0, 0.5],
        ),
        ("para 3, not smooth", 3, vec![2.2, 0.95, 0.05, 0.0773, 0.04]),
        (
            "para 3, a power-law argument of 0",
            3,
            vec![2.4, 1.0, -0.5, 0.0, 0.5],
        ),
        (
            "para 3, d = 0",
            3,
            vec![2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.0],
        ),
        ("para 3, a negative d", 3, vec![2.4, 0.5, 0.6, 0.1, -0.5]),
        (
            "para 3, a negative argument, c * d > 0",
            3,
            vec![2.4, 0.5, -0.6, 0.1, 0.5],
        ),
        (
            "para 3, a negative argument, d < 0",
            3,
            vec![2.4, 0.5, 0.1, 0.1, -0.5],
        ),
        ("para 1, gamma of 1", 1, vec![1.0, 1.1, -0.1]),
        ("para 2, gamma of 1", 2, vec![1.0, 1.1, -0.1, 0.05]),
        (
            "para 4, gamma of 1",
            4,
            vec![1.0, 0.905, 0.052, 0.073, 0.04, 0.1, 0.1],
        ),
        // 30000 * 100 * 1023 code values: past a 32-bit `long` (Windows), not a 64-bit one.
        (
            "para 3, a linear segment past 2^31 codes",
            3,
            vec![2.4, 1.0, 0.0, 30000.0, 100.0],
        ),
        (
            "para 4, a power law past 2^31 codes",
            4,
            vec![2.4, 30000.0, 0.0, 0.5, 1.0, 0.0, 0.0],
        ),
        ("para 4", 4, vec![2.4, 0.905, 0.052, 0.073, 0.04, 0.1, 0.1]),
        (
            "para 4, not continuous",
            4,
            vec![2.4, 0.905, 0.052, 0.073, 0.04, 0.0, 0.1],
        ),
        ("para 5", 5, vec![2.4, 1.0]),
        ("para 1 with 2 params", 1, vec![2.4, 1.1]),
        (
            "para 2 with 7 params",
            2,
            vec![2.4, 1.057, -0.1, 0.1, 0.0, 0.0, 0.0],
        ),
    ] {
        add(label, &profile_with_trc(para(t, &params)));
    }
    // Mixed TRCs.
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    mixed.tags[5] = (sig(b"gTRC"), para(0, &[2.2]));
    add("curv red, para green", &mixed);
    let mut mixed = profile_with_trc(para(0, &[2.2]));
    mixed.tags[6] = (sig(b"bTRC"), curv(&[0x0233]));
    add("para red, curv blue", &mixed);
    let mut mixed = profile_with_trc(para(1, &[2.4, 1.1, -0.1]));
    mixed.tags[5] = (sig(b"gTRC"), para(2, &[2.4, 1.057, -0.1, 0.1]));
    add("para types 1 and 2", &mixed);
    let mut mixed = profile_with_trc(curv(&ramp));
    mixed.tags[6] = (sig(b"bTRC"), curv(&ramp[..128]));
    add("curv lengths differ", &mixed);
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    mixed.tags[4] = (sig(b"rTRC"), xyz(1.0, 1.0, 1.0));
    add("an XYZ TRC", &mixed);
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    mixed.tags.remove(6);
    add("no blue TRC", &mixed);
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    mixed.tags.remove(2);
    add("no green colorant", &mixed);
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    mixed.tags[1] = (sig(b"rXYZ"), curv(&[0x0233]));
    add("a curv colorant", &mixed);
    let mut mixed = profile_with_trc(curv(&[0x0233]));
    let mut long_xyz = xyz(0.4, 0.2, 0.01);
    long_xyz.extend_from_slice(&[0; 12]);
    mixed.tags[1] = (sig(b"rXYZ"), long_xyz);
    add("an XYZ of two numbers", &mixed);

    // Header and table.
    let base = profile_with_trc(curv(&[0x0233]));
    for (label, class) in [
        ("an input class", 0x7363_6E72u32),
        ("an unknown class", 0x1234_5678),
        ("a class past 2^31", 0xFFFF_FFFF),
        ("a zero class", 0),
    ] {
        let mut p = profile_with_trc(curv(&[0x0233]));
        p.class = class;
        add(label, &p);
    }
    for (label, intent) in [
        ("intent 3", 3u32),
        ("intent 4", 4),
        ("an intent past 2^31", 0x8000_0001),
    ] {
        let mut p = profile_with_trc(curv(&[0x0233]));
        p.intent = intent;
        add(label, &p);
    }
    let mut p = profile_with_trc(curv(&[0x0233]));
    p.magic = 0x6163_7371;
    add("a wrong magic number", &p);
    let mut p = profile_with_trc(curv(&[0x0233]));
    p.pcs = sig(b"Lab ");
    add("a Lab PCS", &p);
    let mut p = profile_with_trc(curv(&[0x0233]));
    p.color_space = sig(b"CMYK");
    add("a CMYK device", &p);
    add("no tags", &Profile::new(Vec::new()));

    let bytes = base.bytes();
    out.push(("the header alone, short".into(), bytes[..100].to_vec()));
    out.push((
        "the header without a tag count".into(),
        bytes[..128].to_vec(),
    ));
    out.push(("a truncated tag table".into(), bytes[..140].to_vec()));
    out.push(("truncated tags".into(), bytes[..bytes.len() - 10].to_vec()));
    let mut many = bytes.clone();
    many[128..132].copy_from_slice(&101u32.to_be_bytes());
    out.push(("101 tags".into(), many));
    let mut hundred = bytes.clone();
    hundred[128..132].copy_from_slice(&100u32.to_be_bytes());
    out.push(("100 tags, a short table".into(), hundred));
    let mut far = bytes.clone();
    // The rXYZ tag's offset past the end of the file.
    far[128 + 4 + 12 + 4..128 + 4 + 12 + 8].copy_from_slice(&0x00FF_FFFFu32.to_be_bytes());
    out.push(("a colorant past the end".into(), far));

    // Descriptions.
    let mut add = |label: &str, p: &Profile| out.push((label.to_owned(), p.bytes()));
    let with_desc = |tags: Vec<(u32, Vec<u8>)>| {
        let mut p = profile_with_trc(curv(&[0x0233]));
        p.tags.remove(0);
        for t in tags.into_iter().rev() {
            p.tags.insert(0, t);
        }
        p
    };
    add("no description", &with_desc(vec![]));
    add(
        "a desc without its NUL",
        &with_desc(vec![(sig(b"desc"), desc(b"No NUL", None))]),
    );
    add(
        "a desc with text after a NUL",
        &with_desc(vec![(sig(b"desc"), desc(b"Before\0After", None))]),
    );
    add(
        "a desc longer than the file",
        &with_desc(vec![(sig(b"desc"), desc(b"Short", Some(0x0100_0000)))]),
    );
    add(
        "a desc of size 2^32 - 1",
        &with_desc(vec![(sig(b"desc"), desc(b"Short", Some(0xFFFF_FFFF)))]),
    );
    add(
        "an empty desc",
        &with_desc(vec![(sig(b"desc"), desc(b"", None))]),
    );
    add(
        "a curv desc",
        &with_desc(vec![(sig(b"desc"), curv(&[1, 2]))]),
    );
    add(
        "an mluc desc, US",
        &with_desc(vec![(
            sig(b"desc"),
            mluc(&[
                (b"fr", b"FR", "Profil"),
                (b"en", b"US", "US profile"),
                (b"en", b"UK", "UK"),
            ]),
        )]),
    );
    add(
        "an mluc desc, UK and en",
        &with_desc(vec![(
            sig(b"desc"),
            mluc(&[
                (b"de", b"DE", "Profil"),
                (b"en", b"CA", "en"),
                (b"en", b"UK", "UK profile"),
            ]),
        )]),
    );
    add(
        "an mluc desc, en",
        &with_desc(vec![(
            sig(b"desc"),
            mluc(&[(b"de", b"DE", "Profil"), (b"en", b"CA", "Canada")]),
        )]),
    );
    add(
        "an mluc desc, first",
        &with_desc(vec![(
            sig(b"desc"),
            mluc(&[(b"de", b"DE", "Profil"), (b"ja", b"JP", "Japan")]),
        )]),
    );
    add(
        "a dscm and a desc",
        &with_desc(vec![
            (sig(b"desc"), desc(b"The desc\0", None)),
            (sig(b"dscm"), mluc(&[(b"en", b"US", "The dscm")])),
        ]),
    );
    add(
        "a failing dscm, then a desc",
        &with_desc(vec![
            (sig(b"desc"), desc(b"The desc\0", None)),
            (sig(b"dscm"), desc(b"Short", Some(0x0100_0000))),
        ]),
    );
    add(
        "two descs",
        &with_desc(vec![
            (sig(b"desc"), desc(b"First\0", None)),
            (sig(b"desc"), desc(b"Second\0", None)),
        ]),
    );
    out
}

#[test]
fn icc_profiles_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases: Vec<Case> = generated()
        .iter()
        .map(|(label, bytes)| case(label, "profile.icc", bytes))
        .collect();
    // Upstream's profiles, by their own extensions and an inverse transform.
    let dir = ocio_testkit::paths::upstream_dir().join("tests/data/files");
    for name in [
        "icc-test-1.icc",
        "icc-test-2.pf",
        "icc-test-3.icm",
        "icc-test-pc1.icc",
        "icc-test-pc2.icc",
        "icc-test-pc3.icc",
        "icc-test-pc4.icc",
    ] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        cases.push(case(name, name, &bytes));
        let inverse_dir = write_files(&[file(name, bytes), file("label.txt", "inverse")]);
        cases.push(file_case(
            format!("{name} inverse transform"),
            &format!("{inverse_dir}/{name}"),
            Interpolation::Linear,
            TransformDirection::Inverse,
        ));
    }
    check_file_processors(&cases);
}

/// The parametric curves' warnings, and the interpolations a 1D LUT doesn't take.
#[test]
fn icc_warnings_as_in_the_wheel() {
    let mut cases: Vec<Case> = generated()
        .iter()
        .filter(|(label, _)| label.starts_with("para"))
        // Files of their own: the port's file cache may hold the other tests' files, read
        // without their warnings.
        .map(|(label, bytes)| case(&format!("warnings, {label}"), "profile.icc", bytes))
        .collect();
    let bytes =
        std::fs::read(ocio_testkit::paths::upstream_dir().join("tests/data/files/icc-test-3.icm"))
            .unwrap();
    for interp in [Interpolation::Tetrahedral, Interpolation::Nearest] {
        let dir = write_files(&[
            file("p.icm", bytes.clone()),
            file("label.txt", format!("warnings, {interp:?}")),
        ]);
        cases.push(file_case(
            format!("{interp:?}"),
            &format!("{dir}/p.icm"),
            interp,
            TransformDirection::Forward,
        ));
    }
    check_logs(&cases, TransformDirection::Forward);
}

/// Profiles with bytes changed at random (bounded: 300 profiles of a few hundred bytes, none
/// with a description), read as the wheel reads them.
#[test]
fn icc_profiles_with_random_bytes_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    // Without descriptions: a random string size would make the wheel allocate up to 4 GiB
    // (U-70), which the oracle machine shouldn't.
    let ramp: Vec<u16> = (0..256u32)
        .map(|i| ((i * i * 65535) / (255 * 255)) as u16)
        .collect();
    let bases: Vec<Vec<u8>> = [
        curv(&ramp),
        curv(&[0x0233]),
        para(3, &[2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045]),
        para(4, &[2.4, 0.905, 0.052, 0.073, 0.04, 0.1, 0.1]),
    ]
    .into_iter()
    .map(|trc| {
        let mut p = profile_with_trc(trc);
        p.tags.remove(0);
        p.bytes()
    })
    .collect();
    let mut rng = Rng::new(0x1cc);
    let mut cases = Vec::new();
    for n in 0..300 {
        let mut bytes = bases[n % bases.len()].clone();
        for _ in 0..1 + rng.next_u64() % 4 {
            let at = (rng.next_u64() % bytes.len() as u64) as usize;
            bytes[at] = rng.next_u64() as u8;
        }
        cases.push(case(&format!("random {n}"), "profile.icc", &bytes));
    }
    check_file_processors(&cases);
}
