// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the processor: the parts of `tests/cpu/Processor_tests.cpp` @ v2.5.2 that need no
//! transform class beyond the group, the environment's optimization flags against the C
//! runtime's `strtoul`, the processors' caches on ops built directly, and that a processor's
//! getters leave it as it was.
//!
//! Which calls return the same processor is checked against the wheel by
//! `tests/processor_cache_oracle.rs`, on processors of groups (no ops). The cache tests here
//! run the same checks on processors with matrix ops, which the oracle's cases can't build
//! until `MatrixTransform` is ported; they mirror upstream's own cache tests, and go when the
//! oracle's cases have matrices.
//!
//! The upstream tests that build their processors from `MatrixTransform`,
//! `ExposureContrastTransform` and `Lut3DTransform` come with those classes.

use std::sync::Arc;

use ocio_ops::open_color_types::{BitDepth, OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::{create_matrix_op, create_offset_op};
use ocio_ops::ops::noop::no_ops::{create_file_no_op, create_look_no_op};
use ocio_testkit::crt::{ERANGE, strtoul_c};

use super::*;
use crate::test_env::EnvGuard;
use crate::transforms::group_transform::GroupTransform;

/// A processor of `ops`, finalized as `Processor::Impl::setTransform` finalizes the ops it
/// builds (Processor.cpp:636-640 @ v2.5.2), with the default cache flags.
fn processor_of(ops: OpVec) -> Processor {
    let mut processor = Processor::new();
    processor.set_processor_cache_flags(ProcessorCacheFlags::DEFAULT);
    processor.ops = ops;
    processor.ops.finalize().expect("finalized ops");
    processor
        .ops
        .validate_dynamic_properties()
        .expect("valid ops");
    processor
}

/// Two offsets, which the default optimization composes into one op.
fn two_offsets() -> OpVec {
    let mut ops = OpVec::new();
    create_offset_op(&mut ops, &[0.1, 0.2, 0.3, 0.0], TransformDirection::Forward);
    create_offset_op(&mut ops, &[0.1, 0.2, 0.3, 0.0], TransformDirection::Forward);
    ops
}

/// Port of `OCIO_ADD_TEST(Processor, optimization_env_override_basic)` @ v2.5.2.
#[test]
fn optimization_env_override_basic() {
    let env = EnvGuard::new();
    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "")]);

    let test_flag = OptimizationFlags::DEFAULT;
    assert_eq!(test_flag, environment_override(test_flag).unwrap());

    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "0")]);
    assert_eq!(
        OptimizationFlags::NONE,
        environment_override(test_flag).unwrap()
    );

    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "0xFFFFFFFF")]);
    assert_eq!(
        OptimizationFlags::ALL,
        environment_override(test_flag).unwrap()
    );

    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "144457667")]);
    assert_eq!(
        OptimizationFlags::LOSSLESS,
        environment_override(test_flag).unwrap()
    );

    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "0xFFC3FC3")]);
    assert_eq!(
        OptimizationFlags::GOOD,
        environment_override(test_flag).unwrap()
    );
}

/// Texts for `strtoul`: white space, signs, the three bases and their prefixes, digits of the
/// wrong base, overflow at 32 and at 64 bits, and NULs.
fn strtoul_inputs() -> Vec<&'static [u8]> {
    vec![
        b"",
        b" ",
        b"0",
        b"00",
        b"08",
        b"0x",
        b"0X",
        b"0x1g",
        b"0xg",
        b"0x0",
        b"0X1f",
        b"0x1F",
        b"010",
        b"017",
        b"019",
        b"9",
        b"12abc",
        b"abc",
        b"-",
        b"+",
        b"-0",
        b"+7",
        b"-1",
        b"--1",
        b"+-1",
        b"- 1",
        b" \t\n\x0b\x0c\r42",
        b"\xa042",
        b"\x8542",
        b"4294967295",
        b"4294967296",
        b"-4294967295",
        b"-4294967296",
        b"0xFFFFFFFF",
        b"0x100000000",
        b"-0x1",
        b"18446744073709551615",
        b"18446744073709551616",
        b"-18446744073709551615",
        b"-18446744073709551616",
        b"0xFFFFFFFFFFFFFFFF",
        b"0x10000000000000000",
        b"99999999999999999999999",
        b"0777777777777777777777",
        b"01777777777777777777777",
        b"02000000000000000000000",
        b"1 2",
        b"1\x002",
        b"\x001",
        b"0x\x001",
        b"1e5",
        b"1.5",
        b"0b101",
        b"144457667",
        b"0xFFC3FC3",
    ]
}

