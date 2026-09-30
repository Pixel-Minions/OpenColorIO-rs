// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `BitDepthUtils` and `BitDepthToString` against the wheel, for every `BitDepth` enumerator.
//!
//! The wheel's Python API has no binding for `BitDepthUtils`, but a CPU processor with given
//! input and output bit depths reaches it:
//! - `getOptimizedCPUProcessor(in, out, flags)` finalizes the processor's ops for the CPU
//!   (`FinalizeOpsForCPU`, src/OpenColorIO/CPUProcessor.cpp:311-339 @ v2.5.2). For a non-empty
//!   op list, `OpRcPtrVec::optimizeForBitdepth` calls `IsFloatBitDepth(inBitDepth)`, then
//!   `IsFloatBitDepth(outBitDepth)` (src/OpenColorIO/OpOptimizers.cpp:758-777 @ v2.5.2), so a
//!   bit depth OCIO does not process fails there, on either side, with `IsFloatBitDepth`'s
//!   exception.
//! - Otherwise the CPU processor's cache ID starts with
//!   `CPU Processor: from <BitDepthToString(in)> to <BitDepthToString(out)> oFlags `
//!   (src/OpenColorIO/CPUProcessor.cpp:370-374 @ v2.5.2).
//!
//! `GetBitDepthMaxValue`, `IsFloatBitDepth` and `GetChannelSizeInBytes` list the same six
//! bit depths and throw the same exception for the others
//! (src/OpenColorIO/BitDepthUtils.cpp:18-148 @ v2.5.2), so all three are held to what the
//! wheel's `IsFloatBitDepth` accepts and rejects. The integer casts (`Converter<BD>`) are
//! checked against the wheel in `log_oracle.rs`.

use ocio_ops::Exception;
use ocio_ops::bit_depth_utils::{
    get_bit_depth_max_value, get_channel_size_in_bytes, is_float_bit_depth,
};
use ocio_ops::open_color_types::{BitDepth, bit_depth_to_string};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::{BatchCall, Response};
use serde_json::json;

/// Every `BitDepth` enumerator, with its name in the Python API.
const ALL_BIT_DEPTHS: [(BitDepth, &str); 9] = [
    (BitDepth::Unknown, "BIT_DEPTH_UNKNOWN"),
    (BitDepth::Uint8, "BIT_DEPTH_UINT8"),
    (BitDepth::Uint10, "BIT_DEPTH_UINT10"),
    (BitDepth::Uint12, "BIT_DEPTH_UINT12"),
    (BitDepth::Uint14, "BIT_DEPTH_UINT14"),
    (BitDepth::Uint16, "BIT_DEPTH_UINT16"),
    (BitDepth::Uint32, "BIT_DEPTH_UINT32"),
    (BitDepth::F16, "BIT_DEPTH_F16"),
    (BitDepth::F32, "BIT_DEPTH_F32"),
];

/// What the wheel did when asked for one CPU processor.
#[derive(Debug, PartialEq)]
enum Outcome {
    /// It raised: the Python exception class and the message.
    Rejected { class: String, message: String },
    /// It succeeded: the CPU processor's cache ID.
    Accepted(String),
}

/// The wheel's outcome from a `cpu_apply` response. An exception must come from getting the
/// CPU processor.
fn wheel_outcome(what: &str, resp: Result<Response, String>) -> Outcome {
    let resp = resp.unwrap_or_else(|e| panic!("{what}: {e}"));
    let text = |value: &serde_json::Value| {
        let s = value.as_str();
        s.unwrap_or_else(|| panic!("{what}: {}", resp.result))
            .to_owned()
    };
    match resp.result.get("exception") {
        Some(exc) => {
            assert_eq!(text(&resp.result["stage"]), "cpu_processor", "{what}");
            Outcome::Rejected {
                class: text(&exc["type"]),
                message: text(&exc["message"]),
            }
        }
        None => Outcome::Accepted(text(&resp.result["cpu_cache_id"])),
    }
}

