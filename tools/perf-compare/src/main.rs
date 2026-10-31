// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's side of `tools/perf-compare/perf_compare.py`: runs the plan that script writes, one
//! `apply(src, dst)` of a whole image per iteration, and writes the fastest time of each case
//! and digests of its input and output. See that script for the method.
//!
//! The image is ocioperf's synthetic pattern (src/apps/ocioperf/main.cpp:533-587 @ v2.5.2), and
//! the case list is the script's. Nothing here is an expected value of a test.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::time::Instant;

use ocio::{Config, TransformDirection};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::image_desc::{AUTO_STRIDE, Bytes, PackedImageDesc};
use ocio_ops::open_color_types::{BitDepth, OptimizationFlags};
use serde_json::{Value, json};

fn bit_depth(name: &str) -> (BitDepth, usize) {
    match name {
        "u8" => (BitDepth::Uint8, 1),
        "u10" => (BitDepth::Uint10, 2),
        "u12" => (BitDepth::Uint12, 2),
        "u16" => (BitDepth::Uint16, 2),
        "f16" => (BitDepth::F16, 2),
        "f32" => (BitDepth::F32, 4),
        _ => panic!("unknown bit depth {name}"),
    }
}

/// The pattern's values in `[0, 1]`, for the pixel `idx` of `count`.
fn unit_pixel(idx: usize, count: usize) -> [f32; 4] {
    const LENGTH: usize = 201;
    let step = 1.0f32 / (LENGTH as f32 - 1.0f32);
    [
        ((idx / LENGTH / LENGTH) % LENGTH) as f32 * step,
        ((idx / LENGTH) % LENGTH) as f32 * step,
        (idx % LENGTH) as f32 * step,
        idx as f32 / count as f32,
    ]
}

/// The image at `depth`, as packed RGBA bytes.
fn image(depth: &str, width: usize, height: usize) -> Vec<u8> {
    let count = width * height;
    let mut bytes = Vec::new();
    let scale = match depth {
        "u8" => 255.0f32,
        "u10" => 1023.0,
        "u12" => 4095.0,
        "u16" => 65535.0,
        _ => 0.0,
    };
    for idx in 0..count {
        for v in unit_pixel(idx, count) {
            match depth {
                "u8" => bytes.push((v * scale) as u8),
                "u10" | "u12" | "u16" => {
                    bytes.extend_from_slice(&((v * scale) as u16).to_ne_bytes());
                }
                "f16" => {
                    let wide = v * 3.0 - 1.0;
                    bytes.extend_from_slice(&half::f16::from_f32(wide).to_bits().to_ne_bytes());
                }
                _ => bytes.extend_from_slice(&(v * 3.0 - 1.0).to_ne_bytes()),
            }
        }
    }
    bytes
}

/// A wrapping sum and an xor of the 64-bit words, as the script computes them.
fn digest(bytes: &[u8]) -> String {
    let mut sum = 0u64;
    let mut xor = 0u64;
    for word in bytes.as_chunks::<8>().0 {
        let word = u64::from_ne_bytes(*word);
        sum = sum.wrapping_add(word);
        xor ^= word;
    }
    format!("{sum:016x}{xor:016x}")
}

fn processor(configs: &mut BTreeMap<String, Arc<Config>>, case: &Value) -> Arc<CpuProcessor> {
    let name = case["config"].as_str().expect("a config name");
    let config = configs.entry(name.to_owned()).or_insert_with(|| {
        if name.starts_with("ocio://") {
            Config::create_from_builtin_config(name).expect("the built-in config")
        } else {
            Config::create_from_file(name).expect("the config")
        }
    });
    let src = case["src"].as_str().expect("a source color space");
    let processor = if let Some(display) = case["display"].as_str() {
        let view = case["view"].as_str().expect("a view");
        config
            .processor_for_display_view(src, display, view, TransformDirection::Forward)
            .expect("the display processor")
    } else {
        let dst = case["dst"].as_str().expect("a destination color space");
        config.processor_between(src, dst).expect("the processor")
    };
    let depth_in = bit_depth(case["in"].as_str().expect("a bit depth")).0;
    let depth_out = bit_depth(case["out"].as_str().expect("a bit depth")).0;
    processor
        .optimized_cpu_processor_with_bit_depths(depth_in, depth_out, OptimizationFlags::DEFAULT)
        .expect("the CPU processor")
}

/// A packed F32 RGBA description of `data`.
fn desc(data: &mut [u8], width: usize, height: usize) -> PackedImageDesc<&mut [u8]> {
    let auto = AUTO_STRIDE;
    PackedImageDesc::with_strides(
        Bytes(data),
        width,
        height,
        4,
        BitDepth::F32,
        auto,
        auto,
        auto,
    )
    .expect("an image")
}