/// The port's `strtoul(text, &end, 0)` against the C runtime's, in the "C" locale: the value,
/// the end and `ERANGE`.
#[test]
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::useless_conversion)]
fn strtoul_matches_the_c_runtime() {
    let mut failures = Vec::new();
    for text in strtoul_inputs() {
        let (value, end, erange) = strtoul_base0(text);
        let port = (u64::from(value), end, erange);
        let crt = strtoul_c(text, 0);
        let crt = (crt.value, crt.end, crt.errno == ERANGE);
        if port != crt {
            failures.push(format!(
                "{:?}: port {port:?}, C runtime {crt:?}",
                String::from_utf8_lossy(text)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}

/// `std::stoul` refuses a text `strtoul` converts nothing of, and an `ERANGE`; otherwise it
/// takes the value, whatever follows the digits.
#[test]
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::useless_conversion)]
fn stoul_follows_strtoul() {
    for text in strtoul_inputs() {
        let crt = strtoul_c(text, 0);
        match stoul(text) {
            Ok(value) => {
                assert!(crt.end != 0 && crt.errno != ERANGE, "{text:?}");
                assert_eq!(u64::from(value), crt.value, "{text:?}");
            }
            Err(_) => assert!(crt.end == 0 || crt.errno == ERANGE, "{text:?}"),
        }
    }
}

/// A value `std::stoul` refuses is an error, which names the variable (Processor.cpp:363-371
/// @ v2.5.2).
#[test]
fn env_override_errors_name_the_variable() {
    let env = EnvGuard::new();
    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "abc")]);
    let error = environment_override(OptimizationFlags::DEFAULT).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("Illegal value for OCIO_OPTIMIZATION_FLAGS: "),
        "{error}"
    );
}

/// `getFile` and `getLook` give an empty string outside the lists (Processor.cpp:59-71, 83-92
/// @ v2.5.2), which the binding can't reach (its iterators check the index). What the lists
/// hold, and their order, is checked against the wheel by `tests/processor_oracle.rs`.
#[test]
fn processor_metadata_out_of_range() {
    let mut metadata = ProcessorMetadata::new();
    assert_eq!(metadata.num_files(), 0);
    assert_eq!(metadata.file(0), b"");
    assert_eq!(metadata.file(-1), b"");
    assert_eq!(metadata.num_looks(), 0);
    assert_eq!(metadata.look(0), b"");

    metadata.add_file(b"b");
    metadata.add_look(b"y");
    assert_eq!(metadata.file(1), b"");
    assert_eq!(metadata.file(-1), b"");
    assert_eq!(metadata.look(1), b"");
    assert_eq!(metadata.look(-1), b"");
}

/// `computeMetadata` collects the files of the `FileNoOp`s and the looks of the `LookNoOp`s
/// (Processor.cpp:655-664, ops/noop/NoOps.cpp:352-356, 437-440 @ v2.5.2): the file and look
/// transforms that make those ops come in Phase 3, and the oracle checks them then.
#[test]
fn compute_metadata_reads_the_no_ops() {
    let mut ops = OpVec::new();
    create_look_no_op(&mut ops, b"look1");
    create_file_no_op(&mut ops, b"/b.clf");
    create_offset_op(&mut ops, &[0.1, 0.2, 0.3, 0.0], TransformDirection::Forward);
    create_file_no_op(&mut ops, b"/a.clf");
    create_look_no_op(&mut ops, b"look0");
    let mut processor = processor_of(ops);
    processor.compute_metadata();

    let metadata = processor.processor_metadata();
    assert_eq!(metadata.num_files(), 2);
    assert_eq!([metadata.file(0), metadata.file(1)], [b"/a.clf", b"/b.clf"]);
    assert_eq!(metadata.num_looks(), 2);
    assert_eq!([metadata.look(0), metadata.look(1)], [b"look1", b"look0"]);
    assert_eq!(processor.num_transforms(), 5);
}