/// The port's exception as the wheel reports one: `OCIO.Exception` or
/// `OCIO.ExceptionMissingFile`, and the message.
fn port_rejection(e: &Exception) -> Outcome {
    let class = if e.is_missing_file() {
        "ExceptionMissingFile"
    } else {
        "Exception"
    };
    Outcome::Rejected {
        class: class.to_owned(),
        message: e.message().to_owned(),
    }
}

/// For each bit depth, a LogTransform's CPU processor from it to F32 and from F32 to it: the
/// port's three queries accept exactly the bit depths the wheel accepts and reject the others
/// with the wheel's exception, and `bit_depth_to_string` names each accepted one as the wheel's
/// cache IDs do. All 18 oracle calls go in one batch.
#[test]
fn bit_depths_match_the_wheel() {
    // A LogOp is never a no-op, so the op list the CPU processor optimizes is not empty. The
    // input is never read for a bit depth the wheel rejects; 16 bytes hold whole pixels of
    // every supported one.
    let input = [0u8; 16];
    let spec = |input_name: &str, output_name: &str| {
        json!({
            "transform": {"class": "LogTransform", "args": {"base": 2.0}},
            "in_bitdepth": input_name,
            "out_bitdepth": output_name,
        })
    };
    let calls: Vec<BatchCall<'_>> = ALL_BIT_DEPTHS
        .iter()
        .flat_map(|&(_, name)| [spec(name, "BIT_DEPTH_F32"), spec("BIT_DEPTH_F32", name)])
        .map(|args| BatchCall {
            cmd: "cpu_apply",
            args,
            blobs: vec![&input],
        })
        .collect();
    let mut responses = Oracle::get().batch(&calls, true).into_iter();

    let f32_name = bit_depth_to_string(BitDepth::F32);
    let mut failures = Vec::new();
    for (bit_depth, name) in ALL_BIT_DEPTHS {
        let from = wheel_outcome(&format!("{name} to F32"), responses.next().unwrap());
        let to = wheel_outcome(&format!("F32 to {name}"), responses.next().unwrap());
        let port = [
            (
                "GetBitDepthMaxValue",
                get_bit_depth_max_value(bit_depth).err(),
            ),
            ("IsFloatBitDepth", is_float_bit_depth(bit_depth).err()),
            (
                "GetChannelSizeInBytes",
                get_channel_size_in_bytes(bit_depth).err(),
            ),
        ];
        match (&from, &to) {
            (Outcome::Rejected { .. }, Outcome::Rejected { .. }) => {
                assert_eq!(from, to, "{name}: the wheel's two exceptions");
                for (function, error) in &port {
                    match error {
                        Some(e) if port_rejection(e) == from => {}
                        Some(e) => failures.push(format!(
                            "{function}({name}): {:?}; the wheel: {from:?}",
                            port_rejection(e)
                        )),
                        None => failures
                            .push(format!("{function}({name}) succeeds; the wheel: {from:?}")),
                    }
                }
            }
            (Outcome::Accepted(from_id), Outcome::Accepted(to_id)) => {
                for (function, error) in &port {
                    if let Some(e) = error {
                        failures.push(format!(
                            "{function}({name}) throws {:?}; the wheel accepts {name}",
                            e.message()
                        ));
                    }
                }
                let s = bit_depth_to_string(bit_depth);
                let from_prefix = format!("CPU Processor: from {s} to {f32_name} oFlags ");
                let to_prefix = format!("CPU Processor: from {f32_name} to {s} oFlags ");
                for (id, prefix) in [(from_id, from_prefix), (to_id, to_prefix)] {
                    if !id.starts_with(&prefix) {
                        failures.push(format!(
                            "{name}: the wheel's cache ID {id:?}, port {prefix:?}"
                        ));
                    }
                }
            }
            _ => panic!("{name}: the wheel accepts it on one side only: {from:?}, {to:?}"),
        }
    }
    assert!(responses.next().is_none());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
