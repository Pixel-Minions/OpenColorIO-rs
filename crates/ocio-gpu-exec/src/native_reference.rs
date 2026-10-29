// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Independent platform reference required by the owner's 2026-10-29 class rule.
//! The C++ program calls EGL/GL directly; it shares no Rust readback or OCIO code.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use ocio_gpu::GpuShaderDesc;
use ocio_ops::{Exception, Result};
use ocio_testkit::upstream::gpu::{ENTRIES, HEIGHT, WIDTH, input_digest};
use serde_json::Value;

use crate::{Components, GlInfo, HeadlessApp};

pub(super) struct Proof {
    pub info: GlInfo,
    pub directory: PathBuf,
}

fn error(value: impl ToString) -> Exception {
    Exception::new(value.to_string())
}

fn read_values(path: &Path) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path).map_err(error)?;
    if bytes.len() != ENTRIES * 4 {
        return Err(error("Incomplete native image"));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_bits(u32::from_le_bytes(*v)))
        .collect())
}

fn exact(actual: &[f32], reference: &[f32]) -> Result<()> {
    if actual.len() != reference.len()
        || actual
            .iter()
            .zip(reference)
            .any(|(a, b)| a.to_bits() != b.to_bits())
    {
        return Err(error("Independent native/Rust GPU readback bits differ"));
    }
    Ok(())
}

fn run(mut command: Command, directory: &Path, label: &str) -> Result<()> {
    let stdout = std::fs::File::create(directory.join(format!("{label}.stdout"))).map_err(error)?;
    let stderr = std::fs::File::create(directory.join(format!("{label}.stderr"))).map_err(error)?;
    let status = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .status()
        .map_err(error)?;
    if !status.success() {
        return Err(error(format!(
            "Independent native GPU {label} failed; evidence {}",
            directory.display()
        )));
    }
    Ok(())
}

fn verify() -> Result<Proof> {
    let root = ocio_testkit::paths::workspace_root().join("kept/gpu-validation");
    if root.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("target")
    }) {
        return Err(error(
            "GPU evidence must be outside target; use CI checkout",
        ));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_nanos();
    let directory = root.join(format!(
        "{}-{}-{stamp}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).map_err(error)?;
    let executable = directory.join("native-nonfinite");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native/nonfinite.cpp");
    let mut compiler = Command::new("g++");
    compiler
        .args([
            "-std=c++17",
            if cfg!(debug_assertions) { "-O0" } else { "-O2" },
            "-ffp-contract=off",
            "-Wall",
            "-Wextra",
            "-Werror",
        ])
        .arg(&source)
        .args(["-ldl", "-o"])
        .arg(&executable);
    run(compiler, &directory, "build")?;
    let mut native = Command::new("timeout");
    native
        .args(["--kill-after=5s", "120"])
        .arg(&executable)
        .arg(directory.join("raw"))
        .arg("--generate-upstream-input");
    run(native, &directory, "probe")?;
    let report: Value =
        serde_json::from_slice(&std::fs::read(directory.join("probe.stdout")).map_err(error)?)
            .map_err(error)?;
    let rows = report["rows"]
        .as_array()
        .ok_or_else(|| error("Native probe has no rows"))?;
    if report["completed"] != true
        || rows.len() != 260
        || rows
            .iter()
            .any(|row| row["gl_error"] != 0 || row["upload_exact"] != true)
    {
        return Err(error("Native GPU platform controls incomplete"));
    }
    let mut app = HeadlessApp::new(10, 10)?;
    let info = app.info()?;
    for (field, actual) in [
        ("vendor", &info.vendor),
        ("renderer", &info.renderer),
        ("version", &info.version),
        ("glsl", &info.shading_language_version),
        ("egl_vendor", &info.egl_vendor),
        ("egl_version", &info.egl_version),
    ] {
        if report[field].as_str() != Some(actual.as_str()) {
            return Err(error("Native/Rust renderer identity differs"));
        }
    }
    if report["max_texture_size"].as_i64() != Some(i64::from(info.max_texture_size)) {
        return Err(error("Native/Rust GL limit differs"));
    }
    let profiles = [
        (
            "actual_identity",
            "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        ),
        (
            "profile_nan_only",
            "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        ),
        (
            "profile_inf_only",
            "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
        ),
        (
            "profile_finite_wide",
            "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
        ),
        (
            "profile_finite_unit",
            "394608a63bca0842914575c2b977347e75db40ea3f5f085a2b788687effaf4d2",
        ),
    ];
    for (name, digest) in profiles {
        let folder = directory.join("raw").join(format!("{name}_linear"));
        let input = read_values(&folder.join("input.bin"))?;
        let expected = read_values(&folder.join("output.bin"))?;
        if input_digest(&input) != digest {
            return Err(error("Native upstream input digest differs"));
        }
        app.init_image(WIDTH as i32, HEIGHT as i32, Components::Rgba, &input)?;
        app.create_gl_buffers()?;
        app.reshape(WIDTH as i32, HEIGHT as i32)?;
        let mut desc = GpuShaderDesc::default();
        desc.create_shader_text(
            "",
            b"",
            b"",
            b"vec4 OCIOMain(vec4 inPixel)\n{\n",
            "return inPixel;",
            b"\n}\n",
        );
        app.set_shader(Arc::new(desc))?;
        app.redisplay()?;
        let actual = app.read_image()?;
        exact(&actual, &expected)?;
        std::fs::write(
            folder.join("rust-output.bin"),
            actual
                .iter()
                .flat_map(|v| v.to_bits().to_le_bytes())
                .collect::<Vec<_>>(),
        )
        .map_err(error)?;
    }
    std::fs::write(
        directory.join("VERIFIED"),
        "native/Rust full images agree on all five profiles; 260 native controls complete\n",
    )
    .map_err(error)?;
    Ok(Proof { info, directory })
}

pub(super) fn controls() -> Result<&'static Proof> {
    static PROOF: OnceLock<std::result::Result<Proof, String>> = OnceLock::new();
    match PROOF.get_or_init(|| verify().map_err(|e| e.to_string())) {
        Ok(proof) if proof.directory.join("VERIFIED").is_file() => Ok(proof),
        Ok(_) => Err(error("Independent native GPU proof was removed")),
        Err(message) => Err(error(message)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_rust_special_readback_agree_on_five_complete_images() {
        let proof = controls().unwrap();
        assert!(proof.directory.join("VERIFIED").is_file());
    }

    #[test]
    fn shared_readback_sanitization_and_payload_changes_are_rejected() {
        let proof = controls().unwrap();
        let reference = read_values(
            &proof
                .directory
                .join("raw/actual_identity_linear/output.bin"),
        )
        .unwrap();
        let mut sanitized = reference.clone();
        for pixel in sanitized.as_chunks_mut::<4>().0 {
            if pixel.iter().any(|v| !v.is_finite()) {
                pixel.fill(0.0);
            }
        }
        assert!(exact(&reference, &reference).is_ok());
        assert!(exact(&sanitized, &reference).is_err());
        let mut changed = reference.clone();
        let i = changed.iter().position(|v| v.is_nan()).unwrap();
        changed[i] = f32::from_bits(changed[i].to_bits() ^ 1);
        assert!(exact(&changed, &reference).is_err());
    }
}
