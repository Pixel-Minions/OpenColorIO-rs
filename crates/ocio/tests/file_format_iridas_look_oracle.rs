// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Iridas `.look` reader against the wheel (`processor_ops`): the processors of file
//! transforms of generated files, both directions of the processor and of the transform, the
//! LUT by its bits (NaNs, infinities, signed zeros and subnormals among the hex floats); the
//! XML it reads and refuses (sizes, data, the elements it skips, masks, malformed and hostile
//! documents); and what it logs for the interpolations a 3D LUT doesn't take.

mod common;

use common::lut_files::{LOGGING, check_file_processors, file, file_case, write_files};
use common::transforms::Case;
use ocio::{Interpolation, TransformDirection};

/// The case of a file `lut.look` holding `bytes`, read with `interp`. The set holds the case's
/// label too: the file cache keeps the first read of a path, and each read takes its
/// interpolation, so no two cases share a path.
fn case_with(label: &str, bytes: &[u8], interp: Interpolation, dir: TransformDirection) -> Case {
    let dir_path = write_files(&[file("lut.look", bytes), file("label.txt", label)]);
    file_case(label, &format!("{dir_path}/lut.look"), interp, dir)
}

fn case(label: &str, bytes: &[u8]) -> Case {
    case_with(
        label,
        bytes,
        Interpolation::Default,
        TransformDirection::Forward,
    )
}

/// `value`'s bytes in the file's hex: little endian, upper case.
fn hex(value: f32) -> String {
    value
        .to_le_bytes()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

/// The hex of a 3D LUT of `size` entries per side, red fastest, 8 entries per line.
fn data(size: usize) -> Vec<String> {
    let s = (size - 1) as f32;
    let mut values = Vec::new();
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let (rf, gf, bf) = (r as f32 / s, g as f32 / s, b as f32 / s);
                values.push(hex(0.9 * rf + 0.05 * gf));
                values.push(hex(0.8 * gf + 0.1 * bf * bf));
                values.push(hex(0.7 * bf + 0.2 * rf * gf));
            }
        }
    }
    values.chunks(8).map(|c| c.concat()).collect()
}

/// A look file: the shaders, then a LUT of the `size` element's text and the `data` lines.
fn look(size: &str, data_lines: &[String]) -> String {
    let mut text = String::from(
        "<?xml version=\"1.0\" ?>\n<look>\n  <shaders>\n    <base>\n      \
         <visible>\"1\"</visible>\n    </base>\n  </shaders>\n  <LUT>\n",
    );
    text.push_str(&format!("    <size>{size}</size>\n    <data>\"\n"));
    for line in data_lines {
        text.push_str(&format!("      {line}\n"));
    }
    text.push_str("    \"</data>\n  </LUT>\n</look>\n");
    text
}

