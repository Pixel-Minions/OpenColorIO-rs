// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Generated ICC profiles, to hand `ociobakelut --displayicc`. They are inputs: what a tool makes
//! of them comes from the wheel. They follow ICC.1:2022 and ICC.2 (the `mpet` elements), down to
//! the broken ones, which are valid ones with one thing wrong.

#![allow(dead_code)]

/// A small generator, so that the values and the corruptions are the same on every run.
#[derive(Debug, Clone)]
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 33) as usize) % n
    }
}

fn s15(value: f64) -> [u8; 4] {
    ((value * 65536.0).round() as i32).to_be_bytes()
}

/// A tag's type: the signature and the reserved bytes.
fn base(sig: &[u8; 4]) -> Vec<u8> {
    [&sig[..], &[0; 4]].concat()
}

/// `XYZType`.
pub(crate) fn xyz(x: f64, y: f64, z: f64) -> Vec<u8> {
    [
        base(b"XYZ "),
        s15(x).to_vec(),
        s15(y).to_vec(),
        s15(z).to_vec(),
    ]
    .concat()
}

/// `curveType` with a table: an empty one is the identity.
pub(crate) fn curv(points: &[u16]) -> Vec<u8> {
    let mut data = base(b"curv");
    data.extend_from_slice(&(points.len() as u32).to_be_bytes());
    for point in points {
        data.extend_from_slice(&point.to_be_bytes());
    }
    data
}

/// `curveType` with a gamma.
pub(crate) fn gamma(value: f64) -> Vec<u8> {
    let mut data = base(b"curv");
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(&((value * 256.0) as u16).to_be_bytes());
    data
}

/// `parametricCurveType`.
pub(crate) fn para(kind: u16, params: &[f64]) -> Vec<u8> {
    let mut data = base(b"para");
    data.extend_from_slice(&kind.to_be_bytes());
    data.extend_from_slice(&[0; 2]);
    for param in params {
        data.extend_from_slice(&s15(*param));
    }
    data
}

/// A curve table from a function on 0..1.
pub(crate) fn table(n: usize, f: impl Fn(f64) -> f64) -> Vec<u16> {
    (0..n)
        .map(|i| (f(i as f64 / (n - 1) as f64).clamp(0.0, 1.0) * 65535.0).round() as u16)
        .collect()
}

/// `multiLocalizedUnicodeType` with one English string.
pub(crate) fn mluc(text: &str) -> Vec<u8> {
    let mut data = base(b"mluc");
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(&12u32.to_be_bytes());
    data.extend_from_slice(b"enUS");
    data.extend_from_slice(&(text.len() as u32 * 2).to_be_bytes());
    data.extend_from_slice(&28u32.to_be_bytes());
    for c in text.encode_utf16() {
        data.extend_from_slice(&c.to_be_bytes());
    }
    data
}

/// The values of a CLUT, row by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    /// A smooth ramp per output channel.
    Smooth,
    /// Values of all sizes, the last dimension moving fastest.
    Noise(u64),
    /// Each output is 0 or 0xFFFF, for the widest differences between nodes.
    Extreme(u64),
}

/// `(input channels, output channels, grid, fill)` as 16-bit values.
pub(crate) fn clut16(inputs: usize, outputs: usize, grid: &[usize], fill: Fill) -> Vec<u16> {
    let total: usize = grid[..inputs].iter().product();
    let mut rng = Rng(match fill {
        Fill::Noise(seed) | Fill::Extreme(seed) => seed | 1,
        Fill::Smooth => 1,
    });
    let mut values = Vec::with_capacity(total * outputs);
    for index in 0..total {
        // The coordinates of the node, in 0..1.
        let mut rest = index;
        let mut coordinates = vec![0f64; inputs];
        for t in (0..inputs).rev() {
            coordinates[t] = (rest % grid[t]) as f64 / (grid[t] - 1) as f64;
            rest /= grid[t];
        }
        for output in 0..outputs {
            values.push(match fill {
                Fill::Smooth => {
                    let source = coordinates[output % inputs];
                    let mixed = coordinates.iter().sum::<f64>() / inputs as f64;
                    ((0.75 * source + 0.25 * mixed) * 65535.0).round() as u16
                }
                Fill::Noise(_) => rng.next() as u16,
                Fill::Extreme(_) => {
                    if rng.next() & 1 == 0 {
                        0
                    } else {
                        0xFFFF
                    }
                }
            });
        }
    }
    values
}

