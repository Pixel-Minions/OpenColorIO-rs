// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the app helpers' oracle tests share (`legacy_viewing_pipeline_oracle.rs`,
//! `display_view_helpers_oracle.rs`, `mixing_helpers_oracle.rs`): a deterministic generator, the
//! port's log, and the comparison of a processor the wheel made in an `object_calls` request
//! (its dump, its log, and the pixels of its CPU processors) with the port's.
//!
//! The pixels go through the binding's `applyRGBA(list)`, which converts each value from and
//! to a Python float: signalling NaNs can't pass that way unchanged, so the probe values have
//! quiet NaNs only (each with a payload of its own).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use ocio::{DynamicPropertyType, OptimizationFlags, Processor};
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_testkit::oracle::Response;
use ocio_testkit::oracle_values::{bytes, log};
use ocio_testkit::processor_ops::ProcessorOpsReply;
use serde_json::{Value, json};

use super::transforms::compare_processor;

/// A small deterministic generator (xorshift64*), so the cases never change unnoticed.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub(crate) fn pick<T: Clone>(&mut self, list: &[T]) -> T {
        list[self.below(list.len())].clone()
    }

    /// `None` one time in `of`, else one of `n`.
    pub(crate) fn maybe(&mut self, n: usize, of: usize) -> Option<usize> {
        if self.below(of) == 0 {
            None
        } else {
            Some(self.below(n))
        }
    }
}

/// A double as an exact value spec.
pub(crate) fn f(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}

/// Doubles as exact value specs.
pub(crate) fn fs(values: &[f64]) -> Value {
    values.iter().map(|&v| f(v)).collect()
}

/// A name the port returned, as text.
pub(crate) fn text(v: &[u8]) -> String {
    String::from_utf8(v.to_vec()).expect("UTF-8 names")
}

/// OCIO's logging function is the process's, so the tests of a file log one at a time.
static LOGGING: Mutex<()> = Mutex::new(());

/// Runs `f`, and gives the messages OCIO logged on this thread meanwhile.
pub(crate) fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let me = std::thread::current().id();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        if std::thread::current().id() == me {
            sink.lock().unwrap().push(m.to_vec());
        } else {
            eprint!("{}", String::from_utf8_lossy(m));
        }
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

/// The probe pixels, packed RGBA: zeros, ordinary values, mid grey, the extremes of `f32`,
/// subnormals, the infinities and quiet NaNs with payloads of their own, each in every
/// channel, then each as a grey of alpha 1.
pub(crate) fn pixels() -> Vec<f32> {
    let specials: Vec<f32> = vec![
        0.0,
        -0.0,
        0.18,
        1.0,
        -1.0,
        0.5,
        2.0,
        100.0,
        65504.0,
        -0.25,
        f32::from_bits(1),
        -1e-40,
        f32::MAX,
        -f32::MAX,
        f32::MIN_POSITIVE,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0xffc0_0002),
        f32::from_bits(0x7fe0_1003),
        1e20,
        3.4,
        0.001,
    ];
    let n = specials.len();
    let mut out = Vec::new();
    for i in 0..n {
        out.extend([
            specials[i],
            specials[(i + 5) % n],
            specials[(i + 11) % n],
            specials[(i + 3) % n],
        ]);
    }
    for &v in &specials {
        out.extend([v, v, v, 1.0]);
    }
    out
}

