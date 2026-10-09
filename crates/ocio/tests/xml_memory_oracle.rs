// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! expat's allocation tracker against the wheel: where each family of documents starts being
//! refused ("XML parsing error: out of memory").
//!
//! The tracker refuses an allocation once expat's heap use reaches 64 MiB and is more than
//! 100 times the document's direct bytes. Each family is a CC file whose parameter moves its
//! heap use (or its direct bytes) in steps of one or two bytes; the port finds the first
//! refused parameter by itself, and the wheel must accept the document just below it and
//! refuse that one. So the port counts what the wheel's expat allocates, byte for byte (the
//! block headers, the pools' blocks, the hash tables, the tags' buffers, the attribute arrays,
//! the open entities, the default attributes, the parser's own structure) on this platform.
//!
//! The documents expand two entities (`k`, 1000 bytes; `m`, a million) into the root's
//! attributes: the first value, of `x` bytes, grows the temporary pool's first block to
//! 32 MiB; the second, of 20 MB, starts a block of twice the space left after the first, so
//! the heap use crosses 64 MiB at an `x` near 16.78 MB, and moves by 2 bytes per byte of `x`.
//! A comment of 500 KB keeps the billion laughs protection quiet while the tracker's
//! amplification stays above 100. The families add the structures each counts after that
//! start tag: attribute names, entities, default attributes, nested tags, element types.

mod common;

use common::lut_files::{file, file_case, write_files};
use common::transforms::{Case, check_processors};
use ocio::fileformats::file_format_cc::LocalFileFormat;
use ocio::fileformats::input_stream::InputStream;
use ocio::{Interpolation, TransformDirection};

/// The entities the values are made of: `k`, 1000 bytes, and `m`, 1000 `k`s.
fn entities() -> String {
    format!(
        "<!ENTITY k \"{}\">\n<!ENTITY m \"{}\">\n",
        "A".repeat(1000),
        "&k;".repeat(1000)
    )
}

/// An attribute value of exactly `n` bytes, from references to `m` and `k`.
fn value(n: usize) -> String {
    format!(
        "{}{}{}",
        "&m;".repeat(n / 1_000_000),
        "&k;".repeat((n % 1_000_000) / 1000),
        "A".repeat(n % 1000)
    )
}

/// The end of the CC.
const TAIL: &str = "<SOPNode>\n<Slope>1 1 1</Slope>\n<Offset>0 0 0</Offset>\n<Power>1 1 1</Power>\n\
                    </SOPNode>\n</ColorCorrection>\n";

/// A document of the activation family: the root's values of `x` and `v2` bytes, a comment of
/// `pad` bytes before, and `dtd_extra` and `after` to add structures.
fn base(x: usize, pad: usize, v2: usize, dtd_extra: &str, after: &str) -> Vec<u8> {
    format!(
        "<!-- <ColorCorrection -->\n<!--{}-->\n<!DOCTYPE ColorCorrection [\n{}{dtd_extra}]>\n\
         <ColorCorrection id=\"{}\" name=\"{}\">\n<t>\n</t>\n{after}{TAIL}",
        "P".repeat(pad),
        entities(),
        value(x),
        value(v2)
    )
    .into_bytes()
}

/// A family: its name, the document of a parameter, and the range the first refused
/// parameter is in (the first accepted, the last refused, or the other way round).
struct Family {
    name: &'static str,
    doc: Box<dyn Fn(usize) -> Vec<u8>>,
    range: (usize, usize),
}