/// What ocioperf reports for a F32 image (src/apps/ocioperf/main.cpp:569-700 @ v2.5.2): the mean
/// time of all iterations but the first, in milliseconds, of the four kinds of processing.
fn ocioperf(cpu: &CpuProcessor, width: usize, height: usize, iters: u64) -> Value {
    let reference = image("f32", width, height);
    let auto = AUTO_STRIDE;
    let row = width * 16;
    let mean = |times: &[u128]| {
        let rest = &times[1.min(times.len() - 1)..];
        rest.iter().sum::<u128>() as f64 / rest.len() as f64 / 1e6
    };

    let mut times = Vec::new();
    for _ in 0..iters {
        let mut data = reference.clone();
        let mut img = desc(&mut data, width, height);
        let start = Instant::now();
        cpu.apply(&mut img).expect("the apply");
        times.push(start.elapsed().as_nanos());
    }
    let in_place = mean(&times);

    let mut dst_data = reference.clone();
    let src = PackedImageDesc::with_strides(
        Bytes(&reference[..]),
        width,
        height,
        4,
        BitDepth::F32,
        auto,
        auto,
        auto,
    )
    .expect("an image");
    let mut dst = desc(&mut dst_data, width, height);
    let mut times = Vec::new();
    for _ in 0..iters {
        let start = Instant::now();
        cpu.apply_src_dst(&src, &mut dst).expect("the apply");
        times.push(start.elapsed().as_nanos());
    }
    let two_buffers = mean(&times);

    let mut times = Vec::new();
    for _ in 0..iters {
        let mut data = reference.clone();
        let start = Instant::now();
        for line in data.chunks_mut(row) {
            let mut img = desc(line, width, 1);
            cpu.apply(&mut img).expect("the apply");
        }
        times.push(start.elapsed().as_nanos());
    }
    let lines = mean(&times);

    let mut times = Vec::new();
    for _ in 0..iters {
        let mut pixels: Vec<[f32; 4]> = reference
            .as_chunks::<16>()
            .0
            .iter()
            .map(|p| {
                let mut value = [0.0f32; 4];
                for (v, b) in value.iter_mut().zip(p.as_chunks::<4>().0) {
                    *v = f32::from_ne_bytes(*b);
                }
                value
            })
            .collect();
        let start = Instant::now();
        for pixel in &mut pixels {
            cpu.apply_rgba(pixel).expect("the apply");
        }
        times.push(start.elapsed().as_nanos());
    }
    let pixels = mean(&times);

    json!({ "in_place": in_place, "two_buffers": two_buffers, "lines": lines, "pixels": pixels })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let plan: Value =
        serde_json::from_str(&fs::read_to_string(&args[1]).expect("the plan")).expect("JSON");
    let width = plan["width"].as_u64().expect("a width") as usize;
    let height = plan["height"].as_u64().expect("a height") as usize;
    let iters = plan["iters"].as_u64().expect("a count");

    let mut configs = BTreeMap::new();
    let mut images: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut results = serde_json::Map::new();

    for case in plan["cases"].as_array().expect("cases") {
        let cpu = processor(&mut configs, case);
        if plan["ocioperf"].as_bool() == Some(true) {
            let name = case["name"].as_str().expect("a name").to_owned();
            results.insert(name, ocioperf(&cpu, width, height, iters));
            continue;
        }
        let depth_in = case["in"].as_str().expect("a bit depth");
        let depth_out = case["out"].as_str().expect("a bit depth");
        let (bd_in, _) = bit_depth(depth_in);
        let (bd_out, out_size) = bit_depth(depth_out);
        let src = images
            .entry(depth_in.to_owned())
            .or_insert_with(|| image(depth_in, width, height));
        let mut dst = vec![0u8; width * height * 4 * out_size];
        let ns = {
            let auto = AUTO_STRIDE;
            let src_desc = PackedImageDesc::with_strides(
                Bytes(&src[..]),
                width,
                height,
                4,
                bd_in,
                auto,
                auto,
                auto,
            )
            .expect("a source image");
            let mut dst_desc = PackedImageDesc::with_strides(
                Bytes(&mut dst[..]),
                width,
                height,
                4,
                bd_out,
                auto,
                auto,
                auto,
            )
            .expect("a destination image");
            let mut best = u128::MAX;
            for _ in 0..iters {
                let start = Instant::now();
                cpu.apply_src_dst(&src_desc, &mut dst_desc)
                    .expect("the apply");
                best = best.min(start.elapsed().as_nanos());
            }
            best
        };
        results.insert(
            case["name"].as_str().expect("a name").to_owned(),
            json!({ "ns": ns as u64, "src": digest(src), "dst": digest(&dst) }),
        );
    }

    fs::write(
        &args[2],
        serde_json::to_string(&json!({ "cases": results })).expect("JSON"),
    )
    .expect("the results");
}
