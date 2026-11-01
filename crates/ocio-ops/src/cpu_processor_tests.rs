// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/CPUProcessor_tests.cpp` @ v2.5.2: here `flag_composition`;
//! `tests/cpu_processor_oracle.rs` checks the CPU processor against the wheel. The tests that
//! get their processors from `Config::Create()` (`with_one_matrix`, `one_pixel`,
//! `optimizations`, `planar_vs_packed` and the `scanline_*` tests) are in
//! `crates/ocio/tests/cpu_processor_tests.rs`. `dynamic_properties` also needs
//! `ExposureContrastTransform` (Phase 5), and `with_one_1d_lut` a `FileTransform`. `image_desc`
//! and `with_several_ops` read a config with a `FileTransform` (`Config::CreateFromStream`,
//! Phase 3, and the file formats, Phase 4).

use crate::open_color_types::{
    BitDepth, DynamicPropertyType, OptimizationFlags, TransformDirection,
};

/// Port of `OCIO_ADD_TEST(CPUProcessor, flag_composition)` @ v2.5.2.
#[test]
fn flag_composition() {
    // The test validates the build of a custom optimization flag.

    let mut custom_flags = OptimizationFlags::LOSSLESS;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::NONE
    );

    custom_flags = custom_flags | OptimizationFlags::COMP_LUT1D;

    assert_eq!(
        custom_flags & OptimizationFlags::COMP_LUT1D,
        OptimizationFlags::COMP_LUT1D
    );
}

/// The processor of one matrix op.
fn matrix_processor(
    mat: crate::ops::matrix::MatrixOpData,
    input: BitDepth,
    output: BitDepth,
) -> super::CpuProcessor {
    let mut ops = crate::op::OpVec::new();
    crate::ops::matrix::matrix_op::create_matrix_op(&mut ops, mat, TransformDirection::Forward);
    ops.finalize().unwrap();
    super::CpuProcessor::new(&ops, input, output, OptimizationFlags::DEFAULT).unwrap()
}

/// A matrix processor has no dynamic property, and asking for one gives upstream's message.
/// `is_dynamic` is checked against the wheel in `tests/cpu_processor_oracle.rs`
/// (`is_dynamic_matches_the_wheel`).
///
/// Hand-derived: the oracle doesn't expose hasDynamicProperty; pinned in p1-processor.
#[test]
fn a_matrix_processor_has_no_dynamic_property() {
    let cpu = matrix_processor(
        crate::ops::matrix::MatrixOpData::create_diagonal_matrix(2.0),
        BitDepth::F16,
        BitDepth::Uint10,
    );
    assert!(!cpu.has_dynamic_property(DynamicPropertyType::Exposure));
    // The message of upstream's `CPUProcessor, dynamic_properties` test.
    assert_eq!(
        cpu.get_dynamic_property(DynamicPropertyType::Exposure)
            .unwrap_err()
            .message(),
        "Cannot find dynamic property; not used by CPU processor."
    );
}

/// Each job runs on a thread of its own, and every job runs after one fails: the error is the
/// first job's that failed.
#[test]
fn in_threads_runs_every_job_and_returns_the_first_error() {
    let ids = std::sync::Mutex::new(Vec::new());
    let job = |error: Option<&'static str>| {
        let ids = &ids;
        move || {
            ids.lock().unwrap().push(std::thread::current().id());
            std::thread::sleep(std::time::Duration::from_millis(20));
            error.map_or(Ok(()), |e| Err(crate::exception::Exception::new(e)))
        }
    };
    let jobs = [None, Some("second"), Some("third"), None];
    let result = super::in_threads(jobs.into_iter().map(job));
    assert_eq!(result.unwrap_err().message(), "second");
    let mut seen = ids.lock().unwrap().clone();
    assert_eq!(seen.len(), 4);
    seen.sort_by_key(|id| format!("{id:?}"));
    seen.dedup();
    assert_eq!(seen.len(), 4);

    let last_fails = [None, Some("last")];
    let result = super::in_threads(last_fails.into_iter().map(job));
    assert_eq!(result.unwrap_err().message(), "last");
    assert!(super::in_threads([None].into_iter().map(job)).is_ok());
}

/// A job whose thread can't be made runs on the calling thread, in its place among the results.
#[test]
fn a_job_without_a_thread_runs_on_the_calling_thread() {
    let ran = std::sync::Mutex::new(Vec::new());
    let job = |index: usize| {
        let ran = &ran;
        move || {
            ran.lock()
                .unwrap()
                .push((index, std::thread::current().id()));
            match index {
                1 => Err(crate::exception::Exception::new("second")),
                2 => Err(crate::exception::Exception::new("third")),
                _ => Ok(()),
            }
        }
    };
    let result = super::in_threads_where((0..5).map(job), |index| index != 1);
    assert_eq!(result.unwrap_err().message(), "second");

    let here = std::thread::current().id();
    let mut ran = ran.into_inner().unwrap();
    ran.sort_by_key(|(index, _)| *index);
    assert_eq!(
        ran.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    let on_this_thread: Vec<usize> = ran
        .iter()
        .filter(|(_, id)| *id == here)
        .map(|(index, _)| *index)
        .collect();
    assert_eq!(on_this_thread, [1, 4]);
}

/// A host that sets flush-to-zero and denormals-are-zero, or another rounding mode, on its thread
/// gets the serial apply's bytes from the threaded one: on Windows new threads start with the
/// default control word. Subnormal pixels show the flags, pixels whose product doesn't fit show
/// the rounding, and the plain render shows that each makes a difference.
#[cfg(target_arch = "x86_64")]
#[test]
fn threads_render_under_the_control_word_of_the_caller() {
    use crate::image_desc::PackedImageDesc;
    use crate::sse2::scalar::{control_word, set_control_word};

    let mut ops = crate::op::OpVec::new();
    crate::ops::matrix::matrix_op::create_scale_op(
        &mut ops,
        &[1.5, 1.5, 1.5, 1.0],
        TransformDirection::Forward,
    );
    ops.finalize().unwrap();
    let cpu = super::CpuProcessor::new(&ops, BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE)
        .unwrap();
    let render = |value: f32, threads: usize| {
        let mut image = vec![value; 4 * 8 * 8];
        let mut img = PackedImageDesc::new(&mut image[..], 8, 8, 4).unwrap();
        cpu.apply_threaded(&mut img, threads).unwrap();
        image.iter().map(|v| v.to_bits()).collect::<Vec<u32>>()
    };
    let before = control_word();
    let flush_to_zero = 0x8040;
    let round_toward_zero = 0x6000;
    for (bits, value) in [(flush_to_zero, 1e-40f32), (round_toward_zero, 0.1f32)] {
        let plain = render(value, 4);
        set_control_word(before | bits);
        let serial = render(value, 1);
        let threaded = render(value, 4);
        set_control_word(before);
        assert_ne!(plain, serial);
        assert_eq!(threaded, serial);
    }
}