/// The families.
fn families() -> Vec<Family> {
    const V2: usize = 20_000_000;
    const PAD: usize = 500_000;
    const X: (usize, usize) = (16_777_216, 16_900_000);
    vec![
        Family {
            name: "the root's values alone",
            doc: Box::new(|x| base(x, PAD, V2, "", "")),
            range: X,
        },
        Family {
            name: "40 attribute names",
            doc: Box::new(|x| {
                let atts: Vec<String> = (0..40).map(|i| format!("a{i}=\"v\"")).collect();
                base(x, PAD, V2, "", &format!("<e {}/>\n", atts.join(" ")))
            }),
            range: X,
        },
        Family {
            name: "100 entities",
            doc: Box::new(|x| {
                let ents: String = (0..100)
                    .map(|i| format!("<!ENTITY e{i} \"text{i}\">\n"))
                    .collect();
                base(x, PAD, V2, &ents, "")
            }),
            range: X,
        },
        Family {
            name: "20 default attributes of a prefixed element type",
            doc: Box::new(|x| {
                let decls: String = (0..20)
                    .map(|i| format!("<!ATTLIST p:e d{i} CDATA \"value{i}\">\n"))
                    .collect();
                base(x, PAD, V2, &decls, "<p:e/>\n")
            }),
            range: X,
        },
        Family {
            name: "30 nested tags",
            doc: Box::new(|x| {
                let opens: String = (0..30).map(|i| format!("<n{i}>\n")).collect();
                let closes: String = (0..30).rev().map(|i| format!("</n{i}>\n")).collect();
                base(x, PAD, V2, "", &format!("{opens}{closes}"))
            }),
            range: X,
        },
        Family {
            name: "70 element types",
            doc: Box::new(|x| {
                let elts: String = (0..70).map(|i| format!("<el{i}/>\n")).collect();
                base(x, PAD, V2, "", &elts)
            }),
            range: X,
        },
        Family {
            name: "a tag name of Latin-1 characters (odd steps)",
            doc: Box::new(|n| {
                // The name's characters are one byte in the file and two in UTF-8: its buffer
                // holds 3n + 3 bytes. Its values sit just above the threshold's (fewer bytes of
                // the second value's block than the threshold needs).
                let mut doc = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n".to_vec();
                let mut rest = base(16_783_700, PAD, V2, "", "");
                let at = rest
                    .windows(8)
                    .position(|w| w == b"<t>\n</t>")
                    .expect("the inner tag");
                let name: Vec<u8> = std::iter::once(b't')
                    .chain(std::iter::repeat_n(0xE9u8, n))
                    .collect();
                let tag = [b"<".as_slice(), &name, b">\n</", &name, b">"].concat();
                rest.splice(at..at + 8, tag);
                doc.extend_from_slice(&rest);
                doc
            }),
            range: (0, 60),
        },
        Family {
            name: "two nested tags (the other parity)",
            doc: Box::new(|x| base(x, PAD, V2, "", "<u>\n<v>\n</v>\n</u>\n")),
            range: X,
        },
        Family {
            name: "the billion laughs protection's amplification",
            doc: Box::new(|n| billion_laughs(n, 100_000)),
            range: (5_000_000, 20_000_000),
        },
        Family {
            name: "the billion laughs protection's activation (8 MiB)",
            doc: Box::new(|n| billion_laughs(n, 50_000)),
            range: (5_000_000, 9_000_000),
        },
        Family {
            name: "the amplification (direct bytes)",
            doc: Box::new(|pad| {
                format!(
                    "<!-- <ColorCorrection -->\n<!--{}-->\n<!DOCTYPE ColorCorrection [\n{}]>\n\
                     <ColorCorrection id=\"{}\">\n{TAIL}",
                    "P".repeat(pad),
                    entities(),
                    value(67_200_000)
                )
                .into_bytes()
            }),
            range: (600_000, 1_400_000),
        },
    ]
}

/// Whether the port refuses the document for one of expat's limits (the CC reader: "out of
/// memory", or the billion laughs protection's "limit on input amplification factor").
fn port_refuses(doc: &[u8]) -> bool {
    let mut stream = InputStream::from_bytes(doc.to_vec());
    match LocalFileFormat.read_file(&mut stream, b"doc.cc") {
        Ok(_) => false,
        Err(e) => {
            let message = String::from_utf8_lossy(e.what()).into_owned();
            let limit = message.contains("out of memory")
                || message.contains("limit on input amplification factor");
            assert!(limit, "an unexpected error: {message}");
            limit
        }
    }
}

/// Character data of exactly `n` bytes, from references to `m`, `k` and `c` (one byte).
fn content(n: usize) -> String {
    format!(
        "{}{}{}",
        "&m;".repeat(n / 1_000_000),
        "&k;".repeat((n % 1_000_000) / 1000),
        "&c;".repeat(n % 1000)
    )
}

/// A document of `n` bytes of character data expanded from entities after a comment of
/// `pad` bytes: the billion laughs protection.
fn billion_laughs(n: usize, pad: usize) -> Vec<u8> {
    format!(
        "<!-- <ColorCorrection -->\n<!--{}-->\n<!DOCTYPE ColorCorrection [\n{}<!ENTITY c \"C\">\n]>\n\
         <ColorCorrection id=\"a\">\n<Description>{}</Description>\n{TAIL}",
        "P".repeat(pad),
        entities(),
        content(n)
    )
    .into_bytes()
}

/// The first parameter of `range` whose document the port refuses (or accepts, when it
/// refuses the first one).
fn port_boundary(family: &Family) -> usize {
    let (mut a, mut b) = family.range;
    let ra = port_refuses(&(family.doc)(a));
    let rb = port_refuses(&(family.doc)(b));
    assert_ne!(ra, rb, "{}: no boundary in the range", family.name);
    while a.abs_diff(b) > 1 {
        let m = (a + b) / 2;
        if port_refuses(&(family.doc)(m)) == rb {
            b = m;
        } else {
            a = m;
        }
    }
    b
}

/// The port's boundary of each family, and the wheel's processors of the documents on both
/// sides of it (the error text included) against the port's.
#[test]
fn the_tracker_refuses_where_the_wheel_does() {
    let mut cases: Vec<Case> = Vec::new();
    for family in families() {
        let boundary = port_boundary(&family);
        let before = if family.range.0 < family.range.1 {
            boundary - 1
        } else {
            boundary + 1
        };
        for p in [before, boundary] {
            let dir = write_files(&[file("doc.cc", (family.doc)(p))]);
            cases.push(file_case(
                format!("{} ({p})", family.name),
                &format!("{dir}/doc.cc"),
                Interpolation::Default,
                TransformDirection::Forward,
            ));
        }
    }
    check_processors(&cases);
}