/// `getCacheID` is `<NOOP>` without ops (Processor.cpp:337-340 @ v2.5.2).
#[test]
fn cache_id_without_ops() {
    assert_eq!(processor_of(OpVec::new()).cache_id().unwrap(), "<NOOP>");
}

/// `getTransformFormatMetadata` outside the ops.
#[test]
fn transform_format_metadata_out_of_range() {
    let processor = processor_of(two_offsets());
    for index in [-1, 2] {
        assert_eq!(
            processor
                .transform_format_metadata(index)
                .unwrap_err()
                .to_string(),
            "Processor::getTransformFormatMetadata: index out of range."
        );
    }
    assert!(processor.transform_format_metadata(1).is_ok());
}

/// The optimized processors are cached by bit depths and flags, after the environment's
/// override, as `OCIO_ADD_TEST(Processor, cache_optimized_processors)` checks them with two
/// MatrixTransforms (tests/cpu/Processor_tests.cpp:302-372 @ v2.5.2). The wheel:
/// `processor_cache_oracle.rs`, `cache_flags_and_variables_match_the_wheel` (oa, ob, oc) and
/// `optimization_flags_variable_matches_the_wheel` (o0 against env_o0).
#[test]
fn cache_optimized_processors() {
    let env = EnvGuard::new();
    let proc1 = processor_of(two_offsets());
    let (f16, f32) = (BitDepth::F16, BitDepth::F32);

    let opt_proc1 = proc1
        .optimized_processor_with_bit_depths(f32, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    let opt_proc2 = proc1
        .optimized_processor_with_bit_depths(f32, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&opt_proc1, &opt_proc2));
    assert_eq!(opt_proc1.num_transforms(), 1);

    let opt_proc2 = proc1
        .optimized_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&opt_proc1, &opt_proc2));

    // The input bit-depth is different.
    let opt_proc2 = proc1
        .optimized_processor_with_bit_depths(f16, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(!Arc::ptr_eq(&opt_proc1, &opt_proc2));

    // The optimization flag is different.
    let opt_proc2 = proc1
        .optimized_processor_with_bit_depths(f32, f32, OptimizationFlags::NONE)
        .unwrap();
    assert!(!Arc::ptr_eq(&opt_proc1, &opt_proc2));
    assert_eq!(opt_proc2.num_transforms(), 2);

    let opt_proc1 = proc1
        .optimized_processor_with_bit_depths(f32, f32, OptimizationFlags::NONE)
        .unwrap();
    assert!(Arc::ptr_eq(&opt_proc1, &opt_proc2));

    // The cache key holds the flags the environment gives.
    env.set(&[(OCIO_OPTIMIZATION_FLAGS_ENVVAR, "0")]);
    let opt_proc2 = proc1
        .optimized_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&opt_proc1, &opt_proc2));
    env.set(&[]);

    // The processor's own ops are left as they were.
    assert_eq!(proc1.num_transforms(), 2);
}

