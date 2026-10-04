// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processors' caches and the environment against the wheel, through the oracle's
//! `processor_cache`: which `getProcessor`, `getOptimizedProcessor` and
//! `getOptimizedCPUProcessor`, `getDefaultGPUProcessor` and `getOptimizedGPUProcessor` calls
//! give the same object, under the cache flags,
//! `clearProcessorCache`, `OCIO_DISABLE_ALL_CACHES`, `OCIO_DISABLE_PROCESSOR_CACHES`,
//! `OCIO_DISABLE_CACHE_FALLBACK` and `OCIO_OPTIMIZATION_FLAGS`, and the errors.
//!
//! The fallback reuses the first cached processor of the same cache ID in the order of the
//! cache's keys, `std::hash` values of the processors' texts (Config.cpp:4836-4873 @ v2.5.2):
//! cases that cache several processors of one cache ID check the hash on each platform
//! (docs/improvements.md, I-53). The environment's flags check `std::stoul` (I-54).
//!
//! OCIO reads the environment through one global provider here, so this binary's tests hold
//! one lock.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ocio::{
    BitDepth, Config, GroupTransform, MatrixTransform, OptimizationFlags, Processor,
    ProcessorCacheFlags, RangeTransform, Transform, TransformDirection,
};
use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::platform::{MapEnv, set_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// A group of groups: its direction and its children.
#[derive(Debug, Clone)]
struct Group {
    dir: TransformDirection,
    children: Vec<Group>,
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

impl Group {
    fn spec(&self) -> Value {
        json!({
            "class": "GroupTransform",
            "calls": [["setDirection", {"enum": dir_name(self.dir)}]],
            "children": self.children.iter().map(Group::spec).collect::<Vec<_>>(),
        })
    }

    fn port(&self) -> Transform {
        let mut group = GroupTransform::new();
        group.set_direction(self.dir);
        for child in &self.children {
            group.append_transform(child.port());
        }
        group.into()
    }
}

/// Distinct groups: `n` of them, numbered in binary by their children's directions, under a
/// root of either direction.
fn groups(n: usize) -> Vec<Group> {
    (0..n)
        .map(|i| {
            let bits = usize::BITS - i.leading_zeros();
            let children = (0..bits)
                .map(|b| Group {
                    dir: if i >> b & 1 == 1 {
                        TransformDirection::Inverse
                    } else {
                        TransformDirection::Forward
                    },
                    children: Vec::new(),
                })
                .collect();
            Group {
                dir: if i % 3 == 0 {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                },
                children,
            }
        })
        .collect()
}

/// A step of a case, as the oracle takes it.
#[derive(Debug, Clone)]
enum Step {
    Config,
    Env(&'static str, Option<String>),
    SetCacheFlags(&'static str),
    ClearCache,
    Processor(String, Group, TransformDirection),
    /// `config.getProcessor(transform, direction)` of any transform: its spec, and the port's.
    ProcessorOf(String, Value, Box<Transform>, TransformDirection),
    Optimized(String, String, BitDepth, BitDepth, u64),
    Cpu(String, String, BitDepth, BitDepth, u64),
    /// `getOptimizedGPUProcessor(flags)`, or `getDefaultGPUProcessor()` for `None`.
    Gpu(String, String, Option<u64>),
}

fn depth_name(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::Uint8 => "BIT_DEPTH_UINT8",
        BitDepth::Uint10 => "BIT_DEPTH_UINT10",
        BitDepth::Uint12 => "BIT_DEPTH_UINT12",
        BitDepth::Uint16 => "BIT_DEPTH_UINT16",
        BitDepth::F16 => "BIT_DEPTH_F16",
        BitDepth::F32 => "BIT_DEPTH_F32",
        other => panic!("no pixel bit depth: {other:?}"),
    }
}

impl Step {
    fn spec(&self) -> Value {
        match self {
            Step::Config => json!(["config"]),
            Step::Env(name, value) => json!(["env", name, value]),
            Step::SetCacheFlags(flags) => json!(["set_cache_flags", flags]),
            Step::ClearCache => json!(["clear_cache"]),
            Step::Processor(name, group, dir) => {
                json!(["processor", name, group.spec(), dir_name(*dir)])
            }
            Step::ProcessorOf(name, spec, _, dir) => {
                json!(["processor", name, spec, dir_name(*dir)])
            }
            Step::Optimized(name, of, i, o, flags) => {
                json!(["optimized", name, of, depth_name(*i), depth_name(*o), flags])
            }
            Step::Cpu(name, of, i, o, flags) => {
                json!(["cpu", name, of, depth_name(*i), depth_name(*o), flags])
            }
            Step::Gpu(name, of, flags) => json!(["gpu", name, of, flags]),
        }
    }
}

/// A case: the environment it starts with, and its steps.
#[derive(Debug, Clone)]
struct Case {
    what: String,
    env: Vec<(&'static str, String)>,
    steps: Vec<Step>,
}

impl Case {
    fn spec(&self) -> Value {
        let env: BTreeMap<&str, &str> = self.env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        json!({"env": env, "steps": self.steps.iter().map(Step::spec).collect::<Vec<_>>()})
    }
}

/// What a case makes.
#[derive(Clone)]
enum Object {
    Processor(Arc<Processor>),
    Cpu(Arc<CpuProcessor>),
    Gpu(Arc<GpuProcessor>),
}

impl Object {
    fn same(&self, other: &Object) -> bool {
        match (self, other) {
            (Object::Processor(a), Object::Processor(b)) => Arc::ptr_eq(a, b),
            (Object::Cpu(a), Object::Cpu(b)) => Arc::ptr_eq(a, b),
            (Object::Gpu(a), Object::Gpu(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    fn cache_id(&self) -> String {
        match self {
            Object::Processor(p) => p.cache_id().expect("a cache ID"),
            Object::Cpu(p) => String::from_utf8(p.get_cache_id().to_vec()).expect("UTF-8"),
            Object::Gpu(p) => String::from_utf8(p.get_cache_id().to_vec()).expect("UTF-8"),
        }
    }
}

fn cache_flags(name: &str) -> ProcessorCacheFlags {
    match name {
        "PROCESSOR_CACHE_OFF" => ProcessorCacheFlags::OFF,
        "PROCESSOR_CACHE_ENABLED" => ProcessorCacheFlags::ENABLED,
        "PROCESSOR_CACHE_SHARE_DYN_PROPERTIES" => ProcessorCacheFlags::SHARE_DYN_PROPERTIES,
        "PROCESSOR_CACHE_DEFAULT" => ProcessorCacheFlags::DEFAULT,
        other => panic!("no cache flags {other}"),
    }
}

/// Runs a case on the port, in the form of the oracle's report.
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::unnecessary_cast)]
fn run_port(case: &Case) -> Value {
    let mut env: BTreeMap<String, String> = case
        .env
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    set_env_provider(Some(Arc::new(MapEnv(env.clone()))));

    let mut config: Option<Arc<Config>> = None;
    let mut objects: Vec<(String, Object)> = Vec::new();
    let find = |objects: &[(String, Object)], name: &str| -> Arc<Processor> {
        match objects.iter().find(|(n, _)| n == name) {
            Some((_, Object::Processor(p))) => p.clone(),
            _ => panic!("no processor {name}"),
        }
    };
    let mut results = Vec::new();
    for step in &case.steps {
        let result: ocio::Result<()> = (|| {
            match step {
                Step::Config => config = Some(Config::create_raw()),
                Step::Env(name, value) => {
                    match value {
                        Some(v) => env.insert(name.to_string(), v.clone()),
                        None => env.remove(*name),
                    };
                    set_env_provider(Some(Arc::new(MapEnv(env.clone()))));
                }
                Step::SetCacheFlags(flags) => config
                    .as_ref()
                    .expect("a config")
                    .set_processor_cache_flags(cache_flags(flags)),
                Step::ClearCache => config.as_ref().expect("a config").clear_processor_cache(),
                Step::Processor(name, group, dir) => {
                    let p = config
                        .as_ref()
                        .expect("a config")
                        .processor_in_direction(&group.port(), *dir)?;
                    objects.push((name.clone(), Object::Processor(p)));
                }
                Step::ProcessorOf(name, _, transform, dir) => {
                    let p = config
                        .as_ref()
                        .expect("a config")
                        .processor_in_direction(transform, *dir)?;
                    objects.push((name.clone(), Object::Processor(p)));
                }
                Step::Optimized(name, of, i, o, flags) => {
                    let flags = OptimizationFlags(*flags as std::ffi::c_ulong);
                    let p =
                        find(&objects, of).optimized_processor_with_bit_depths(*i, *o, flags)?;
                    objects.push((name.clone(), Object::Processor(p)));
                }
                Step::Cpu(name, of, i, o, flags) => {
                    let flags = OptimizationFlags(*flags as std::ffi::c_ulong);
                    let p = find(&objects, of)
                        .optimized_cpu_processor_with_bit_depths(*i, *o, flags)?;
                    objects.push((name.clone(), Object::Cpu(p)));
                }
                Step::Gpu(name, of, flags) => {
                    let p = match flags {
                        None => find(&objects, of).default_gpu_processor()?,
                        Some(flags) => find(&objects, of).optimized_gpu_processor(
                            OptimizationFlags(*flags as std::ffi::c_ulong),
                        )?,
                    };
                    objects.push((name.clone(), Object::Gpu(p)));
                }
            }
            Ok(())
        })();
        results.push(match result {
            Ok(()) => Value::Null,
            Err(e) => json!({"type": "Exception", "message": e.message()}),
        });
    }
    set_env_provider(None);

    let same: BTreeMap<String, String> = objects
        .iter()
        .map(|(name, object)| {
            let first = objects
                .iter()
                .find(|(_, o)| o.same(object))
                .map(|(n, _)| n.clone())
                .expect("itself");
            (name.clone(), first)
        })
        .collect();
    let ids: BTreeMap<String, String> = objects
        .iter()
        .map(|(name, object)| (name.clone(), object.cache_id()))
        .collect();
    json!({"steps": results, "same": same, "cache_ids": ids})
}

/// Runs the cases on the wheel and on the port, and compares the reports.
fn check(cases: &[Case]) {
    let _lock = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    let response = Oracle::get().call(
        "processor_cache",
        json!({"cases": cases.iter().map(Case::spec).collect::<Vec<_>>()}),
        &[],
    );
    let results = response.result.as_array().expect("a result per case");
    assert_eq!(results.len(), cases.len());
    let mut failures = Vec::new();
    for (case, result) in cases.iter().zip(results) {
        let stderr = &response.blobs[result["stderr"].as_u64().expect("a blob") as usize];
        assert_eq!(
            result["returncode"],
            json!(0),
            "{}: {}",
            case.what,
            String::from_utf8_lossy(stderr)
        );
        let wheel = &result["report"];
        let port = run_port(case);
        if *wheel != port {
            failures.push(format!("{}\n  wheel {wheel}\n  port  {port}", case.what));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const FALLBACK: &str = "OCIO_DISABLE_CACHE_FALLBACK";
const FLAGS: &str = "OCIO_OPTIMIZATION_FLAGS";
const F32: BitDepth = BitDepth::F32;

fn name(prefix: &str, i: usize) -> String {
    format!("{prefix}{i}")
}

/// The fallback: with it, every group's processor is the first one (they all have the cache
/// ID `<NOOP>`); without it, each text gets its own, and the same text the same one; once
/// several are cached, the fallback picks the one of the smallest key, which is the platform's
/// `std::hash`.
#[test]
fn the_fallback_and_the_hash_match_the_wheel() {
    let all = groups(64);
    let mut cases = vec![Case {
        what: "fallback on".to_string(),
        env: Vec::new(),
        steps: std::iter::once(Step::Config)
            .chain((0..6).map(|i| {
                Step::Processor(name("p", i), all[i].clone(), TransformDirection::Forward)
            }))
            .chain(std::iter::once(Step::Processor(
                "again".to_string(),
                all[3].clone(),
                TransformDirection::Inverse,
            )))
            .collect(),
    }];
    // Rounds of 8 processors cached without the fallback, then a 9th text with it.
    for round in 0..7 {
        let set: Vec<&Group> = all[round * 9..round * 9 + 9].iter().collect();
        let mut steps = vec![Step::Config, Step::Env(FALLBACK, Some("1".to_string()))];
        for (i, group) in set[..8].iter().enumerate() {
            steps.push(Step::Processor(
                name("p", i),
                (*group).clone(),
                TransformDirection::Forward,
            ));
        }
        steps.push(Step::Processor(
            "same_text".to_string(),
            set[2].clone(),
            TransformDirection::Forward,
        ));
        steps.push(Step::Env(FALLBACK, None));
        steps.push(Step::Processor(
            "fallback".to_string(),
            set[8].clone(),
            TransformDirection::Forward,
        ));
        steps.push(Step::Processor(
            "fallback_inverse".to_string(),
            set[8].clone(),
            TransformDirection::Inverse,
        ));
        steps.push(Step::ClearCache);
        steps.push(Step::Processor(
            "after_clear".to_string(),
            set[0].clone(),
            TransformDirection::Forward,
        ));
        cases.push(Case {
            what: format!("round {round}"),
            env: Vec::new(),
            steps,
        });
    }
    check(&cases);
}

/// The cache flags, `clearProcessorCache` and the variables that disable the caches, for the
/// config's cache and the processors' caches.
#[test]
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::unnecessary_cast)]
fn cache_flags_and_variables_match_the_wheel() {
    let all = groups(4);
    let steps = |flags: Option<&'static str>| {
        let mut steps = vec![Step::Config];
        if let Some(flags) = flags {
            steps.push(Step::SetCacheFlags(flags));
        }
        steps.extend([
            Step::Processor("a".into(), all[1].clone(), TransformDirection::Forward),
            Step::Processor("b".into(), all[1].clone(), TransformDirection::Forward),
            Step::Processor("c".into(), all[2].clone(), TransformDirection::Forward),
            Step::Optimized("oa".into(), "a".into(), F32, F32, 0),
            Step::Optimized("ob".into(), "a".into(), F32, F32, 0),
            Step::Optimized("oc".into(), "a".into(), BitDepth::Uint8, F32, 0),
            Step::Cpu("ca".into(), "a".into(), F32, F32, 0),
            Step::Cpu("cb".into(), "a".into(), F32, F32, 0),
            Step::Cpu("cc".into(), "a".into(), F32, BitDepth::F16, 0),
            Step::Gpu("ga".into(), "a".into(), None),
            Step::Gpu("gb".into(), "a".into(), None),
            Step::Gpu("gc".into(), "a".into(), Some(0)),
            Step::Gpu("gd".into(), "a".into(), Some(0)),
            Step::Gpu(
                "ge".into(),
                "a".into(),
                Some(OptimizationFlags::DEFAULT.0 as u64),
            ),
            Step::SetCacheFlags("PROCESSOR_CACHE_OFF"),
            Step::Processor("d".into(), all[1].clone(), TransformDirection::Forward),
            Step::Optimized("od".into(), "a".into(), F32, F32, 0),
            Step::SetCacheFlags("PROCESSOR_CACHE_ENABLED"),
            Step::Processor("e".into(), all[1].clone(), TransformDirection::Forward),
            Step::ClearCache,
            Step::Processor("f".into(), all[1].clone(), TransformDirection::Forward),
            Step::Optimized("of".into(), "f".into(), F32, F32, 0),
            Step::Optimized("og".into(), "f".into(), F32, F32, 0),
            Step::Gpu("gf".into(), "f".into(), None),
            Step::Gpu("gg".into(), "f".into(), None),
            Step::Gpu("gh".into(), "a".into(), None),
        ]);
        steps
    };
    let mut cases = Vec::new();
    for flags in [
        None,
        Some("PROCESSOR_CACHE_OFF"),
        Some("PROCESSOR_CACHE_ENABLED"),
        Some("PROCESSOR_CACHE_SHARE_DYN_PROPERTIES"),
        Some("PROCESSOR_CACHE_DEFAULT"),
    ] {
        cases.push(Case {
            what: format!("flags {flags:?}"),
            env: Vec::new(),
            steps: steps(flags),
        });
    }
    for var in ["OCIO_DISABLE_ALL_CACHES", "OCIO_DISABLE_PROCESSOR_CACHES"] {
        cases.push(Case {
            what: format!("{var} set"),
            env: vec![(var, "1".to_string())],
            steps: steps(None),
        });
        // Set once the config exists: the config's cache read it before.
        let mut late = steps(None);
        late.insert(1, Step::Env(var, Some("1".to_string())));
        cases.push(Case {
            what: format!("{var} set after the config"),
            env: Vec::new(),
            steps: late,
        });
    }
    check(&cases);
}

/// `OCIO_OPTIMIZATION_FLAGS`: the processors made under each value, against processors made
/// with explicit flags and under the other values, and the values `std::stoul` refuses.
#[test]
fn optimization_flags_variable_matches_the_wheel() {
    let values = [
        "0",
        "00",
        "0x0",
        "1",
        "010",
        "0x1F",
        "0X1f",
        " 7",
        "+7",
        "\t31zz",
        "0x1Fzz",
        "-0",
        "-1",
        "4294967295",
        "4294967296",
        "0xFFFFFFFF",
        "0x100000000",
        "18446744073709551615",
        "18446744073709551616",
        "-4294967295",
        "0x",
        "0X",
        "0xg",
        "08",
        "019",
        "abc",
        "-",
        "+",
        " ",
        "1e5",
    ];
    let group = groups(2)[1].clone();
    let mut steps = vec![
        Step::Config,
        Step::Processor("p".into(), group, TransformDirection::Forward),
    ];
    for explicit in [0u64, 1, 7, 8, 31, 0xFFFF_FFFF] {
        steps.push(Step::Optimized(
            name("o", explicit as usize),
            "p".into(),
            F32,
            F32,
            explicit,
        ));
        steps.push(Step::Cpu(
            name("c", explicit as usize),
            "p".into(),
            F32,
            F32,
            explicit,
        ));
        steps.push(Step::Gpu(
            name("g", explicit as usize),
            "p".into(),
            Some(explicit),
        ));
    }
    for (i, value) in values.iter().enumerate() {
        steps.push(Step::Env(FLAGS, Some(value.to_string())));
        steps.push(Step::Optimized(
            name("env_o", i),
            "p".into(),
            F32,
            F32,
            0x0000_0003,
        ));
        steps.push(Step::Cpu(
            name("env_c", i),
            "p".into(),
            F32,
            F32,
            0x0000_0003,
        ));
        steps.push(Step::Optimized(
            name("env_o16_", i),
            "p".into(),
            BitDepth::F16,
            F32,
            0,
        ));
        steps.push(Step::Gpu(name("env_g", i), "p".into(), Some(0x0000_0003)));
        steps.push(Step::Gpu(name("env_gd", i), "p".into(), None));
    }
    steps.push(Step::Env(FLAGS, None));
    steps.push(Step::Optimized(
        "unset".into(),
        "p".into(),
        F32,
        F32,
        0x0000_0003,
    ));
    check(&[Case {
        what: "OCIO_OPTIMIZATION_FLAGS".to_string(),
        env: Vec::new(),
        steps,
    }]);
}

/// The direction is part of the config's key (`<< direction`, Config.cpp:4836-4839 @ v2.5.2):
/// without the fallback, the same group forward and inverse gives two processors, each of them
/// again for its direction. And a processor's copies keep its cache flags: the optimized
/// processor's own caches are on or off as the config's flags were when the processor was
/// made (Processor.cpp:237-263, 584-593 @ v2.5.2).
#[test]
fn the_key_and_the_copies_match_the_wheel() {
    let all = groups(6);
    let (fwd, inv) = (TransformDirection::Forward, TransformDirection::Inverse);
    let mut cases = vec![Case {
        what: "direction without fallback".into(),
        env: vec![(FALLBACK, "1".to_string())],
        steps: vec![
            Step::Config,
            Step::Processor("f".into(), all[3].clone(), fwd),
            Step::Processor("i".into(), all[3].clone(), inv),
            Step::Processor("f2".into(), all[3].clone(), fwd),
            Step::Processor("i2".into(), all[3].clone(), inv),
        ],
    }];
    for flags in [
        "PROCESSOR_CACHE_OFF",
        "PROCESSOR_CACHE_DEFAULT",
        "PROCESSOR_CACHE_ENABLED",
    ] {
        cases.push(Case {
            what: format!("optimized of optimized, {flags}"),
            env: Vec::new(),
            steps: vec![
                Step::Config,
                Step::SetCacheFlags(flags),
                Step::Processor("p".into(), all[1].clone(), fwd),
                Step::Optimized("o".into(), "p".into(), F32, F32, 0),
                Step::Optimized("oo1".into(), "o".into(), F32, F32, 0),
                Step::Optimized("oo2".into(), "o".into(), F32, F32, 0),
                Step::Cpu("oc1".into(), "o".into(), F32, F32, 0),
                Step::Cpu("oc2".into(), "o".into(), F32, F32, 0),
                Step::Gpu("og1".into(), "o".into(), None),
                Step::Gpu("og2".into(), "o".into(), None),
                Step::SetCacheFlags("PROCESSOR_CACHE_DEFAULT"),
                Step::Optimized("oo3".into(), "o".into(), F32, F32, 0),
                Step::Optimized("oo4".into(), "o".into(), F32, F32, 0),
                Step::Processor("q".into(), all[1].clone(), fwd),
            ],
        });
    }
    check(&cases);
}

/// A matrix transform of `offset4`, with the spec that builds it in the wheel.
fn offset_matrix(offset4: [f64; 4]) -> (Value, Transform) {
    let mut port = MatrixTransform::new();
    port.set_offset(&offset4);
    let spec = json!({"class": "MatrixTransform", "calls": [
        ["setOffset", offset4.iter().map(|&v| f64_spec(v)).collect::<Vec<_>>()],
    ]});
    (spec, port.into())
}

/// A clamping range transform from `[0, 1]` to `[0, 1]`: an identity clamp.
fn identity_clamp() -> (Value, Transform) {
    let mut port = RangeTransform::new();
    port.set_min_in_value(0.0);
    port.set_max_in_value(1.0);
    port.set_min_out_value(0.0);
    port.set_max_out_value(1.0);
    let spec = json!({"class": "RangeTransform", "calls": [
        ["setMinInValue", f64_spec(0.0)],
        ["setMaxInValue", f64_spec(1.0)],
        ["setMinOutValue", f64_spec(0.0)],
        ["setMaxOutValue", f64_spec(1.0)],
    ]});
    (spec, port.into())
}

/// A group of `children`, with the spec that builds it in the wheel.
fn group_of(children: Vec<(Value, Transform)>) -> (Value, Transform) {
    let mut port = GroupTransform::new();
    let mut specs = Vec::new();
    for (spec, child) in children {
        port.append_transform(child);
        specs.push(spec);
    }
    (
        json!({"class": "GroupTransform", "children": specs}),
        port.into(),
    )
}

fn processor_of(name: &str, (spec, port): (Value, Transform)) -> Step {
    Step::ProcessorOf(
        name.into(),
        spec,
        Box::new(port),
        TransformDirection::Forward,
    )
}

/// Processors of ops, whose cache IDs differ (the groups' are all `<NOOP>`):
/// - the fallback reuses only a processor of the same cache ID (a group of one matrix and the
///   matrix), not any cached one (Config.cpp:4857-4868 @ v2.5.2);
/// - an optimized processor computes its own cache ID, which differs from the processor's when
///   the optimizer changes the ops: the copy starts without one (Processor.cpp:237-263 @
///   v2.5.2), even when the processor's is already known (the fallback computed it);
/// - the optimized processors of integer input lose the leading identity clamp
///   (`optimizeForBitdepth`, Processor.cpp:382-433 @ v2.5.2).
#[test]
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::unnecessary_cast)]
fn processors_of_ops_match_the_wheel() {
    let default = OptimizationFlags::DEFAULT.0 as u64;
    let two_matrices_after_a_clamp = || {
        group_of(vec![
            identity_clamp(),
            offset_matrix([0.1, 0.0, 0.0, 0.0]),
            offset_matrix([0.0, 0.2, 0.0, 0.0]),
        ])
    };
    let mut cases = Vec::new();
    for fallback in [None, Some("1".to_string())] {
        let env = match &fallback {
            Some(value) => vec![(FALLBACK, value.clone())],
            None => Vec::new(),
        };
        cases.push(Case {
            what: format!("matrices, {FALLBACK} {fallback:?}"),
            env,
            steps: vec![
                Step::Config,
                processor_of("a", offset_matrix([0.1, 0.0, 0.0, 0.0])),
                processor_of("b", offset_matrix([0.2, 0.0, 0.0, 0.0])),
                processor_of("c", group_of(vec![offset_matrix([0.1, 0.0, 0.0, 0.0])])),
                processor_of("d", two_matrices_after_a_clamp()),
                Step::Optimized("od".into(), "d".into(), F32, F32, default),
                Step::Optimized("od_none".into(), "d".into(), F32, F32, 0),
                Step::Optimized("od8".into(), "d".into(), BitDepth::Uint8, F32, default),
                Step::Optimized("od16".into(), "d".into(), BitDepth::Uint16, F32, default),
                Step::Optimized("od_f16".into(), "d".into(), BitDepth::F16, F32, default),
                Step::Cpu("cd".into(), "d".into(), F32, F32, default),
                Step::Cpu("cd8".into(), "d".into(), BitDepth::Uint8, F32, default),
                Step::Cpu("cd8_out".into(), "d".into(), F32, BitDepth::Uint8, default),
                processor_of("e", two_matrices_after_a_clamp()),
            ],
        });
    }
    check(&cases);
}