/// A dynamic property a case sets on the CPU processor before it applies it: the type, its
/// binding name, and the value.
pub(crate) type DynamicSet = (DynamicPropertyType, &'static str, f64);

/// The dynamic properties of exposure, contrast and gamma, set away from their defaults.
pub(crate) const EXPOSURE_CONTRAST_GAMMA: [DynamicSet; 3] = [
    (
        DynamicPropertyType::Exposure,
        "DYNAMIC_PROPERTY_EXPOSURE",
        0.75,
    ),
    (
        DynamicPropertyType::Contrast,
        "DYNAMIC_PROPERTY_CONTRAST",
        1.25,
    ),
    (DynamicPropertyType::Gamma, "DYNAMIC_PROPERTY_GAMMA", 1.5),
];

/// The port's CPU processor of `proc` (the default one for `None`), with the dynamic
/// properties `dynamic` set, applied to [`pixels`] as the binding's list overload applies it
/// (in place, one row).
fn apply_rgba(
    proc: &Processor,
    flags: Option<OptimizationFlags>,
    dynamic: &[DynamicSet],
) -> ocio::Result<Vec<f32>> {
    let cpu = match flags {
        None => proc.default_cpu_processor()?,
        Some(flags) => proc.optimized_cpu_processor(flags)?,
    };
    for &(property, _, value) in dynamic {
        match cpu.get_dynamic_property(property)? {
            DynamicPropertyRcPtr::Double(p) => p.set_value(value),
            other => panic!("{property:?} is no double: {other:?}"),
        }
    }
    let mut data = pixels();
    let width = data.len() / 4;
    {
        let mut img = PackedImageDesc::new(&mut data[..], width, 1, 4)?;
        cpu.apply(&mut img)?;
    }
    Ok(data)
}

/// The calls of one CPU processor of a processor, and the port's outcome.
pub(crate) struct Applied {
    label: &'static str,
    /// The index of the CPU processor's call; the dynamic properties' calls and the apply
    /// follow it, up to `apply_at`.
    first: usize,
    apply_at: usize,
    port: (ocio::Result<Vec<f32>>, Vec<Vec<u8>>),
}

/// Appends to `calls` the wheel's calls that apply the processor stored as `proc_name`: its
/// default CPU processor and its CPU processor without optimization, each with the dynamic
/// properties `dynamic` set, to [`pixels`]; and runs the same on the port's processor `proc`.
pub(crate) fn apply_calls(
    calls: &mut Vec<Value>,
    proc_name: &str,
    proc: &Processor,
    dynamic: &[DynamicSet],
) -> Vec<Applied> {
    let pixel_values: Vec<Value> = pixels().iter().map(|&v| f(f64::from(v))).collect();
    let mut out = Vec::new();
    for (label, flags, extra) in [
        ("getDefaultCPUProcessor", None, json!([])),
        (
            "getOptimizedCPUProcessor",
            Some(OptimizationFlags::NONE),
            json!([{"enum": "OPTIMIZATION_NONE"}]),
        ),
    ] {
        let cpu_name = format!("{proc_name}_{label}");
        let first = calls.len();
        calls.push(json!({"call": label, "on": proc_name, "args": extra, "as": cpu_name}));
        for (k, &(_, name, value)) in dynamic.iter().enumerate() {
            let dp = format!("{cpu_name}_dp{k}");
            calls.push(json!({"call": "getDynamicProperty", "on": cpu_name,
                "args": [{"enum": name}], "as": dp}));
            calls.push(json!({"call": "setDouble", "on": dp, "args": [f(value)]}));
        }
        let apply_at = calls.len();
        calls.push(json!({"call": "applyRGBA", "on": cpu_name, "args": [pixel_values.clone()]}));
        out.push(Applied {
            label,
            first,
            apply_at,
            port: capture(|| apply_rgba(proc, flags, dynamic)),
        });
    }
    out
}

/// What the cases of a test reached.
#[derive(Default)]
pub(crate) struct Reached {
    pub(crate) made: usize,
    pub(crate) refused: usize,
    pub(crate) messages: BTreeSet<String>,
    pub(crate) groups: BTreeSet<Vec<String>>,
    pub(crate) pixels: usize,
}

impl Reached {
    pub(crate) fn add(&mut self, other: Reached) {
        self.made += other.made;
        self.refused += other.refused;
        self.messages.extend(other.messages);
        self.groups.extend(other.groups);
        self.pixels += other.pixels;
    }
}

/// Compares the wheel's call `results[at]`, which made a processor or raised, with the port's
/// `made` and its log `port_log`, then the CPU processors' calls `applies` (only when the port
/// made the processor); the problems go to `problems`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compare_processor_call(
    response: &Response,
    results: &[Value],
    at: usize,
    made: &ocio::Result<Arc<Processor>>,
    port_log: &[Vec<u8>],
    applies: Vec<Applied>,
    reached: &mut Reached,
    problems: &mut Vec<String>,
) {
    let wheel = &results[at];
    match (wheel.get("exception"), made) {
        (Some(e), Err(port)) => {
            reached.refused += 1;
            let message = bytes(&e["message"]);
            reached
                .messages
                .insert(String::from_utf8_lossy(&message).into_owned());
            if message != port.what() {
                problems.push(format!(
                    "messages {:?} != {:?}",
                    String::from_utf8_lossy(&message),
                    String::from_utf8_lossy(port.what())
                ));
            }
        }
        (None, Ok(port)) => {
            reached.made += 1;
            let dump = &wheel["result"]["processor"];
            if dump.is_null() {
                problems.push(format!("the wheel gave {wheel}"));
            } else {
                let reply = ProcessorOpsReply::from_response(Response {
                    result: json!({"processor": dump}),
                    blobs: response.blobs.clone(),
                });
                reached.groups.insert(
                    reply
                        .processor()
                        .classes()
                        .into_iter()
                        .map(String::from)
                        .collect(),
                );
                let group = port.create_group_transform().unwrap();
                if let Err(e) = compare_processor(reply.processor(), port, &group) {
                    problems.push(format!("processor: {e}"));
                }
            }
        }
        (wheel_e, port) => problems.push(format!(
            "wheel {} {wheel}, port {:?}",
            if wheel_e.is_some() { "raised" } else { "made" },
            port.as_ref()
                .map(|_| "a processor")
                .map_err(|e| e.message())
        )),
    }
    let wheel_log = log(&wheel["log"]);
    if wheel_log != port_log {
        problems.push(format!(
            "log {:?} != {:?}",
            wheel_log
                .iter()
                .map(|m| String::from_utf8_lossy(m))
                .collect::<Vec<_>>(),
            port_log
                .iter()
                .map(|m| String::from_utf8_lossy(m))
                .collect::<Vec<_>>()
        ));
    }

    for Applied {
        label,
        first,
        apply_at,
        port: (port_pixels, apply_log),
    } in applies
    {
        let mut wheel_apply_log = Vec::new();
        for call in &results[first..=apply_at] {
            wheel_apply_log.extend(log(&call["log"]));
            // A DynamicProperty is made but can't be written out: the dump calls each of its
            // getters, and those of the other types raise ("stage": "write"); "as" stores it.
            let made = call.get("result").is_some() || call["stage"] == "write";
            if !made {
                problems.push(format!("{label}: the wheel gave {call}"));
            }
        }
        if wheel_apply_log != apply_log {
            problems.push(format!("{label} log {wheel_apply_log:?} != {apply_log:?}"));
        }
        let wheel_pixels = &results[apply_at];
        let port_pixels = match port_pixels {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!(
                    "{label}: the port raised {e:?}, wheel {wheel_pixels}"
                ));
                continue;
            }
        };
        let Some(list) = wheel_pixels["result"].as_array() else {
            continue;
        };
        let wheel_bits: Vec<u64> = list
            .iter()
            .map(|v| v["f64"].as_u64().expect("an f64"))
            .collect();
        let port_bits: Vec<u64> = port_pixels
            .iter()
            .map(|&v| f64::from(v).to_bits())
            .collect();
        assert_eq!(port_bits.len(), pixels().len());
        if wheel_bits != port_bits {
            let inputs = pixels();
            let diffs: Vec<String> = wheel_bits
                .iter()
                .zip(&port_bits)
                .enumerate()
                .filter(|(_, (w, p))| w != p)
                .take(8)
                .map(|(i, (w, p))| {
                    format!(
                        "[{i}] in {:?}: wheel {:?} ({w:#x}), port {:?} ({p:#x})",
                        inputs[i],
                        f64::from_bits(*w),
                        f64::from_bits(*p)
                    )
                })
                .collect();
            problems.push(format!("{label} pixels: {}", diffs.join("; ")));
        } else {
            reached.pixels += 1;
        }
    }
}

/// Writes `failures` to a file under the target directory and fails with their number and the
/// first few.
pub(crate) fn report(test: &str, failures: &[String]) {
    if failures.is_empty() {
        return;
    }
    let path = ocio_testkit::paths::target_dir().join(format!("{test}-failures.txt"));
    std::fs::write(&path, failures.join("\n\n")).unwrap();
    panic!(
        "{} mismatches (all in {}):\n{}",
        failures.len(),
        path.display(),
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}
