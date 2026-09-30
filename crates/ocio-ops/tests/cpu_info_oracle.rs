// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Spike S4: the port's CPU detection against the wheel's own `ociocpuinfo`.
//!
//! The wheels ship `ociocpuinfo`, which compiles upstream's CPUInfo.cpp with the library's
//! generated `CPUInfoConfig.h` (src/apps/ociocpuinfo/main.cpp @ v2.5.2). Its `has*()` lines
//! combine this CPU's flags with the build's `OCIO_USE_*` switches, so they check both the
//! port's CPUID decoding and its `BuildConfig`. The `cpu_info` oracle command runs it.

use ocio_ops::cpu_info::CpuInfo;
use ocio_testkit::Oracle;
use serde_json::json;

/// The bytes `ociocpuinfo` printed for a string field: the oracle decodes the app's output as
/// Latin-1, so every character is one byte.
fn latin1_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).expect("a Latin-1 character"))
        .collect()
}

#[test]
fn cpu_info_matches_ociocpuinfo() {
    let resp = Oracle::get().call("cpu_info", json!({}), &[]);
    let fields = resp.result["fields"]
        .as_object()
        .unwrap_or_else(|| panic!("ociocpuinfo fields: {}", resp.result));
    let field = |name: &str| {
        fields
            .get(name)
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("ociocpuinfo printed no `{name}`: {}", resp.result))
    };

    let cpu = CpuInfo::instance();
    assert_eq!(
        latin1_bytes(field("name")),
        cpu.name_bytes(),
        "getName(): wheel {:?}, port {:?}",
        field("name"),
        cpu.name()
    );
    assert_eq!(
        latin1_bytes(field("vendor")),
        cpu.vendor_bytes(),
        "getVendor(): wheel {:?}, port {:?}",
        field("vendor"),
        cpu.vendor()
    );

    // main.cpp prints each answer as `std::cout << bool`: "0" or "1".
    let flags = [
        ("hasSSE2", cpu.has_sse2()),
        ("SSE2Slow", cpu.sse2_slow()),
        ("hasSSE3", cpu.has_sse3()),
        ("SSE3Slow", cpu.sse3_slow()),
        ("hasSSSE3", cpu.has_ssse3()),
        ("SSSE3Slow", cpu.ssse3_slow()),
        ("hasSSE4", cpu.has_sse4()),
        ("hasSSE42", cpu.has_sse42()),
        ("hasAVX", cpu.has_avx()),
        ("AVXSlow", cpu.avx_slow()),
        ("hasAVX2", cpu.has_avx2()),
        ("AVX2SlowGather", cpu.avx2_slow_gather()),
        ("hasAVX512", cpu.has_avx512()),
        ("hasF16C", cpu.has_f16c()),
    ];
    for (name, port) in flags {
        let port = if port { "1" } else { "0" };
        assert_eq!(
            field(name),
            port,
            "{name}: wheel {}, port {port}",
            field(name)
        );
    }
    assert_eq!(
        fields.len(),
        flags.len() + 2,
        "ociocpuinfo printed fields the test doesn't know: {}",
        resp.result
    );
}