/// The CPU processors are cached by bit depths and flags, as `OCIO_ADD_TEST(Processor,
/// cache_cpu_processors)` checks them with a MatrixTransform (Processor_tests.cpp:374-471 @
/// v2.5.2), and not at all when the cache flags turn the cache off. The wheel:
/// `processor_cache_oracle.rs`, `cache_flags_and_variables_match_the_wheel` (ca, cb, cc, and
/// the `PROCESSOR_CACHE_OFF` case).
#[test]
fn cache_cpu_processors() {
    let _env = EnvGuard::new();
    let proc1 = processor_of(two_offsets());
    let (f16, f32) = (BitDepth::F16, BitDepth::F32);

    let cpu_proc1 = proc1
        .optimized_cpu_processor_with_bit_depths(f32, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    let cpu_proc2 = proc1
        .optimized_cpu_processor_with_bit_depths(f32, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    let cpu_proc2 = proc1
        .optimized_cpu_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    let cpu_proc2 = proc1.default_cpu_processor().unwrap();
    assert!(Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    // The input bit-depth is different.
    let cpu_proc2 = proc1
        .optimized_cpu_processor_with_bit_depths(f16, f32, OptimizationFlags::DEFAULT)
        .unwrap();
    assert!(!Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    // The optimization flag is different.
    let cpu_proc2 = proc1
        .optimized_cpu_processor_with_bit_depths(f32, f32, OptimizationFlags::LOSSLESS)
        .unwrap();
    assert!(!Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    let cpu_proc1 = proc1
        .optimized_cpu_processor(OptimizationFlags::LOSSLESS)
        .unwrap();
    assert!(Arc::ptr_eq(&cpu_proc1, &cpu_proc2));

    // Disable the caches.
    let mut proc2 = processor_of(two_offsets());
    proc2.set_processor_cache_flags(ProcessorCacheFlags::OFF);
    assert!(!Arc::ptr_eq(
        &proc2.default_cpu_processor().unwrap(),
        &proc2.default_cpu_processor().unwrap()
    ));
    assert!(!Arc::ptr_eq(
        &proc2
            .optimized_processor(OptimizationFlags::DEFAULT)
            .unwrap(),
        &proc2
            .optimized_processor(OptimizationFlags::DEFAULT)
            .unwrap()
    ));
}

/// An optimized processor keeps its parent's cache flags (`Processor::Impl::operator=`,
/// Processor.cpp:237-263 @ v2.5.2) and shares its metadata: the port's structure, which the
/// wheel's binding doesn't show.
#[test]
fn optimized_processors_keep_the_cache_flags_and_metadata() {
    let _env = EnvGuard::new();
    let mut ops = two_offsets();
    create_file_no_op(&mut ops, b"/a.clf");
    let mut proc1 = processor_of(ops);
    proc1.compute_metadata();
    proc1.set_processor_cache_flags(ProcessorCacheFlags::OFF);

    let optimized = proc1
        .optimized_processor(OptimizationFlags::DEFAULT)
        .unwrap();
    assert_eq!(optimized.cache_flags, ProcessorCacheFlags::OFF);
    assert!(!optimized.cpu_processor_cache.is_enabled());
    assert!(Arc::ptr_eq(&optimized.metadata, &proc1.metadata));
    assert_eq!(optimized.processor_metadata().file(0), b"/a.clf");
}

/// A copy of a config keeps its cache flags and none of its processors
/// (`Config::Impl::operator=`, Config.cpp:451-454 @ v2.5.2); the oracle doesn't copy configs.
/// The rest of the config's cache is checked against the wheel by `processor_cache_oracle.rs`.
#[test]
fn config_copy_keeps_the_flags_and_no_processor() {
    let _env = EnvGuard::new();
    let config = Config::create_raw();
    config.set_processor_cache_flags(ProcessorCacheFlags::ENABLED);
    let group = Transform::from(GroupTransform::new());
    let p1 = config.processor(&group).unwrap();

    let copy = (*config).clone();
    assert_eq!(copy.processor_cache_flags(), ProcessorCacheFlags::ENABLED);
    let p2 = copy.processor(&group).unwrap();
    assert!(!Arc::ptr_eq(&p1, &p2));
    assert!(Arc::ptr_eq(&p2, &copy.processor(&group).unwrap()));

    // A copy of a config whose flags turn the cache off has its cache off.
    config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
    let off = (*config).clone();
    assert_eq!(off.processor_cache_flags(), ProcessorCacheFlags::OFF);
    assert!(!Arc::ptr_eq(
        &off.processor(&group).unwrap(),
        &off.processor(&group).unwrap()
    ));

    // The processors take the config's flags.
    let p3 = config.processor(&group).unwrap();
    assert_eq!(p3.cache_flags, ProcessorCacheFlags::OFF);
    assert!(!p3.opt_processor_cache.is_enabled());
    assert!(!p3.gpu_processor_cache.is_enabled());
}

/// A processor's ops are built once: building a processor twice is an internal error
/// (Processor.cpp:628-631 @ v2.5.2).
#[test]
fn set_transform_needs_an_empty_processor() {
    let _env = EnvGuard::new();
    let config = Config::create_raw();
    let context = Context::new();
    let mut processor = processor_of(two_offsets());
    let group = Transform::from(GroupTransform::new());
    assert_eq!(
        processor
            .set_transform(&config, &context, &group, TransformDirection::Forward)
            .unwrap_err()
            .to_string(),
        "Internal error: Processor should be empty"
    );
}

/// `setMajorVersion` resets the config's cache IDs, which empties its cache of processors
/// (Config.cpp:1285-1304, 5462-5474 @ v2.5.2).
#[test]
fn set_major_version_empties_the_processor_cache() {
    let _env = EnvGuard::new();
    let mut config = Config::create_raw();
    let group = Transform::from(GroupTransform::new());
    let p1 = config.processor(&group).unwrap();
    assert!(Arc::ptr_eq(&p1, &config.processor(&group).unwrap()));
    Arc::get_mut(&mut config)
        .expect("one owner")
        .set_major_version(2)
        .unwrap();
    assert!(!Arc::ptr_eq(&p1, &config.processor(&group).unwrap()));
}

/// What a processor's state is: its cache ID, computed afresh (the processor's own is computed
/// once), and each op's data, its address and its cache ID.
fn state(processor: &Processor) -> (String, Vec<(usize, String, Vec<u8>)>) {
    let cache_id = cache_id_hash(&processor.ops.get_cache_id().expect("a cache ID"));
    let ops = processor
        .ops
        .iter()
        .map(|op| {
            (
                Arc::as_ptr(op.data()) as usize,
                format!("{:?}", op.data()),
                op.get_cache_id().expect("an op cache ID"),
            )
        })
        .collect();
    (cache_id, ops)
}

/// A processor of an inverse matrix, a 3x3 matrix (as a CLF file gives one, which the
/// processor's finalize expands to 4x4) and an offset: every getter that makes a processor
/// from it works on a copy of its ops, and leaves the processor's ops, their data and its
/// cache ID as they were (the processor finalizes its own ops once, in `setTransform`).
#[test]
fn getters_leave_the_processor_as_it_was() {
    let _env = EnvGuard::new();
    let mut ops = OpVec::new();
    let mut inverse = MatrixOpData::new();
    inverse.set_rgba(&[
        2.0, 0.1, 0.0, 0.0, 0.0, 1.5, 0.2, 0.0, 0.3, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    create_matrix_op(&mut ops, inverse, TransformDirection::Inverse);
    let mut three = MatrixOpData::new();
    three.get_array_mut().resize(3, 3);
    three
        .get_array_mut()
        .get_values_mut()
        .copy_from_slice(&[1.1, 0.1, 0.0, 0.0, 0.9, 0.0, 0.2, 0.0, 1.2]);
    create_matrix_op(&mut ops, three, TransformDirection::Forward);
    create_offset_op(&mut ops, &[0.1, 0.2, 0.3, 0.0], TransformDirection::Forward);
    let processor = processor_of(ops);

    let before = state(&processor);
    let cache_id = processor.cache_id().unwrap();
    assert_eq!(before.1.len(), 3);

    let (f16, u8_, f32) = (BitDepth::F16, BitDepth::Uint8, BitDepth::F32);
    let made = [
        processor
            .optimized_processor(OptimizationFlags::DEFAULT)
            .unwrap(),
        processor
            .optimized_processor(OptimizationFlags::NONE)
            .unwrap(),
        processor
            .optimized_processor_with_bit_depths(f16, u8_, OptimizationFlags::ALL)
            .unwrap(),
    ];
    processor.default_cpu_processor().unwrap();
    processor
        .optimized_cpu_processor_with_bit_depths(f16, u8_, OptimizationFlags::ALL)
        .unwrap();
    processor
        .optimized_cpu_processor_with_bit_depths(f32, f32, OptimizationFlags::NONE)
        .unwrap();
    processor.default_gpu_processor().unwrap();
    processor
        .optimized_gpu_processor(OptimizationFlags::ALL)
        .unwrap();
    processor
        .optimized_gpu_processor(OptimizationFlags::NONE)
        .unwrap();
    // createGroupTransform of a matrix op comes with MatrixTransform: an error until then.
    let _ = processor.create_group_transform();
    for optimized in &made {
        optimized.default_cpu_processor().unwrap();
        optimized.default_gpu_processor().unwrap();
    }

    assert_eq!(state(&processor), before);
    assert_eq!(processor.cache_id().unwrap(), cache_id);
    assert_eq!(processor.num_transforms(), 3);
    // Not vacuous: the optimizer changed the copies' ops.
    assert_ne!(state(&made[0]).1, before.1);
}
