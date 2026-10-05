// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port of yaml-cpp's conversions (`as<double>`, `as<std::vector<float>>`, `as<bool>`)
//! against the wheel's, on this platform, through the oracle's `yaml_scalars` (O3.3).
//!
//! yaml-cpp reads a number with `std::stringstream >> std::noskipws` after
//! `unsetf(std::ios::dec)`, then accepts its spellings of the infinities and NaN
//! (include/yaml-cpp/node/convert.h:160-201). The C++ library doing the reading is each
//! wheel's own: MSVC's STL with the UCRT's `strtod` on Windows, libstdc++ with glibc's on
//! Linux, and they differ (hexadecimal floats, values that round to zero).
//!
//! The oracle writes each spelling into a config at a field OCIO reads as one type, loads it,
//! and reads the value back, or reports the error; it returns the config text it loaded. The
//! port loads the same text with `YAML::Load`, finds the field's node, and converts it as OCIO
//! does (`load(const YAML::Node&, T&)`, src/OpenColorIO/OCIOYaml.cpp:66-160 @ v2.5.2): the
//! value must have the same bits, or the error the same text: "Error: Loading the OCIO profile
//! failed. " (`OCIOYaml::Read`), "At line N, '<tag>' parsing <type> failed with: " and
//! yaml-cpp's `what()`.

use ocio::yaml_cpp::convert::Convert;
use ocio::yaml_cpp::exceptions::Result;
use ocio::yaml_cpp::node::Node;
use ocio::yaml_cpp::parse::load;
use ocio_testkit::fixtures::sha256_hex;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

const PREFIX: &str = "Error: Loading the OCIO profile failed. ";

/// The value a field reads, as the oracle writes it.
trait OracleValue: Convert {
    /// The type's name in OCIO's message ("parsing double failed").
    const OCIO_NAME: &'static str;
    fn to_json(&self) -> Value;
}

impl OracleValue for f64 {
    const OCIO_NAME: &'static str = "double";
    fn to_json(&self) -> Value {
        json!({"f64": self.to_bits()})
    }
}

impl OracleValue for Vec<f32> {
    const OCIO_NAME: &'static str = "vector<float>";
    fn to_json(&self) -> Value {
        Value::Array(self.iter().map(|v| json!({"f32": v.to_bits()})).collect())
    }
}

impl OracleValue for Vec<f64> {
    const OCIO_NAME: &'static str = "vector<double>";
    fn to_json(&self) -> Value {
        Value::Array(self.iter().map(|v| json!({"f64": v.to_bits()})).collect())
    }
}

impl OracleValue for bool {
    const OCIO_NAME: &'static str = "boolean";
    fn to_json(&self) -> Value {
        json!(self)
    }
}

/// OCIO's `load(node, x)`: the value, or its message (OCIOYaml.cpp:66-160).
fn ocio_load<T: OracleValue>(node: &Node) -> Result<std::result::Result<T, String>> {
    match node.as_::<T>() {
        Ok(v) => Ok(Ok(v)),
        Err(e) => Ok(Err(format!(
            "{PREFIX}At line {}, '{}' parsing {} failed with: {}",
            node.mark()?.line + 1,
            String::from_utf8_lossy(node.tag()?),
            T::OCIO_NAME,
            String::from_utf8_lossy(&e.what())
        ))),
    }
}

/// The field's node in the oracle's config: (field, path of keys and indices).
fn field_node(root: &Node, field: &str) -> Result<Node> {
    Ok(match field {
        "base" => root
            .get("colorspaces")?
            .get(0)?
            .get("to_reference")?
            .get("base")?,
        "allocationvars" => root.get("colorspaces")?.get(0)?.get("allocationvars")?,
        "isdata" => root.get("colorspaces")?.get(0)?.get("isdata")?,
        "luma" => root.get("luma")?,
        "strictparsing" => root.get("strictparsing")?,
        _ => panic!("unknown field {field}"),
    })
}