/// A look file of the root's `body` lines.
fn root(body: &[&str]) -> String {
    let mut text = String::from("<?xml version=\"1.0\" ?>\n<look>\n");
    for line in body {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("</look>\n");
    text
}

/// Files the reader takes and refuses, generated.
fn generated_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |label: &str, text: String| cases.push(case(label, text.as_bytes()));
    let d2 = data(2);
    let d2_one_line = d2.concat();

    // LUTs of every kind of size.
    add("2 per side", look("\"2\"", &d2));
    add("5 per side", look("\"5\"", &data(5)));
    add("8 per side", look("\"8\"", &data(8)));
    add("17 per side", look("\"17\"", &data(17)));
    add(
        "the data on one line",
        look("\"2\"", std::slice::from_ref(&d2_one_line)),
    );
    add(
        "lower-case hex",
        look("\"2\"", &[d2_one_line.to_ascii_lowercase()]),
    );
    add(
        "the data split in quoted pieces",
        look(
            "\"2\"",
            &[format!("{}\" '{}", &d2_one_line[..20], &d2_one_line[20..])],
        ),
    );
    add(
        "spaces inside a value",
        look(
            "\"2\"",
            &[format!("{} {}", &d2_one_line[..4], &d2_one_line[4..])],
        ),
    );
    add(
        "tabs in the data",
        look(
            "\"2\"",
            &[format!("{}\t{}", &d2_one_line[..8], &d2_one_line[8..])],
        ),
    );
    add("CRLF line ends", look("\"2\"", &d2).replace('\n', "\r\n"));
    add("CR line ends", look("\"2\"", &d2).replace('\n', "\r"));
    add(
        "no final line feed",
        look("\"2\"", &d2).trim_end().to_owned(),
    );

    // Special floats among the values.
    for (label, bits) in [
        ("NaN", 0x7fc0_0000u32),
        ("-NaN", 0xffc0_0000),
        ("a signaling NaN", 0x7f80_0001),
        ("a NaN payload", 0x7fc1_2345),
        ("infinity", 0x7f80_0000),
        ("-infinity", 0xff80_0000),
        ("-0", 0x8000_0000),
        ("a subnormal", 0x0000_0001),
        ("the largest float", 0x7f7f_ffff),
    ] {
        let mut values = d2_one_line.clone();
        values.replace_range(24..32, &hex(f32::from_bits(bits)));
        add(&format!("a value of {label}"), look("\"2\"", &[values]));
    }

    // The size.
    for (label, size) in [
        ("unquoted", "2"),
        ("single quotes", "'2'"),
        ("spaces", "  \" 2 \"  "),
        ("a suffix", "\"2x\""),
        ("a sign", "\"+2\""),
        ("hex", "\"0x2\""),
        ("an octal look", "\"02\""),
        ("1", "\"1\""),
        ("0", "\"0\""),
        ("-2", "\"-2\""),
        ("129", "\"129\""),
        ("130", "\"130\""),
        ("past a 32-bit long", "\"4294967298\""),
        ("past a 64-bit long", "\"99999999999999999999\""),
        ("empty", ""),
        ("quotes only", "\"\""),
        ("not a number", "\"two\""),
        ("a float", "\"2.0\""),
    ] {
        add(&format!("a size of {label}"), look(size, &d2));
    }
    add("a size on two lines", look("\"\n2\"", &d2));
    add(
        "a size split by spaces on its own line",
        look("\n  \"2\"\n  ", &d2),
    );
    add("a size by a character reference", look("\"&#50;\"", &d2));
    // The size is read from each piece of text expat delivers (I-195).
    add("a pretty-printed size", look("\n      \"2\"\n    ", &d2));
    add(
        "a size split by a comment",
        look("\"1<!-- -->7\"", &data(17)),
    );
    add("a size in quote references", look("&quot;2&quot;", &d2));
    add(
        "a size in apostrophe references",
        look("&apos;2&apos;", &d2),
    );
    // A zero-padded size: 10 on Windows, 8 on Linux (I-185).
    add("a size of 010, 10 per side", look("\"010\"", &data(10)));
    add("a size of 010, 8 per side", look("\"010\"", &data(8)));
    add(
        "two size elements",
        root(&[
            "<LUT>",
            "<size>\"3\"</size>",
            "<size>\"2\"</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "no size",
        root(&["<LUT>", "<data>", &d2_one_line, "</data>", "</LUT>"]),
    );
    add("no size and no data", root(&["<LUT>", "</LUT>"]));
    add("no LUT", root(&["<shaders/>"]));
    add("an empty look", "<look/>\n".into());

    // The data.
    add(
        "a value missing",
        look("\"2\"", &[d2_one_line[8..].to_owned()]),
    );
    add(
        "a value too many",
        look("\"2\"", &[format!("{d2_one_line}0000803F")]),
    );
    add(
        "a character missing",
        look("\"2\"", &[d2_one_line[1..].to_owned()]),
    );
    add(
        "a character that isn't hex",
        look(
            "\"2\"",
            &[format!("{}G{}", &d2_one_line[..13], &d2_one_line[14..])],
        ),
    );
    add(
        "a hex prefix",
        look("\"2\"", &[format!("0x{}", &d2_one_line[2..])]),
    );
    // Only spaces, quotes and line feeds are removed from the data (I-196).
    add(
        "a carriage return reference in the data",
        look(
            "\"2\"",
            &[format!("{}&#13;{}", &d2_one_line[..16], &d2_one_line[16..])],
        ),
    );
    add(
        "a tab reference in the data",
        look(
            "\"2\"",
            &[format!("{}&#9;{}", &d2_one_line[..16], &d2_one_line[16..])],
        ),
    );
    add(
        "a line feed reference in the data",
        look(
            "\"2\"",
            &[format!("{}&#10;{}", &d2_one_line[..16], &d2_one_line[16..])],
        ),
    );
    add(
        "tab-indented data lines",
        look("\"2\"", &d2).replace("\n      ", "\n\t"),
    );
    add(
        "the data by character references",
        look("\"2\"", &[d2_one_line.replacen('0', "&#48;", 3)]),
    );
    add(
        "the data in CDATA",
        look("\"2\"", &[format!("<![CDATA[{d2_one_line}]]>")]),
    );
    add(
        "a comment in the data",
        look(
            "\"2\"",
            &[format!(
                "{}<!-- c -->{}",
                &d2_one_line[..16],
                &d2_one_line[16..]
            )],
        ),
    );
    add(
        "an element in the data",
        look(
            "\"2\"",
            &[format!("{}<x/>{}", &d2_one_line[..16], &d2_one_line[16..])],
        ),
    );
    add(
        "the data before the size",
        root(&[
            "<LUT>",
            "<data>",
            &d2_one_line,
            "</data>",
            "<size>2</size>",
            "</LUT>",
        ]),
    );
    add(
        "two data elements",
        root(&[
            "<LUT>",
            "<size>2</size>",
            "<data>",
            &d2_one_line[..96],
            "</data>",
            "<data>",
            &d2_one_line[96..],
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "two LUTs",
        root(&[
            "<LUT>",
            "<size>2</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
            "<LUT>",
            "<size>2</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );

    // The elements.
    add(
        "an unknown element in the LUT",
        root(&[
            "<LUT>",
            "<size>2</size>",
            "<other>x</other>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "a LUT1D section",
        root(&[
            "<LUT1D><size>\"2\"</size><data>\"0000803F\"</data></LUT1D>",
            "<LUT>",
            "<size>2</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "a size and data in the shaders",
        root(&[
            "<shaders><size>\"9\"</size><data>\"00\"</data></shaders>",
            "<LUT>",
            "<size>2</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "a size element ended by another",
        root(&["<LUT>", "<size>2</data>", "</LUT>"]),
    );
    add(
        "a size inside the data",
        root(&[
            "<LUT>",
            "<data>",
            "<size>2</size>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );
    add(
        "a data element inside the size",
        root(&["<LUT>", "<size>2<data>00</data></size>", "</LUT>"]),
    );
    add("a LUT inside a LUT", root(&["<LUT><LUT></LUT></LUT>"]));
    add(
        "attributes",
        root(&[
            "<LUT a=\"1\">",
            "<size b='2'>2</size>",
            "<data c=\"3\">",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );

    // The root.
    add(
        "another root",
        look("\"2\"", &d2).replace("look>", "looks>"),
    );
    add("a look in a look", root(&["<look></look>"]));
    add(
        "a LUT outside the look",
        format!("<LUT><size>2</size><data>{d2_one_line}</data></LUT>\n"),
    );
    add(
        "text after the look",
        format!("{}trailing\n", look("\"2\"", &d2)),
    );
    add(
        "a second root after the look",
        format!("{}<look/>\n", look("\"2\"", &d2)),
    );
    add(
        "no XML declaration",
        look("\"2\"", &d2).replacen("<?xml version=\"1.0\" ?>\n", "", 1),
    );
    add(
        "a byte-order mark",
        format!("\u{feff}{}", look("\"2\"", &d2)),
    );

    // Masks.
    let lut_lines = [
        "<LUT>",
        "<size>2</size>",
        "<data>",
        &d2_one_line,
        "</data>",
        "</LUT>",
    ];
    add(
        "an empty mask",
        root(&[&["<mask></mask>"], &lut_lines[..]].concat()),
    );
    add(
        "a mask with text",
        root(&[&["<mask>text</mask>"], &lut_lines[..]].concat()),
    );
    add(
        "a mask with an element",
        root(&[&["<mask><name>x</name></mask>"], &lut_lines[..]].concat()),
    );
    add(
        "an empty mask, then shaders with an element",
        root(&[&["<mask/>", "<shaders><base/></shaders>"], &lut_lines[..]].concat()),
    );
    add(
        "an empty mask after the LUT",
        root(&[&lut_lines[..], &["<mask/>"]].concat()),
    );
    add(
        "an empty mask in the shaders",
        root(&[&["<shaders><mask/></shaders>"], &lut_lines[..]].concat()),
    );
    add(
        "a mask in the LUT",
        root(&[
            "<LUT>",
            "<mask><x/></mask>",
            "<size>2</size>",
            "<data>",
            &d2_one_line,
            "</data>",
            "</LUT>",
        ]),
    );

    // Malformed and hostile XML.
    add("empty", String::new());
    add("white space only", " \n\t\n".into());
    add("the declaration only", "<?xml version=\"1.0\" ?>\n".into());
    add(
        "unbalanced tags",
        root(&["<LUT>", "<size>2</size>", "</data>"]),
    );
    add(
        "an unclosed look",
        look("\"2\"", &d2).replace("</look>\n", ""),
    );
    add(
        "a truncated tag",
        look("\"2\"", &d2).replace("</look>\n", "</lo"),
    );
    add(
        "an undefined entity",
        look("\"2\"", &[format!("{d2_one_line}&foo;")]),
    );
    add(
        "a NUL character reference",
        look("\"2\"", &[format!("{d2_one_line}&#0;")]),
    );
    add(
        "an internal entity",
        format!(
            "<?xml version=\"1.0\" ?>\n<!DOCTYPE look [<!ENTITY v \"{d2_one_line}\">]>\n\
             <look><LUT><size>2</size><data>&v;</data></LUT></look>\n"
        ),
    );
    add(
        "an external entity",
        "<?xml version=\"1.0\" ?>\n<!DOCTYPE look [<!ENTITY v SYSTEM \"x.txt\">]>\n\
         <look><LUT><size>2</size><data>&v;</data></LUT></look>\n"
            .into(),
    );
    add(
        "an encoding the parser doesn't know",
        look("\"2\"", &d2).replacen("version=\"1.0\"", "version=\"1.0\" encoding=\"EBCDIC\"", 1),
    );
    add(
        "Latin-1 text in the shaders",
        look("\"2\"", &d2).replacen(
            "version=\"1.0\"",
            "version=\"1.0\" encoding=\"ISO-8859-1\"",
            1,
        ),
    );
    let mut bytes = look("\"2\"", &d2).into_bytes();
    let at = bytes.windows(7).position(|w| w == b"<visibl").unwrap();
    bytes.splice(at..at, *b"\xff\xfe");
    cases.push(case("invalid UTF-8", &bytes));
    let utf16: Vec<u8> = "\u{feff}<look/>"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    cases.push(case("UTF-16", &utf16));
    let mut nul = look("\"2\"", &d2).into_bytes();
    let at = nul.windows(6).position(|w| w == b"<size>").unwrap();
    nul.insert(at, 0);
    cases.push(case("a NUL byte", &nul));
    cases.push(case(
        "a line of 70000 characters",
        look("\"2\"", &[format!("{}{d2_one_line}", " ".repeat(70_000))]).as_bytes(),
    ));
    cases
}

#[test]
fn look_files_read_as_in_the_wheel() {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let mut cases = generated_cases();
    let text = look("\"3\"", &data(3));
    cases.push(case_with(
        "the inverse transform",
        text.as_bytes(),
        Interpolation::Tetrahedral,
        TransformDirection::Inverse,
    ));
    check_file_processors(&cases);
}

/// The cap on the data's characters, 129^3 * 3 * 8 (FileFormatIridasLook.cpp:451-455 @
/// v2.5.2): a LUT of 129 per side reaches it exactly, one value more passes it. The files are
/// 60 MB each, so only the full and exhaustive tiers read them.
#[test]
fn the_data_cap_reads_as_in_the_wheel() {
    if ocio_testkit::battery::Tier::current() == ocio_testkit::battery::Tier::Quick {
        return;
    }
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let values = 129 * 129 * 129 * 3;
    let cases: Vec<Case> = [values, values + 1]
        .into_iter()
        .map(|count| {
            let line = "0000803F".repeat(8);
            let mut lines = vec![line; count / 8];
            lines.push("0000003F".repeat(count % 8));
            let text = look("\"129\"", &lines);
            case(&format!("{count} values of 129 per side"), text.as_bytes())
        })
        .collect();
    check_file_processors(&cases);
}

/// The file transform's interpolation: kept when a 3D LUT takes it, else a warning, and the
/// default.
#[test]
fn interpolations_read_and_log_as_in_the_wheel() {
    let text = look("\"2\"", &data(2));
    let cases: Vec<Case> = [
        Interpolation::Unknown,
        Interpolation::Nearest,
        Interpolation::Linear,
        Interpolation::Tetrahedral,
        Interpolation::Cubic,
        Interpolation::Default,
        Interpolation::Best,
    ]
    .into_iter()
    .map(|interp| {
        case_with(
            &format!("{interp:?}"),
            text.as_bytes(),
            interp,
            TransformDirection::Forward,
        )
    })
    .collect();
    {
        let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
        check_file_processors(&cases);
    }
    common::lut_files::check_logs(&cases, TransformDirection::Forward);
    common::lut_files::check_logs(&cases, TransformDirection::Inverse);
}