const IDENTITY: [f64; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// `lut16Type`, with the same curve on all channels, and the same on both sides.
pub(crate) fn mft2(
    inputs: usize,
    outputs: usize,
    grid: usize,
    matrix: Option<[f64; 9]>,
    curve: &[u16],
    fill: Fill,
) -> Vec<u8> {
    let mut data = base(b"mft2");
    data.extend_from_slice(&[inputs as u8, outputs as u8, grid as u8, 0]);
    for value in matrix.unwrap_or(IDENTITY) {
        data.extend_from_slice(&s15(value));
    }
    data.extend_from_slice(&(curve.len() as u16).to_be_bytes());
    data.extend_from_slice(&(curve.len() as u16).to_be_bytes());
    for _ in 0..inputs {
        for value in curve {
            data.extend_from_slice(&value.to_be_bytes());
        }
    }
    if grid > 0 {
        for value in clut16(inputs, outputs, &[grid; 16], fill) {
            data.extend_from_slice(&value.to_be_bytes());
        }
    }
    for _ in 0..outputs {
        for value in curve {
            data.extend_from_slice(&value.to_be_bytes());
        }
    }
    data
}

/// `lut8Type`.
pub(crate) fn mft1(
    inputs: usize,
    outputs: usize,
    grid: usize,
    matrix: Option<[f64; 9]>,
    fill: Fill,
) -> Vec<u8> {
    let mut data = base(b"mft1");
    data.extend_from_slice(&[inputs as u8, outputs as u8, grid as u8, 0]);
    for value in matrix.unwrap_or(IDENTITY) {
        data.extend_from_slice(&s15(value));
    }
    let ramp: Vec<u8> = (0..=255).collect();
    for _ in 0..inputs {
        data.extend_from_slice(&ramp);
    }
    for value in clut16(inputs, outputs, &[grid; 16], fill) {
        data.push((value >> 8) as u8);
    }
    for _ in 0..outputs {
        data.extend_from_slice(&ramp);
    }
    data
}

/// Curves one after another, aligned to 4 bytes, as `lutAToBType` wants them.
fn curve_set(curves: &[Vec<u8>]) -> Vec<u8> {
    let mut data = Vec::new();
    for curve in curves {
        data.extend_from_slice(curve);
        while data.len() % 4 != 0 {
            data.push(0);
        }
    }
    data
}

/// The parts of a `lutAToBType` or `lutBToAType`, by name.
#[derive(Debug, Clone, Default)]
pub(crate) struct LutV4 {
    pub(crate) a: Option<Vec<Vec<u8>>>,
    pub(crate) clut: Option<(Vec<usize>, Fill, u8)>,
    pub(crate) m: Option<Vec<Vec<u8>>>,
    pub(crate) matrix: Option<[f64; 12]>,
    pub(crate) b: Option<Vec<Vec<u8>>>,
}

/// `lutAToBType` (`mAB `) or `lutBToAType` (`mBA `): the parts go in the order of the processing,
/// which is why the file has them in the other order for one of the types.
pub(crate) fn lut_v4(signature: &[u8; 4], inputs: usize, outputs: usize, parts: &LutV4) -> Vec<u8> {
    let mut body = Vec::new();
    let header = 32;
    let mut place = |bytes: Vec<u8>| -> u32 {
        let offset = header + body.len() as u32;
        body.extend_from_slice(&bytes);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset
    };
    let offset_b = parts
        .b
        .as_ref()
        .map_or(0, |curves| place(curve_set(curves)));
    let offset_mat = parts.matrix.as_ref().map_or(0, |matrix| {
        place(matrix.iter().flat_map(|value| s15(*value)).collect())
    });
    let offset_m = parts
        .m
        .as_ref()
        .map_or(0, |curves| place(curve_set(curves)));
    let offset_c = parts.clut.as_ref().map_or(0, |(grid, fill, precision)| {
        let mut clut = vec![0u8; 16];
        let dimensions = grid.len();
        for (slot, points) in clut.iter_mut().zip(grid) {
            *slot = *points as u8;
        }
        clut.extend_from_slice(&[*precision, 0, 0, 0]);
        let mut full = grid.clone();
        full.resize(16, 2);
        for value in clut16(dimensions, outputs, &full, *fill) {
            if *precision == 1 {
                clut.push((value >> 8) as u8);
            } else {
                clut.extend_from_slice(&value.to_be_bytes());
            }
        }
        place(clut)
    });
    let offset_a = parts
        .a
        .as_ref()
        .map_or(0, |curves| place(curve_set(curves)));
    let mut data = base(signature);
    data.extend_from_slice(&[inputs as u8, outputs as u8, 0, 0]);
    for offset in [offset_b, offset_mat, offset_m, offset_c, offset_a] {
        data.extend_from_slice(&offset.to_be_bytes());
    }
    data.extend_from_slice(&body);
    data
}

/// A `curf` of `multiProcessElementsType`.
pub(crate) fn segmented(breaks: &[f32], segments: &[Vec<u8>]) -> Vec<u8> {
    let mut data = base(b"curf");
    data.extend_from_slice(&(segments.len() as u16).to_be_bytes());
    data.extend_from_slice(&[0; 2]);
    for point in breaks {
        data.extend_from_slice(&point.to_be_bytes());
    }
    for segment in segments {
        data.extend_from_slice(segment);
    }
    data
}

/// A `parf` segment.
pub(crate) fn formula(kind: u16, params: &[f32]) -> Vec<u8> {
    let mut data = base(b"parf");
    data.extend_from_slice(&kind.to_be_bytes());
    data.extend_from_slice(&[0; 2]);
    for param in params {
        data.extend_from_slice(&param.to_be_bytes());
    }
    data
}

/// A `samf` segment.
pub(crate) fn sampled(points: &[f32]) -> Vec<u8> {
    let mut data = base(b"samf");
    data.extend_from_slice(&(points.len() as u32).to_be_bytes());
    for point in points {
        data.extend_from_slice(&point.to_be_bytes());
    }
    data
}

/// Elements or curves after a table of positions.
fn positioned(start: usize, items: &[Vec<u8>]) -> Vec<u8> {
    let mut table = Vec::new();
    let mut body = Vec::new();
    let table_size = 8 * items.len();
    for item in items {
        table.extend_from_slice(&((start + table_size + body.len()) as u32).to_be_bytes());
        table.extend_from_slice(&(item.len() as u32).to_be_bytes());
        body.extend_from_slice(item);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    [table, body].concat()
}

/// A `cvst` element.
pub(crate) fn cvst(curves: &[Vec<u8>]) -> Vec<u8> {
    let mut data = base(b"cvst");
    data.extend_from_slice(&(curves.len() as u16).to_be_bytes());
    data.extend_from_slice(&(curves.len() as u16).to_be_bytes());
    data.extend_from_slice(&positioned(data.len(), curves));
    data
}

/// A `matf` element.
pub(crate) fn matf(inputs: usize, outputs: usize, values: &[f32]) -> Vec<u8> {
    let mut data = base(b"matf");
    data.extend_from_slice(&(inputs as u16).to_be_bytes());
    data.extend_from_slice(&(outputs as u16).to_be_bytes());
    for value in values {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data
}

/// A `clut` element.
pub(crate) fn clutf(inputs: usize, outputs: usize, grid: &[usize], fill: Fill) -> Vec<u8> {
    let mut data = base(b"clut");
    data.extend_from_slice(&(inputs as u16).to_be_bytes());
    data.extend_from_slice(&(outputs as u16).to_be_bytes());
    let mut dimensions = [0u8; 16];
    for (slot, points) in dimensions.iter_mut().zip(grid) {
        *slot = *points as u8;
    }
    data.extend_from_slice(&dimensions);
    let mut full = grid.to_vec();
    full.resize(16, 2);
    for value in clut16(inputs, outputs, &full, fill) {
        data.extend_from_slice(&(value as f32 / 65535.0).to_be_bytes());
    }
    data
}

/// `multiProcessElementsType`.
pub(crate) fn mpet(inputs: usize, outputs: usize, elements: &[Vec<u8>]) -> Vec<u8> {
    let mut data = base(b"mpet");
    data.extend_from_slice(&(inputs as u16).to_be_bytes());
    data.extend_from_slice(&(outputs as u16).to_be_bytes());
    data.extend_from_slice(&(elements.len() as u32).to_be_bytes());
    data.extend_from_slice(&positioned(data.len(), elements));
    data
}

/// `colorantTableType`.
pub(crate) fn clrt(names: &[&str]) -> Vec<u8> {
    let mut data = base(b"clrt");
    data.extend_from_slice(&(names.len() as u32).to_be_bytes());
    for name in names {
        let mut field = [0u8; 32];
        field[..name.len()].copy_from_slice(name.as_bytes());
        data.extend_from_slice(&field);
        data.extend_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    }
    data
}

/// A profile.
#[derive(Debug, Clone)]
pub(crate) struct Profile {
    pub(crate) version: u32,
    pub(crate) class: [u8; 4],
    pub(crate) space: [u8; 4],
    pub(crate) pcs: [u8; 4],
    pub(crate) intent: u32,
    pub(crate) tags: Vec<([u8; 4], Vec<u8>)>,
}

impl Profile {
    pub(crate) fn new(version: u32, class: &[u8; 4], space: &[u8; 4], pcs: &[u8; 4]) -> Profile {
        Profile {
            version,
            class: *class,
            space: *space,
            pcs: *pcs,
            intent: 0,
            tags: Vec::new(),
        }
    }

    pub(crate) fn tag(mut self, sig: &[u8; 4], data: Vec<u8>) -> Profile {
        self.tags.push((*sig, data));
        self
    }

    /// The colorants and curves of a matrix-shaper with one curve.
    pub(crate) fn shaper(self, curve: &[u8]) -> Profile {
        self.colorants()
            .tag(b"rTRC", curve.to_vec())
            .tag(b"gTRC", curve.to_vec())
            .tag(b"bTRC", curve.to_vec())
    }

    /// The colorants of sRGB, adapted to D50.
    pub(crate) fn colorants(self) -> Profile {
        self.tag(b"rXYZ", xyz(0.4360, 0.2225, 0.0139))
            .tag(b"gXYZ", xyz(0.3851, 0.7169, 0.0971))
            .tag(b"bXYZ", xyz(0.1431, 0.0606, 0.7139))
    }

    /// The usual tags that aren't read.
    pub(crate) fn described(self) -> Profile {
        self.tag(b"desc", mluc("generated"))
            .tag(b"wtpt", xyz(0.9642, 1.0, 0.8249))
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        let table = 4 + 12 * self.tags.len();
        let mut body = Vec::new();
        let mut directory = (self.tags.len() as u32).to_be_bytes().to_vec();
        for (sig, data) in &self.tags {
            directory.extend_from_slice(sig);
            directory.extend_from_slice(&((128 + table + body.len()) as u32).to_be_bytes());
            directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
        }
        let size = 128 + table + body.len();
        let mut header = Vec::with_capacity(size);
        header.extend_from_slice(&(size as u32).to_be_bytes());
        header.extend_from_slice(b"test");
        header.extend_from_slice(&self.version.to_be_bytes());
        header.extend_from_slice(&self.class);
        header.extend_from_slice(&self.space);
        header.extend_from_slice(&self.pcs);
        header.extend_from_slice(&[0x07, 0xE9, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
        header.extend_from_slice(b"acsp");
        header.extend_from_slice(&[0; 24]);
        header.extend_from_slice(&self.intent.to_be_bytes());
        for value in [0.9642, 1.0, 0.8249] {
            header.extend_from_slice(&s15(value));
        }
        header.extend_from_slice(b"test");
        header.resize(128, 0);
        [header, directory, body].concat()
    }
}

pub(crate) const V2: u32 = 0x02400000;
pub(crate) const V4: u32 = 0x04300000;

/// A parameter of each `para` kind.
fn para_kinds() -> Vec<(&'static str, Vec<u8>)> {
    let srgb = [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.04045];
    vec![
        ("para0", para(0, &[2.2])),
        ("para0_one", para(0, &[1.0])),
        ("para1", para(1, &[2.2, 1.1, -0.1])),
        ("para2", para(2, &[2.2, 1.1, -0.1, 0.05])),
        ("para3", para(3, &srgb)),
        // A break exactly on the black or the white of the cube.
        ("para3_break_0", para(3, &[2.4, 0.95, 0.05, 0.5, 0.0])),
        ("para3_break_1", para(3, &[2.4, 0.95, 0.05, 0.5, 1.0])),
        ("para2_c_1", para(2, &[2.2, 1.0, 0.0, 1.0])),
        (
            "para4_break_1",
            para(4, &[2.4, 0.9, 0.05, 0.08, 1.0, 0.01, 0.002]),
        ),
        ("para4", para(4, &[2.4, 0.9, 0.05, 0.08, 0.04, 0.01, 0.002])),
    ]
}

/// A curve of each kind `curv` has.
fn curv_kinds() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("identity", curv(&[])),
        ("gamma18", gamma(1.8)),
        ("gamma22", gamma(2.2)),
        ("gamma1", gamma(1.0)),
        ("gamma0", gamma(0.0)),
        ("table2", curv(&[0, 0xFFFF])),
        ("table_ramp", curv(&table(256, |x| x))),
        ("table_power", curv(&table(4096, |x| x.powf(2.2)))),
        ("table_big", curv(&table(32767, |x| x.powf(0.8)))),
        ("table_descending", curv(&table(33, |x| 1.0 - x))),
        (
            "table_not_monotonic",
            curv(&table(33, |x| 0.5 + 0.5 * (x * 12.0).sin())),
        ),
        ("table_flat", curv(&[0x8000; 8])),
        ("table_steps", curv(&table(17, |x| (x * 4.0).floor() / 4.0))),
        ("too_long", curv(&table(40000, |x| x))),
    ]
}

/// The element lists of `mpet` tags, three channels in and three out.
fn mpet_kinds() -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let full = |segments: Vec<u8>| cvst(&[segments.clone(), segments.clone(), segments]);
    let line = segmented(&[], &[formula(0, &[1.0, 1.0, 0.0, 0.0])]);
    let power = segmented(
        &[0.0, 1.0],
        &[
            formula(0, &[1.0, 0.0, 0.0, 0.0]),
            formula(0, &[2.4, 0.95, 0.05, 0.0]),
            formula(0, &[1.0, 0.0, 0.0, 1.0]),
        ],
    );
    let log = segmented(&[], &[formula(1, &[0.5, 0.25, 1.5, 0.75, 0.1])]);
    let exp = segmented(&[], &[formula(2, &[0.5, 2.0, 1.5, 0.25, 0.1])]);
    let samples = segmented(
        &[0.0, 1.0],
        &[
            formula(0, &[1.0, 0.0, 0.0, 0.0]),
            sampled(&[0.1, 0.3, 0.45, 0.8, 1.0]),
            formula(0, &[1.0, 0.0, 0.0, 1.0]),
        ],
    );
    let first_sampled = segmented(
        &[0.5],
        &[sampled(&[0.2, 0.4, 0.6]), formula(0, &[1.0, 1.0, 0.0, 0.0])],
    );
    let scale = |value: f32| {
        matf(
            3,
            3,
            &[
                value, 0.1, 0.0, 0.0, value, 0.1, 0.1, 0.0, value, 0.01, 0.02, 0.03,
            ],
        )
    };
    let to = |outputs: usize| -> Vec<u8> {
        let mut values = Vec::new();
        for row in 0..outputs {
            for column in 0..3 {
                values.push(if row % 3 == column { 0.9 } else { 0.05 });
            }
        }
        values.resize(3 * outputs + outputs, 0.0);
        matf(3, outputs, &values)
    };
    let big = [
        1e19, 1e19, 1e19, 1e19, 1e19, 1e19, 1e19, 1e19, 1e19, 0.0, 0.0, 0.0f32,
    ];
    let difference = [
        1.0, -1.0, 0.0, 0.0, 1.0, -1.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0f32,
    ];
    vec![
        ("line", vec![full(line.clone())]),
        ("power", vec![full(power)]),
        ("log", vec![full(log)]),
        ("exp", vec![full(exp)]),
        ("samples", vec![full(samples.clone())]),
        ("first_sampled", vec![full(first_sampled)]),
        ("matrix", vec![scale(0.8)]),
        ("curves_matrix", vec![full(samples), scale(0.9)]),
        (
            "clut3",
            vec![clutf(3, 3, &[5, 5, 5], Fill::Smooth), full(line.clone())],
        ),
        ("clut3_noise", vec![clutf(3, 3, &[3, 4, 5], Fill::Noise(7))]),
        (
            "matrix_to_2",
            vec![to(2), clutf(2, 3, &[5, 7], Fill::Noise(3))],
        ),
        ("matrix_to_1", vec![to(1), clutf(1, 3, &[9], Fill::Smooth)]),
        (
            "matrix_to_1_noise",
            vec![to(1), clutf(1, 3, &[2], Fill::Noise(5))],
        ),
        (
            "matrix_to_4",
            vec![to(4), clutf(4, 3, &[3, 3, 3, 3], Fill::Noise(11))],
        ),
        (
            "matrix_to_5",
            vec![to(5), clutf(5, 3, &[3; 5], Fill::Noise(13))],
        ),
        (
            "matrix_to_6",
            vec![to(6), clutf(6, 3, &[2; 6], Fill::Noise(17))],
        ),
        (
            "matrix_to_7",
            vec![to(7), clutf(7, 3, &[2; 7], Fill::Noise(19))],
        ),
        (
            "matrix_to_8",
            vec![to(8), clutf(8, 3, &[2; 8], Fill::Noise(23))],
        ),
        ("bacs_eacs", vec![base(b"bACS"), scale(0.8), base(b"eACS")]),
        ("unknown_element", vec![base(b"nope")]),
        // Two matrices past a float's range, and their difference: infinity minus infinity.
        (
            "nan_to_clut",
            vec![
                matf(3, 3, &big),
                matf(3, 3, &big),
                matf(3, 3, &difference),
                clutf(3, 3, &[3, 3, 3], Fill::Noise(67)),
            ],
        ),
        (
            "nan_to_curves",
            vec![
                matf(3, 3, &big),
                matf(3, 3, &big),
                matf(3, 3, &difference),
                full(line.clone()),
            ],
        ),
        (
            "nan_out",
            vec![matf(3, 3, &big), matf(3, 3, &big), matf(3, 3, &difference)],
        ),
    ]
}

/// A name and bytes for every kind of profile the sweep goes through.
pub(crate) fn generated() -> Vec<(String, Vec<u8>)> {
    let mut all: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |label: String, profile: Profile| all.push((label, profile.bytes()));

    // Matrix-shapers, with each kind of curve, in both versions and with both PCSs.
    for (name, curve) in curv_kinds().into_iter().chain(para_kinds()) {
        for (version_name, version) in [("v2", V2), ("v4", V4)] {
            for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab ")] {
                add(
                    format!("shaper_{name}_{version_name}_{pcs_name}"),
                    Profile::new(version, b"mntr", b"RGB ", pcs)
                        .described()
                        .shaper(&curve),
                );
            }
        }
    }

    // One channel of a shaper different, and missing or broken parts.
    let ramp = curv(&table(256, |x| x));
    let base_shaper = || Profile::new(V4, b"mntr", b"RGB ", b"XYZ ").described();
    add(
        "shaper_mixed_curves".into(),
        base_shaper()
            .colorants()
            .tag(b"rTRC", gamma(1.8))
            .tag(b"gTRC", para(3, &[2.4, 0.95, 0.05, 0.08, 0.04]))
            .tag(b"bTRC", ramp.clone()),
    );
    add(
        "shaper_no_blue_curve".into(),
        base_shaper()
            .colorants()
            .tag(b"rTRC", gamma(1.8))
            .tag(b"gTRC", gamma(1.8)),
    );
    add(
        "shaper_no_colorants".into(),
        base_shaper()
            .tag(b"rTRC", gamma(1.8))
            .tag(b"gTRC", gamma(1.8))
            .tag(b"bTRC", gamma(1.8)),
    );
    add(
        "shaper_singular".into(),
        base_shaper()
            .tag(b"rXYZ", xyz(0.5, 0.5, 0.5))
            .tag(b"gXYZ", xyz(0.5, 0.5, 0.5))
            .tag(b"bXYZ", xyz(0.5, 0.5, 0.5))
            .shaper(&gamma(2.2)),
    );
    add(
        "shaper_bad_para".into(),
        base_shaper().shaper(&para(5, &[1.0])),
    );
    add(
        "shaper_broken_xyz_type".into(),
        base_shaper()
            .tag(b"rXYZ", {
                let mut data = xyz(0.4360, 0.2225, 0.0139);
                data[..4].copy_from_slice(&[0x17, 0xA5, 0x05, 0xB8]);
                data
            })
            .tag(b"gXYZ", xyz(0.3851, 0.7169, 0.0971))
            .tag(b"bXYZ", xyz(0.1431, 0.0606, 0.7139))
            .tag(b"rTRC", gamma(1.8))
            .tag(b"gTRC", gamma(1.8))
            .tag(b"bTRC", gamma(1.8)),
    );
    add(
        "shaper_broken_curve_type".into(),
        base_shaper().shaper(&{
            let mut data = gamma(1.8);
            data[..4].copy_from_slice(&[0x94, 0x78, 0xEE, 0x00]);
            data
        }),
    );

    // Tags of the transform's tail.
    add(
        "colorant_table".into(),
        base_shaper()
            .shaper(&gamma(2.2))
            .tag(b"clrt", clrt(&["red", "green", "blue"])),
    );
    add(
        "colorant_table_empty".into(),
        base_shaper().shaper(&gamma(2.2)).tag(b"clrt", clrt(&[])),
    );
    add(
        "colorant_table_too_long".into(),
        base_shaper()
            .shaper(&gamma(2.2))
            .tag(b"clrt", clrt(&["c"; 17])),
    );
    add(
        "colorant_table_cut".into(),
        base_shaper().shaper(&gamma(2.2)).tag(b"clrt", {
            let mut data = clrt(&["a", "b"]);
            data.truncate(50);
            data
        }),
    );
    add(
        "white_point_of_bad_type".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
            .tag(b"wtpt", gamma(1.0))
            .shaper(&gamma(2.2)),
    );
    add(
        "white_point_cut".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
            .tag(b"wtpt", vec![b'X', b'Y', b'Z', b' ', 0, 0, 0, 0, 0, 1])
            .shaper(&gamma(2.2)),
    );

    // LUTs of each type, version and PCS, with a matrix where the type has one.
    let scaled = [0.9, 0.05, 0.0, 0.0, 0.9, 0.05, 0.05, 0.0, 0.9];
    let curve16 = table(256, |x| x.powf(1.2));
    for (version_name, version) in [("v2", V2), ("v4", V4)] {
        for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab ")] {
            let start = || Profile::new(version, b"prtr", b"RGB ", pcs).described();
            let suffix = format!("{version_name}_{pcs_name}");
            for (fill_name, fill) in [
                ("smooth", Fill::Smooth),
                ("noise", Fill::Noise(29)),
                ("extreme", Fill::Extreme(31)),
            ] {
                add(
                    format!("mft2_{fill_name}_{suffix}"),
                    start()
                        .tag(b"A2B0", mft2(3, 3, 9, None, &curve16, fill))
                        .tag(b"B2A0", mft2(3, 3, 9, None, &curve16, fill)),
                );
                add(
                    format!("mft1_{fill_name}_{suffix}"),
                    start()
                        .tag(b"A2B0", mft1(3, 3, 9, None, fill))
                        .tag(b"B2A0", mft1(3, 3, 9, None, fill)),
                );
                let parts = LutV4 {
                    a: Some(vec![para(0, &[1.2]); 3]),
                    clut: Some((vec![9, 9, 9], fill, 2)),
                    m: Some(vec![gamma(1.1); 3]),
                    matrix: Some([
                        0.9, 0.05, 0.0, 0.0, 0.9, 0.05, 0.05, 0.0, 0.9, 0.01, 0.02, 0.03,
                    ]),
                    b: Some(vec![curv(&table(64, |x| x.powf(0.9))); 3]),
                };
                add(
                    format!("mab_{fill_name}_{suffix}"),
                    start()
                        .tag(b"A2B0", lut_v4(b"mAB ", 3, 3, &parts))
                        .tag(b"B2A0", lut_v4(b"mBA ", 3, 3, &parts)),
                );
            }
            add(
                format!("mft2_matrix_{suffix}"),
                start()
                    .tag(b"A2B0", mft2(3, 3, 5, Some(scaled), &[], Fill::Smooth))
                    .tag(b"B2A0", mft2(3, 3, 5, Some(scaled), &curve16, Fill::Smooth)),
            );
            add(
                format!("mft2_no_clut_{suffix}"),
                start()
                    .tag(b"A2B0", mft2(3, 3, 0, None, &curve16, Fill::Smooth))
                    .tag(b"B2A0", mft2(3, 3, 0, None, &curve16, Fill::Smooth)),
            );
            add(
                format!("mft1_matrix_{suffix}"),
                start()
                    .tag(b"A2B0", mft1(3, 3, 5, Some(scaled), Fill::Smooth))
                    .tag(b"B2A0", mft1(3, 3, 5, Some(scaled), Fill::Noise(37))),
            );
            add(
                format!("only_a2b0_{suffix}"),
                start().tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Smooth)),
            );
            add(
                format!("only_b2a0_{suffix}"),
                start().tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Smooth)),
            );
            add(
                format!("a2b1_and_b2a1_{suffix}"),
                start()
                    .tag(b"A2B1", mft2(3, 3, 5, None, &[], Fill::Smooth))
                    .tag(b"B2A1", mft2(3, 3, 5, None, &[], Fill::Noise(41))),
            );
            add(
                format!("lut_beats_shaper_{suffix}"),
                start()
                    .shaper(&gamma(2.2))
                    .tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Smooth))
                    .tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Smooth)),
            );
            add(
                format!("both_intents_{suffix}"),
                start()
                    .shaper(&gamma(2.2))
                    .tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Noise(43)))
                    .tag(b"A2B1", mft2(3, 3, 5, None, &[], Fill::Smooth))
                    .tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Noise(47)))
                    .tag(b"B2A1", mft2(3, 3, 5, None, &[], Fill::Smooth)),
            );

            // Each structure of the v4 types.
            let curves = |value: f64| Some(vec![gamma(value); 3]);
            let matrix = Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
            let clut = |precision: u8| Some((vec![5, 3, 2], Fill::Noise(53), precision));
            let structures = [
                (
                    "b",
                    LutV4 {
                        b: curves(1.1),
                        ..LutV4::default()
                    },
                ),
                (
                    "m_matrix_b",
                    LutV4 {
                        m: curves(1.2),
                        matrix,
                        b: curves(1.1),
                        ..LutV4::default()
                    },
                ),
                (
                    "a_clut_b",
                    LutV4 {
                        a: curves(1.3),
                        clut: clut(2),
                        b: curves(1.1),
                        ..LutV4::default()
                    },
                ),
                (
                    "a_clut_b_8",
                    LutV4 {
                        a: curves(1.3),
                        clut: clut(1),
                        b: curves(1.1),
                        ..LutV4::default()
                    },
                ),
                (
                    "bad_precision",
                    LutV4 {
                        a: curves(1.3),
                        clut: clut(3),
                        b: curves(1.1),
                        ..LutV4::default()
                    },
                ),
                (
                    "too_few_curves",
                    LutV4 {
                        b: Some(vec![gamma(1.1); 2]),
                        ..LutV4::default()
                    },
                ),
            ];
            for (name, parts) in structures {
                add(
                    format!("mab_{name}_{suffix}"),
                    start()
                        .tag(b"A2B0", lut_v4(b"mAB ", 3, 3, &parts))
                        .tag(b"B2A0", lut_v4(b"mBA ", 3, 3, &parts)),
                );
            }
        }
    }

    // An offset of 2^31 and over: `fseek` takes a `long`, of 32 bits on Windows.
    let far = {
        let parts = LutV4 {
            b: Some(vec![gamma(1.1); 3]),
            ..LutV4::default()
        };
        let mut tag = lut_v4(b"mAB ", 3, 3, &parts);
        tag[12..16].copy_from_slice(&0x8000_0000u32.to_be_bytes());
        tag
    };
    add(
        "offset_past_a_long".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"Lab ")
            .described()
            .shaper(&gamma(2.2))
            .tag(b"A2B0", far.clone())
            .tag(b"B2A0", far),
    );

    // A float tag beats the rest; each element, on one side and both.
    for (name, elements) in mpet_kinds() {
        for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab ")] {
            let tag = mpet(3, 3, &elements);
            add(
                format!("mpet_{name}_{pcs_name}"),
                Profile::new(V4, b"mntr", b"RGB ", pcs)
                    .described()
                    .tag(b"D2B0", tag.clone())
                    .tag(b"B2D0", tag),
            );
        }
    }
    add(
        "mpet_only_d2b1".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
            .described()
            .tag(b"D2B1", mpet(3, 3, &[matf(3, 3, &[1.0; 12])]))
            .shaper(&gamma(2.2)),
    );
    add(
        "mpet_wrong_channels".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
            .described()
            .tag(b"D2B0", mpet(4, 3, &[matf(3, 3, &[1.0; 12])])),
    );
    add(
        "mpet_empty".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
            .described()
            .tag(b"D2B0", mpet(3, 3, &[])),
    );

    // Device classes and spaces, each with what it can use.
    for class in [
        b"scnr", b"mntr", b"prtr", b"spac", b"link", b"abst", b"nmcl",
    ] {
        let class_name = String::from_utf8_lossy(class).into_owned();
        for (space_name, space) in [
            ("rgb", b"RGB "),
            ("lab", b"Lab "),
            ("xyz", b"XYZ "),
            ("gray", b"GRAY"),
            ("cmyk", b"CMYK"),
            ("cmy", b"CMY "),
            ("mch3", b"3CLR"),
        ] {
            for (pcs_name, pcs) in [("xyz", b"XYZ "), ("lab", b"Lab "), ("rgb", b"RGB ")] {
                for (version_name, version) in [("v2", V2), ("v4", V4)] {
                    add(
                        format!("{class_name}_{space_name}_{pcs_name}_{version_name}"),
                        Profile::new(version, class, space, pcs)
                            .described()
                            .shaper(&gamma(2.2))
                            .tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Smooth))
                            .tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Smooth)),
                    );
                }
            }
        }
        add(
            format!("{class_name}_rgb_lab_mpet"),
            Profile::new(V4, class, b"RGB ", b"Lab ")
                .tag(b"D2B0", mpet(3, 3, &[matf(3, 3, &[0.9; 12])]))
                .tag(b"B2D0", mpet(3, 3, &[matf(3, 3, &[0.2; 12])])),
        );
        add(
            format!("{class_name}_lab_rgb_mpet"),
            Profile::new(V4, class, b"Lab ", b"RGB ")
                .tag(b"D2B0", mpet(3, 3, &[matf(3, 3, &[0.9; 12])]))
                .tag(b"B2D0", mpet(3, 3, &[matf(3, 3, &[0.2; 12])])),
        );
    }
    add(
        "link_rgb_lab_no_lut".into(),
        Profile::new(V4, b"link", b"RGB ", b"Lab ").described(),
    );
    add(
        "link_rgb_lab_d2b1".into(),
        Profile::new(V4, b"link", b"RGB ", b"Lab ")
            .tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Smooth))
            .tag(b"clot", clrt(&["a"])),
    );
    add(
        "cmyk_4_channel_lut".into(),
        Profile::new(V2, b"prtr", b"CMYK", b"Lab ")
            .described()
            .tag(b"A2B0", mft2(4, 3, 3, None, &[], Fill::Smooth))
            .tag(b"B2A0", mft2(3, 4, 3, None, &[], Fill::Smooth)),
    );
    add(
        "gray_shaper".into(),
        Profile::new(V2, b"mntr", b"GRAY", b"XYZ ")
            .described()
            .tag(b"kTRC", gamma(2.2)),
    );
    add(
        "version_5".into(),
        Profile::new(0x05000000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "version_past_5".into(),
        Profile::new(0x05100000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "version_5_tenths_out_of_range".into(),
        Profile::new(0x050F0000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "version_5_hundredths_out_of_range".into(),
        Profile::new(0x05F00000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "version_major_out_of_range".into(),
        Profile::new(0x0C000000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "version_digits_out_of_range".into(),
        Profile::new(0x4FF00000, b"mntr", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "no_class".into(),
        Profile::new(V4, &[0; 4], b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "bad_class".into(),
        Profile::new(V4, b"nope", b"RGB ", b"XYZ ").shaper(&gamma(2.2)),
    );
    add(
        "no_tags".into(),
        Profile::new(V4, b"mntr", b"RGB ", b"XYZ "),
    );

    // Tags that share a block: read as one, if their types agree.
    let mut shared = Profile::new(V4, b"mntr", b"RGB ", b"XYZ ")
        .tag(b"rTRC", gamma(1.8))
        .tag(b"gTRC", gamma(1.8))
        .colorants()
        .tag(b"A2B0", mft2(3, 3, 5, None, &[], Fill::Noise(59)))
        .tag(b"B2A0", mft2(3, 3, 5, None, &[], Fill::Noise(61)));
    shared.tags.push((*b"bTRC", gamma(2.0)));
    let bytes = shared.bytes();
    all.push(("linked_copy_source".into(), bytes.clone()));
    // The directory: 3 fields per tag after the count, and bTRC is the last. Make it point at gTRC.
    let last = 132 + 12 * (shared.tags.len() - 1);
    let mut linked = bytes.clone();
    let green = 132 + 12;
    let block: [u8; 8] = linked[green + 4..green + 12].try_into().unwrap();
    linked[last + 4..last + 12].copy_from_slice(&block);
    all.push(("linked_trc".into(), linked));
    let mut duplicate = bytes.clone();
    duplicate[last..last + 4].copy_from_slice(b"gTRC");
    all.push(("duplicate_tag".into(), duplicate));
    let mut too_many = bytes;
    too_many[128..132].copy_from_slice(&101u32.to_be_bytes());
    all.push(("too_many_tags".into(), too_many));

    all
}

/// `bytes` with a defect, chosen by `rng`.
pub(crate) fn corrupt(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let count = u32::from_be_bytes(out[128..132].try_into().unwrap()) as usize;
    let count = count.min(out.len().saturating_sub(132) / 12);
    let word = |out: &mut Vec<u8>, at: usize, value: u32| {
        if at + 4 <= out.len() {
            out[at..at + 4].copy_from_slice(&value.to_be_bytes());
        }
    };
    match rng.below(8) {
        0 => {
            let length = rng.below(out.len());
            out.truncate(length);
        }
        1 => {
            for _ in 0..1 + rng.below(30) {
                let at = rng.below(out.len());
                out[at] = rng.next() as u8;
            }
        }
        2 if count > 0 => {
            let entry = 132 + 12 * rng.below(count);
            let field = entry + [0, 4, 8][rng.below(3)];
            let length = out.len() as u32;
            let values = [
                0,
                1,
                7,
                8,
                128,
                length,
                length / 2,
                0x7FFFFFFF,
                0x80000000,
                0xFFFFFFFF,
                rng.next() as u32,
            ];
            let value = values[rng.below(values.len())];
            word(&mut out, field, value);
        }
        3 if count > 0 => {
            let entry = 132 + 12 * rng.below(count);
            let offset = u32::from_be_bytes(out[entry + 4..entry + 8].try_into().unwrap());
            let signatures: [&[u8; 4]; 11] = [
                b"curv", b"para", b"mft1", b"mft2", b"mAB ", b"mBA ", b"mpet", b"XYZ ", b"clrt",
                b"sf32", &[0; 4],
            ];
            let signature = signatures[rng.below(signatures.len())];
            if offset as usize + 4 <= out.len() {
                out[offset as usize..offset as usize + 4].copy_from_slice(signature);
            }
        }
        4 if count > 0 => {
            let entry = 132 + 12 * rng.below(count);
            let offset = u32::from_be_bytes(out[entry + 4..entry + 8].try_into().unwrap()) as usize;
            let size = u32::from_be_bytes(out[entry + 8..entry + 12].try_into().unwrap()) as usize;
            for _ in 0..1 + rng.below(12) {
                if size > 8 && offset + size <= out.len() {
                    let at = offset + 8 + rng.below((size - 8).min(64));
                    let values = [0, 1, 2, 3, 15, 16, 17, 255, rng.next() as u8];
                    out[at] = values[rng.below(values.len())];
                }
            }
        }
        5 => {
            let classes: [&[u8; 4]; 7] = [
                b"scnr", b"mntr", b"prtr", b"link", b"abst", b"spac", b"nmcl",
            ];
            let spaces: [&[u8; 4]; 5] = [b"RGB ", b"GRAY", b"CMYK", b"Lab ", b"XYZ "];
            let pcss: [&[u8; 4]; 3] = [b"XYZ ", b"Lab ", b"RGB "];
            out[12..16].copy_from_slice(classes[rng.below(classes.len())]);
            out[16..20].copy_from_slice(spaces[rng.below(spaces.len())]);
            out[20..24].copy_from_slice(pcss[rng.below(pcss.len())]);
        }
        6 if count > 1 => {
            let a = 132 + 12 * rng.below(count);
            let b = 132 + 12 * rng.below(count);
            for i in 0..4 {
                out.swap(a + i, b + i);
            }
        }
        _ => {
            let at = [0, 8, 9, 36, 64, 128, 131][rng.below(7)];
            out[at] = rng.next() as u8;
        }
    }
    out
}