/// Compares the port with one oracle entry.
fn check<T: OracleValue>(field: &str, spelling: &str, entry: &Value) {
    let label = format!("{field}: {spelling:?}");
    let yaml = entry["yaml"]
        .as_str()
        .unwrap_or_else(|| panic!("{label}: the config text isn't UTF-8: {entry}"));
    let wheel_message = || {
        entry["exception"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{label}: the wheel loaded it: {entry}"))
    };
    let root = match load(yaml.as_bytes()) {
        Ok(root) => root,
        // a spelling that isn't a YAML value there: the parser's error
        Err(e) => {
            let what = format!("{PREFIX}{}", String::from_utf8_lossy(&e.what()));
            return assert_text_eq(&label, wheel_message(), &what);
        }
    };
    let node = field_node(&root, field).unwrap_or_else(|e| panic!("{label}: {e}"));
    // A LogTransform's base must not be a collection with elements (OCIOYaml.cpp:2889-2904).
    if field == "base" {
        let nb = node.size().unwrap_or_else(|e| panic!("{label}: {e}"));
        if nb != 0 {
            let message = format!(
                "{PREFIX}LogTransform parse error, base must be a  single double. Found {nb}."
            );
            return assert_text_eq(&label, wheel_message(), &message);
        }
    }
    // OCIO skips a key whose value is null (OCIOYaml.cpp:3445 for a color space's keys, 4491
    // for the config's, 2887 for a LogTransform's): the field keeps its default.
    if node.is_null().unwrap_or_else(|e| panic!("{label}: {e}")) {
        assert!(entry.get("value").is_some(), "{label}: {entry}");
        return;
    }
    let port = ocio_load::<T>(&node).unwrap_or_else(|e| panic!("{label}: {e}"));
    match (port, entry.get("value"), entry.get("exception")) {
        (Ok(v), Some(wheel), None) => assert_eq!(&v.to_json(), wheel, "{label}"),
        (Err(message), None, Some(wheel)) => {
            assert_text_eq(&label, wheel["message"].as_str().unwrap(), &message)
        }
        (port, _, _) => panic!(
            "{label}: the port gives {:?}, the wheel {entry}",
            port.map(|v| v.to_json())
        ),
    }
}

/// Hand-written spellings of numbers: each rule of both libraries' readers.
const NUMBERS: &[&str] = &[
    "2",
    "0",
    "-0",
    "+0",
    "+1",
    "-1",
    "0e5",
    "0.0e5",
    "1e5",
    "1E5",
    "1e+5",
    "1e-5",
    "1e05",
    "1e-0",
    "1e0001",
    "1e",
    "1e+",
    "1e-",
    "1.e1",
    ".e1",
    "0.e1",
    "e1",
    ".5",
    "5.",
    "-.5",
    "+.5",
    ".",
    "-",
    "+",
    "+-1",
    "--1",
    "00.5",
    "010",
    "0000000001",
    "0x",
    "0x1p3",
    "0x10",
    "0X1.8p1",
    "0x.8",
    "0x.0",
    "0x.",
    "-0x.0",
    "0x0",
    "0x0p5",
    "0x.0p1",
    "0x00.00",
    "0x1p-1074",
    "0x1p1024",
    "0x1.fffffffffffff8p1023",
    "0x1.fffffep127",
    "0x1p-149",
    "0x1p-150",
    "0xg",
    "0x1g",
    "0x1.",
    "0x1p",
    "0x1p+",
    "1p3",
    "1d1",
    "1_000",
    "1,5",
    "1.5.5",
    "1e5e5",
    "inf",
    "-inf",
    "nan",
    "infinity",
    ".inf",
    ".Inf",
    ".INF",
    "+.inf",
    "-.inf",
    "-.Inf",
    "-.INF",
    ".nan",
    ".NaN",
    ".NAN",
    "-.nan",
    "+.nan",
    ".iNf",
    "1e-310",
    "1e-320",
    "4.9e-324",
    "2e-324",
    "2.4703282292062327e-324",
    "2.4703282292062328e-324",
    "1e-325",
    "1e-400",
    "-1e-400",
    "-1e-310",
    "1e308",
    "1.7976931348623157e308",
    "1.7976931348623158e308",
    "1.7976931348623159e308",
    "1.8e308",
    "1e400",
    "-1e400",
    "1e9999999999999999999",
    "1e-9999999999999999999",
    "0e9999999999999999999",
    "1.17549435e-38",
    "1.4e-45",
    "1e-45",
    "7e-46",
    "1e-46",
    "3.4028235e38",
    "3.4028236e38",
    "3.5e38",
    "1e39",
    "0.1",
    "0.3",
    "0.30000000000000004",
    "1.000000000000000000000000000000001",
    "123456789012345678901234567890",
    "'1.5'",
    "\"1.5\"",
    "'0x10'",
    "\"-.inf\"",
    "!!float 1.5",
    "!foo 2",
    "1.5 ",
    "1.5  # c",
    "\"1.5 \"",
    "\"1.5\\t\"",
    "\"1.5\\v\"",
    "\"1.5\\f\"",
    "\"1.5\\r\"",
    "\"1.5\\n\"",
    "\" 1.5\"",
    "\"\\t1.5\"",
    "'1.5\n        '",
    "|\n        1.5\n",
    ">-\n        1.5\n",
    "~",
    "null",
    "",
    "[1]",
    "{a: 1}",
    "\"\"",
    "1 2",
    "1\t2",
];

/// Hand-written spellings of booleans.
const BOOLS: &[&str] = &[
    "y",
    "Y",
    "n",
    "N",
    "yes",
    "Yes",
    "YES",
    "yEs",
    "YeS",
    "no",
    "No",
    "NO",
    "nO",
    "true",
    "True",
    "TRUE",
    "tRUE",
    "TrUe",
    "false",
    "False",
    "FALSE",
    "on",
    "On",
    "ON",
    "oN",
    "off",
    "Off",
    "OFF",
    "1",
    "0",
    "t",
    "f",
    "yess",
    "'true'",
    "\"yes\"",
    "!!bool true",
    "~",
    "null",
    "",
    "[true]",
    "true ",
    "\"true \"",
    "\" true\"",
];

/// A small deterministic generator (xorshift64*), so the generated spellings never change.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random numbers: pieces of decimal and hexadecimal significands and exponents.
fn random_numbers(rng: &mut Rng, count: usize) -> Vec<String> {
    const PIECES: &[&str] = &[
        "0",
        "1",
        "5",
        "9",
        "00",
        "123",
        "999999999",
        "0000000000",
        ".",
        "e",
        "E",
        "e-",
        "e+",
        "-",
        "+",
        "0x",
        "0X",
        "p",
        "P",
        "p-",
        "a",
        "f",
        "F",
        "38",
        "39",
        "45",
        "46",
        "307",
        "308",
        "309",
        "323",
        "324",
        "325",
        "4.9",
        "2.2250738585072014",
        "1.401298464324817",
        "3.4028",
        "1.7976931348623157",
    ];
    (0..count)
        .map(|_| {
            let len = 1 + rng.below(6);
            (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect()
        })
        .collect()
}

/// Numbers with more significant digits than MSVC's `num_get` keeps (768), where the digits it
/// drops decide the rounding: the halfway points between two doubles (1 + 2^-53) and two
/// floats (1 + 2^-24), in decimal and hexadecimal, followed by zeros and a last nonzero digit
/// on both sides of the 768th significant digit; long integer parts scaled back by an
/// exponent; long runs of leading zeros against large exponents, where `num_get` clamps the
/// exponent it hands `strtod`.
fn long_numbers() -> Vec<String> {
    let zeros = |n: usize| "0".repeat(n);
    // 1 + 2^-53 and 1 + 2^-24, exactly: 54 and 25 significant digits.
    const HALF_F64: &str = "1.00000000000000011102230246251565404236316680908203125";
    const HALF_F32: &str = "1.000000059604644775390625";
    let mut out = Vec::new();
    for pad in [700, 712, 713, 714, 715, 740, 741, 742, 743, 744, 800] {
        for half in [HALF_F64, HALF_F32] {
            out.push(format!("{half}{}1", zeros(pad)));
            out.push(format!("{half}{}5", zeros(pad)));
            out.push(format!("{half}{}", zeros(pad)));
            out.push(format!("-{half}{}1e0", zeros(pad)));
        }
        // just below the halfway point, by a run of nines
        out.push(format!(
            "1.00000000000000011102230246251565404236316680908203124{}",
            "9".repeat(pad)
        ));
        // the halfway point in hexadecimal, 1 + 8 * 16^-14
        out.push(format!("0x1.00000000000008{}1p0", zeros(pad)));
        out.push(format!("0x1.000001{}1p0", zeros(pad)));
        // a long integer part, scaled back
        let digits = HALF_F64.replace('.', "");
        out.push(format!("{digits}{}1e-{}", zeros(pad), 53 + pad + 1));
        out.push(format!("0x1{}p-{}", zeros(pad), 4 * pad));
    }
    // Halfway points with exactly 768 significant digits, N * 2^-1075 for an odd N next to
    // 2^53 (N * 5^1075 * 10^-1075): rounding to even goes up for 2^53 - 1 and down for
    // 2^53 - 3, and a digit after the 768th breaks the tie.
    for n in [(1u64 << 53) - 1, (1u64 << 53) - 3] {
        // little-endian decimal digits of n * 5^1075
        let mut d: Vec<u8> = n.to_string().bytes().rev().map(|c| c - b'0').collect();
        for _ in 0..1075 {
            let mut carry = 0u8;
            for digit in d.iter_mut() {
                let v = *digit * 5 + carry;
                *digit = v % 10;
                carry = v / 10;
            }
            if carry > 0 {
                d.push(carry);
            }
        }
        let s: String = d.iter().rev().map(|&c| char::from(b'0' + c)).collect();
        out.push(format!("{s}e-1075"));
        out.push(format!("{s}1e-1076"));
        out.push(format!("{s}{}1e-1081", zeros(5)));
        out.push(format!("0.{}{s}", zeros(1075 - s.len())));
        out.push(format!("0.{}{s}1", zeros(1075 - s.len())));
        out.push(format!("0.{}{s}{}1", zeros(1075 - s.len()), zeros(5)));
    }
    for (lead, exp) in [
        (1099, "1099"),
        (1100, "1100"),
        (1101, "1101"),
        (1200, "1200"),
        (1200, "1201"),
        (1200, "99999999999999999999"),
    ] {
        out.push(format!("0.{}1e{exp}", zeros(lead)));
        out.push(format!("0.{}15e{exp}", zeros(lead)));
        out.push(format!("1{}e-{exp}", zeros(lead)));
        out.push(format!("0x0.{}1p{exp}", zeros(lead / 4)));
        out.push(format!("0x1{}p-{exp}", zeros(lead / 4)));
    }
    out
}

/// The oracle's entries for these spellings of the field.
fn entries(field: &str, spellings: &[String]) -> Vec<Value> {
    let response = Oracle::get().call(
        "yaml_scalars",
        json!({"cases": [{"field": field, "spellings": spellings}]}),
        &[],
    );
    response.result["cases"][0]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result))
        .clone()
}

#[test]
fn numbers_read_as_the_wheel_reads_them() {
    let mut rng = Rng(0x5EED_F10A_7BAD_C0DE);
    let mut spellings: Vec<String> = NUMBERS.iter().map(|s| s.to_string()).collect();
    spellings.extend(random_numbers(&mut rng, 4000));
    spellings.extend(long_numbers());

    // The generated spellings are fixed: a change to the generator must be deliberate.
    let all: Vec<u8> = spellings
        .iter()
        .flat_map(|s| [s.as_bytes(), b"\x01"].concat())
        .collect();
    assert_eq!(
        sha256_hex(&all),
        "afd72e849fc6d4c2bad33315152f85a0a2ce873c23963ea38dd7da3b8fb291a5",
        "the generated spellings changed"
    );

    // a double, as a scalar
    for (spelling, entry) in spellings.iter().zip(entries("base", &spellings)) {
        check::<f64>("base", spelling, &entry);
    }

    // floats and doubles, as list elements (flow lists, whose plain scalars end at a comma)
    let lists: Vec<String> = spellings
        .iter()
        .filter(|s| !s.contains('\n') && !s.starts_with('!'))
        .map(|s| format!("[{s}]"))
        .collect();
    for (spelling, entry) in lists.iter().zip(entries("allocationvars", &lists)) {
        check::<Vec<f32>>("allocationvars", spelling, &entry);
    }
    let lumas: Vec<String> = spellings
        .iter()
        .filter(|s| !s.contains('\n') && !s.starts_with('!'))
        .map(|s| format!("[0.25, {s}, 0.25]"))
        .collect();
    for (spelling, entry) in lumas.iter().zip(entries("luma", &lumas)) {
        // OCIO refuses a luma that doesn't hold 3 values after reading it; those are OCIO's
        // own checks, not the conversion's.
        if entry.get("exception").is_some_and(|e| {
            e["message"]
                .as_str()
                .is_some_and(|m| m.contains("'luma' values must be 3"))
        }) {
            continue;
        }
        check::<Vec<f64>>("luma", spelling, &entry);
    }
}

#[test]
fn booleans_read_as_the_wheel_reads_them() {
    let spellings: Vec<String> = BOOLS.iter().map(|s| s.to_string()).collect();
    for field in ["strictparsing", "isdata"] {
        for (spelling, entry) in spellings.iter().zip(entries(field, &spellings)) {
            check::<bool>(field, spelling, &entry);
        }
    }
}
