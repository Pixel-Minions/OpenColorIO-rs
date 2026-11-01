#!/usr/bin/env python
# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Speed of the port against the real wheel on this machine (PLAN.md Phase 8).

Both sides run the same cases: the same config (built here with the wheel's API, so the two
load the same text), the same image (ocioperf's synthetic pattern) and one `apply(src, dst)` of
a whole image per iteration.

The wheel's C++ is timed from Python with `perf_counter_ns` around a single `cpu.apply` call.
Python's cost is one pybind11 call per image, not per pixel: the run measures that call on a
1x1 image (`python_call_ns`) and reports it, so it can be compared with the image times. Building
the image descriptions and everything else stays outside the timed region.

Each process runs pinned to one logical CPU (`--cpu`), so core migration and the two CCDs of
this machine don't enter the times. Each case is timed `--iters` times per process, and the
processes run `--rounds` times, alternating wheel and port. The minimum of all is reported, as
background work only ever adds time.

The output digests of the two sides are compared: a speed comparison of different results
would mean nothing. It is a sanity check, not a parity test (those are the oracle tests).

    python tools/perf-compare/perf_compare.py --port-exe target/perf-port/release/perf-port.exe \
        --out <scratch dir> [--filter REGEX] [--iters N] [--rounds N] [--cpu N] [--size WxH]
"""

import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import PyOpenColorIO as OCIO

BIT_DEPTHS = {
    "u8": (OCIO.BIT_DEPTH_UINT8, np.uint8, 255.0),
    "u10": (OCIO.BIT_DEPTH_UINT10, np.uint16, 1023.0),
    "u12": (OCIO.BIT_DEPTH_UINT12, np.uint16, 4095.0),
    "u16": (OCIO.BIT_DEPTH_UINT16, np.uint16, 65535.0),
    "f16": (OCIO.BIT_DEPTH_F16, np.float16, None),
    "f32": (OCIO.BIT_DEPTH_F32, np.float32, None),
}


def make_lut_files(root):
    """Writes the LUT files of the cases, from smooth functions with cross talk."""
    grid = np.linspace(0.0, 1.0, 4096, dtype=np.float64)
    with open(root / "lut1d_4096.cube", "w", encoding="ascii") as f:
        f.write("LUT_1D_SIZE 4096\n")
        for x in grid:
            v = x**0.8
            f.write(f"{v:.7f} {v * 0.97:.7f} {v * 0.93:.7f}\n")

    for size in (33, 65):
        with open(root / f"lut3d_{size}.cube", "w", encoding="ascii") as f:
            f.write(f"LUT_3D_SIZE {size}\n")
            axis = np.linspace(0.0, 1.0, size)
            for b in axis:
                for g in axis:
                    for r in axis:
                        luma = 0.2 * r + 0.7 * g + 0.1 * b
                        out = [(0.9 * c + 0.1 * luma) ** 1.1 for c in (r, g, b)]
                        f.write(f"{out[0]:.7f} {out[1]:.7f} {out[2]:.7f}\n")

    half = np.arange(65536, dtype=np.uint32).astype(np.uint16)
    values = half.view(np.float16).astype(np.float64)
    finite = np.where(np.isfinite(values), values, 0.0)
    curve = np.clip(np.sign(finite) * np.abs(finite) ** 0.8, -1000.0, 1000.0)
    with open(root / "lut1d_half.clf", "w", encoding="ascii") as f:
        f.write(
            '<?xml version="1.0" encoding="UTF-8"?>\n'
            '<ProcessList compCLFversion="3" id="half">\n'
            '<LUT1D inBitDepth="16f" outBitDepth="32f" halfDomain="true">\n'
            '<Array dim="65536 1">\n'
        )
        f.writelines(f"{v:.7f}\n" for v in curve)
        f.write("</Array>\n</LUT1D>\n</ProcessList>\n")


def grading_primary(style):
    v = OCIO.GradingPrimary(style)
    if style == OCIO.GRADING_LOG:
        v.brightness = OCIO.GradingRGBM(0.05, 0.02, -0.03, 0.01)
        v.contrast = OCIO.GradingRGBM(1.1, 0.95, 1.05, 1.02)
        v.gamma = OCIO.GradingRGBM(1.1, 0.9, 1.02, 0.97)
    elif style == OCIO.GRADING_LIN:
        v.offset = OCIO.GradingRGBM(0.01, 0.02, -0.01, 0.005)
        v.exposure = OCIO.GradingRGBM(0.2, -0.1, 0.1, 0.05)
        v.contrast = OCIO.GradingRGBM(1.1, 0.95, 1.05, 1.02)
    else:
        v.gamma = OCIO.GradingRGBM(1.1, 0.9, 1.02, 0.97)
        v.lift = OCIO.GradingRGBM(0.02, 0.01, -0.01, 0.0)
        v.gain = OCIO.GradingRGBM(1.1, 0.95, 1.05, 1.0)
        v.offset = OCIO.GradingRGBM(0.01, 0.02, -0.01, 0.005)
    v.saturation = 1.1
    v.clampBlack = 0.0
    v.clampWhite = 1.5
    return v


def grading_rgb_curve(style):
    if style == OCIO.GRADING_LOG:
        points = [-7.0, -7.0, -2.0, -1.8, 2.0, 2.2, 8.0, 8.0]
    else:
        points = [-1.0, -1.0, 0.0, 0.05, 1.0, 1.1, 4.0, 4.0]
    v = OCIO.GradingRGBCurve(style)
    v.red = OCIO.GradingBSplineCurve(points)
    v.green = OCIO.GradingBSplineCurve(points)
    v.blue = OCIO.GradingBSplineCurve(points)
    v.master = OCIO.GradingBSplineCurve(points)
    return v


def grading_tone(style):
    v = OCIO.GradingTone(style)
    v.blacks = OCIO.GradingRGBMSW(1.1, 1.0, 0.95, 1.02, v.blacks.start, v.blacks.width)
    v.shadows = OCIO.GradingRGBMSW(1.05, 1.0, 0.9, 1.01, v.shadows.start, v.shadows.width)
    v.midtones = OCIO.GradingRGBMSW(1.1, 0.95, 1.02, 1.0, v.midtones.start, v.midtones.width)
    v.highlights = OCIO.GradingRGBMSW(0.9, 1.05, 1.1, 1.0, v.highlights.start, v.highlights.width)
    v.whites = OCIO.GradingRGBMSW(0.95, 1.05, 1.0, 1.02, v.whites.start, v.whites.width)
    v.scontrast = 1.1
    return v


def grading_hue_curve(style):
    v = OCIO.GradingHueCurve(style)
    v.hue_sat = OCIO.GradingBSplineCurve([0.0, 1.0, 0.25, 1.1, 0.5, 0.9, 0.75, 1.05], OCIO.HUE_SAT)
    v.sat_sat = OCIO.GradingBSplineCurve([0.0, 0.0, 0.5, 0.6, 1.0, 1.0], OCIO.SAT_SAT)
    return v


def matrix():
    # fmt: off
    return OCIO.MatrixTransform([
        1.2, -0.1, -0.1, 0.01,
        -0.05, 1.1, -0.05, 0.02,
        -0.02, -0.08, 1.1, -0.01,
        0.0, 0.0, 0.0, 1.0,
    ])
    # fmt: on


def inverse(t):
    t.setDirection(OCIO.TRANSFORM_DIR_INVERSE)
    return t


def op_transforms():
    """The op families (and the renderer paths inside them) to compare, by case name."""
    ff = OCIO.FixedFunctionTransform
    ec = OCIO.ExposureContrastTransform
    gp = OCIO.GradingPrimaryTransform
    gt = OCIO.GradingToneTransform
    rc = OCIO.GradingRGBCurveTransform
    hc = OCIO.GradingHueCurveTransform

    def cdl_transform(style):
        t = OCIO.CDLTransform(
            slope=[1.1, 0.9, 1.05], offset=[0.01, -0.02, 0.03], power=[1.1, 0.95, 1.05], sat=1.1
        )
        t.setStyle(style)
        return t

    def moncurve(*extra):
        return OCIO.ExponentWithLinearTransform(
            [2.4, 2.4, 2.4, 1.0], [0.055, 0.055, 0.055, 0.0], *extra
        )

    def log_affine():
        t = OCIO.LogAffineTransform(
            [0.1, 0.1, 0.1], [0.5, 0.5, 0.5], [1.0, 1.0, 1.0], [0.01, 0.01, 0.01]
        )
        t.setBase(10.0)
        return t

    def log_camera():
        t = OCIO.LogCameraTransform(
            [0.01, 0.01, 0.01],
            logSideSlope=[0.25, 0.25, 0.25],
            logSideOffset=[0.5, 0.5, 0.5],
            linSideSlope=[5.5, 5.5, 5.5],
            linSideOffset=[0.05, 0.05, 0.05],
        )
        t.setBase(10.0)
        return t

    return {
        "matrix": matrix(),
        "matrix_diag": OCIO.MatrixTransform([1.2, 0, 0, 0, 0, 1.1, 0, 0, 0, 0, 0.9, 0, 0, 0, 0, 1]),
        "range": OCIO.RangeTransform(0.1, 0.9, 0.0, 1.0),
        "cdl_no_clamp": cdl_transform(OCIO.CDL_NO_CLAMP),
        "cdl_asc": cdl_transform(OCIO.CDL_ASC),
        "exponent": OCIO.ExponentTransform([2.2, 2.2, 2.2, 1.0]),
        "exponent_mirror": OCIO.ExponentTransform([2.2, 2.2, 2.2, 1.0], OCIO.NEGATIVE_MIRROR),
        "moncurve": moncurve(),
        "moncurve_inv": inverse(moncurve()),
        "moncurve_mirror": moncurve(OCIO.NEGATIVE_MIRROR),
        "log_base2": OCIO.LogTransform(2.0),
        "log_base2_inv": inverse(OCIO.LogTransform(2.0)),
        "log_affine": log_affine(),
        "log_affine_inv": inverse(log_affine()),
        "log_camera": log_camera(),
        "log_camera_inv": inverse(log_camera()),
        "ec_linear": ec(style=OCIO.EXPOSURE_CONTRAST_LINEAR, exposure=0.5, contrast=1.2, gamma=1.1),
        "ec_video": ec(style=OCIO.EXPOSURE_CONTRAST_VIDEO, exposure=0.5, contrast=1.2, gamma=1.1),
        "ec_log": ec(style=OCIO.EXPOSURE_CONTRAST_LOGARITHMIC, exposure=0.5, contrast=1.2),
        "ff_red_mod_10": ff(OCIO.FIXED_FUNCTION_ACES_RED_MOD_10),
        "ff_glow_10": ff(OCIO.FIXED_FUNCTION_ACES_GLOW_10),
        "ff_dark_to_dim_10": ff(OCIO.FIXED_FUNCTION_ACES_DARK_TO_DIM_10),
        "ff_gamut_comp_13": ff(
            OCIO.FIXED_FUNCTION_ACES_GAMUT_COMP_13,
            params=[1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
        ),
        "ff_rgb_to_hsv": ff(OCIO.FIXED_FUNCTION_RGB_TO_HSV),
        "ff_xyz_to_luv": ff(OCIO.FIXED_FUNCTION_XYZ_TO_LUV),
        "ff_lin_to_pq": ff(OCIO.FIXED_FUNCTION_LIN_TO_PQ),
        "ff_rgb_to_hsy_log": ff(OCIO.FIXED_FUNCTION_RGB_TO_HSY_LOG),
        "ff_aces2_sdr": OCIO.BuiltinTransform(
            "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0"
        ),
        "gp_log": gp(grading_primary(OCIO.GRADING_LOG), OCIO.GRADING_LOG),
        "gp_lin": gp(grading_primary(OCIO.GRADING_LIN), OCIO.GRADING_LIN),
        "gp_video": gp(grading_primary(OCIO.GRADING_VIDEO), OCIO.GRADING_VIDEO),
        "gp_log_inv": inverse(gp(grading_primary(OCIO.GRADING_LOG), OCIO.GRADING_LOG)),
        "gt_log": gt(grading_tone(OCIO.GRADING_LOG), OCIO.GRADING_LOG),
        "gt_lin": gt(grading_tone(OCIO.GRADING_LIN), OCIO.GRADING_LIN),
        "gt_log_inv": inverse(gt(grading_tone(OCIO.GRADING_LOG), OCIO.GRADING_LOG)),
        "rc_log": rc(grading_rgb_curve(OCIO.GRADING_LOG), OCIO.GRADING_LOG),
        "rc_lin": rc(grading_rgb_curve(OCIO.GRADING_LIN), OCIO.GRADING_LIN),
        "rc_log_inv": inverse(rc(grading_rgb_curve(OCIO.GRADING_LOG), OCIO.GRADING_LOG)),
        "hc_log": hc(grading_hue_curve(OCIO.GRADING_LOG), OCIO.GRADING_LOG),
        "lut1d": OCIO.FileTransform("lut1d_4096.cube", interpolation=OCIO.INTERP_LINEAR),
        "lut1d_inv": inverse(
            OCIO.FileTransform("lut1d_4096.cube", interpolation=OCIO.INTERP_LINEAR)
        ),
        "lut1d_half": OCIO.FileTransform("lut1d_half.clf", interpolation=OCIO.INTERP_LINEAR),
        "lut3d_tetra_33": OCIO.FileTransform(
            "lut3d_33.cube", interpolation=OCIO.INTERP_TETRAHEDRAL
        ),
        "lut3d_tetra_65": OCIO.FileTransform(
            "lut3d_65.cube", interpolation=OCIO.INTERP_TETRAHEDRAL
        ),
        "lut3d_linear_33": OCIO.FileTransform("lut3d_33.cube", interpolation=OCIO.INTERP_LINEAR),
        "lut3d_inv_33": inverse(
            OCIO.FileTransform("lut3d_33.cube", interpolation=OCIO.INTERP_TETRAHEDRAL)
        ),
    }


# The cases checked at the other pixel formats: no op at all, a cheap op and two tables.
FORMAT_OPS = ("noop", "matrix", "lut1d", "lut3d_tetra_33")
FORMATS = (
    ("u8", "u8"),
    ("u10", "u10"),
    ("u12", "u12"),
    ("u16", "u16"),
    ("f16", "f16"),
    ("u8", "f32"),
    ("u10", "f32"),
    ("u16", "f32"),
    ("f16", "f32"),
    ("f32", "u8"),
    ("f32", "u10"),
    ("f32", "u16"),
    ("f32", "f16"),
)


def build_config(root):
    """Writes `perf.ocio`, one color space per op case, and returns the case names."""
    make_lut_files(root)
    config = OCIO.Config.CreateRaw()
    config.setVersion(2, 5)
    config.setSearchPath(".")
    names = []
    for name, transform in op_transforms().items():
        space = OCIO.ColorSpace(name=name)
        space.setTransform(transform, OCIO.COLORSPACE_DIR_FROM_REFERENCE)
        config.addColorSpace(space)
        names.append(name)
    config.addColorSpace(OCIO.ColorSpace(name="noop"))
    config.addColorSpace(OCIO.ColorSpace(name="ref"))
    path = root / "perf.ocio"
    path.write_text(config.serialize(), encoding="utf-8")
    return names


def build_cases(root, names):
    perf = str(root / "perf.ocio")
    cases = [
        {"name": n, "config": perf, "src": "ref", "dst": n, "in": "f32", "out": "f32"}
        for n in names
    ]
    for depth_in, depth_out in FORMATS:
        for op in FORMAT_OPS:
            cases.append(
                {
                    "name": f"{op}/{depth_in}-{depth_out}",
                    "config": perf,
                    "src": "ref",
                    "dst": op,
                    "in": depth_in,
                    "out": depth_out,
                }
            )
    cases.append(
        {
            "name": "chain_aces2_srgb",
            "config": "ocio://studio-config-latest",
            "src": "ACEScg",
            "display": "sRGB - Display",
            "view": "ACES 2.0 - SDR 100 nits (Rec.709)",
            "in": "f32",
            "out": "f32",
        }
    )
    return cases


def image(depth, width, height):
    """ocioperf's synthetic image (src/apps/ocioperf/main.cpp:533-587 @ v2.5.2)."""
    length = 201
    step = np.float32(1.0) / (np.float32(length) - np.float32(1.0))
    count = width * height
    idx = np.arange(count, dtype=np.uint64)
    r = ((idx // length // length) % length).astype(np.float32) * step
    g = ((idx // length) % length).astype(np.float32) * step
    b = (idx % length).astype(np.float32) * step
    a = idx.astype(np.float32) / np.float32(count)
    unit = np.stack([r, g, b, a], axis=1).reshape(-1)
    _, dtype, scale = BIT_DEPTHS[depth]
    if scale is None:
        return (unit * np.float32(3.0) + np.float32(-1.0)).astype(np.float32).astype(dtype)
    return (unit * np.float32(scale)).astype(dtype)


def digest(buffer):
    """A wrapping sum and an xor of the 64-bit words, as `src/main.rs` computes them."""
    raw = buffer.tobytes()
    words = np.frombuffer(raw[: len(raw) // 8 * 8], dtype=np.uint64)
    return f"{int(words.sum(dtype=np.uint64)):016x}{int(np.bitwise_xor.reduce(words)):016x}"


def desc(array, depth, width, height):
    bd = BIT_DEPTHS[depth][0]
    return OCIO.PackedImageDesc(
        array, width, height, 4, bd, OCIO.AutoStride, OCIO.AutoStride, OCIO.AutoStride
    )


def wheel_child(plan_path, out_path):
    plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
    width, height, iters = plan["width"], plan["height"], plan["iters"]
    configs = {}
    images = {}
    results = {}

    # The cost of the Python call itself, on a 1x1 image.
    one = np.zeros(4, dtype=np.float32)
    one_out = np.zeros(4, dtype=np.float32)
    raw = OCIO.Config.CreateRaw()
    processor = raw.getProcessor(matrix()).getDefaultCPUProcessor()
    one_src, one_dst = desc(one, "f32", 1, 1), desc(one_out, "f32", 1, 1)
    times = []
    for _ in range(2000):
        t0 = time.perf_counter_ns()
        processor.apply(one_src, one_dst)
        times.append(time.perf_counter_ns() - t0)
    python_call_ns = int(sorted(times)[len(times) // 2])

    for case in plan["cases"]:
        name = case["config"]
        if name not in configs:
            if name.startswith("ocio://"):
                configs[name] = OCIO.Config.CreateFromBuiltinConfig(name)
            else:
                configs[name] = OCIO.Config.CreateFromFile(name)
        config = configs[name]
        if "display" in case:
            processor = config.getProcessor(
                case["src"], case["display"], case["view"], OCIO.TRANSFORM_DIR_FORWARD
            )
        else:
            processor = config.getProcessor(case["src"], case["dst"])
        cpu = processor.getOptimizedCPUProcessor(
            BIT_DEPTHS[case["in"]][0], BIT_DEPTHS[case["out"]][0], OCIO.OPTIMIZATION_DEFAULT
        )
        if case["in"] not in images:
            images[case["in"]] = image(case["in"], width, height)
        src = images[case["in"]]
        dst = np.zeros(src.shape, dtype=BIT_DEPTHS[case["out"]][1])
        src_desc = desc(src, case["in"], width, height)
        dst_desc = desc(dst, case["out"], width, height)
        times = []
        for _ in range(iters):
            t0 = time.perf_counter_ns()
            cpu.apply(src_desc, dst_desc)
            times.append(time.perf_counter_ns() - t0)
        results[case["name"]] = {"ns": min(times), "src": digest(src), "dst": digest(dst)}

    Path(out_path).write_text(
        json.dumps({"python_call_ns": python_call_ns, "cases": results}), encoding="utf-8"
    )


OCIOPERF_LABELS = {
    "in_place": "(in place):",
    "two_buffers": "(two buffers):",
    "lines": "line by line:",
    "pixels": "pixel per pixel:",
}


def ocioperf_table(args, root, cases):
    """The wheel's own ocioperf against the port, on ocioperf's 4K image (PLAN.md Phase 8)."""
    tool = Path(OCIO.__file__).parent / "bin" / ("ocioperf.exe" if sys.platform == "win32" else "ocioperf")
    plan = {"width": 3840, "height": 2160, "iters": args.iters, "ocioperf": True, "cases": cases}
    plan_path = root / "plan-ocioperf.json"
    plan_path.write_text(json.dumps(plan), encoding="utf-8")
    port_out = root / "port-ocioperf.json"
    subprocess.run(pinned(args.cpu, [args.port_exe, str(plan_path), str(port_out)]), check=True)
    port = json.loads(port_out.read_text(encoding="utf-8"))["cases"]

    rows = [
        "| case | kind | ocioperf ms | port ms | port/wheel |",
        "|---|---|---:|---:|---:|",
    ]
    for case in cases:
        command = [
            str(tool),
            "--iconfig",
            case["config"],
            "--colorspaces",
            case["src"],
            case["dst"],
            "--iter",
            str(args.iters),
            "--test",
            "-1",
        ]
        text = subprocess.run(
            pinned(args.cpu, command), check=True, capture_output=True, text=True
        ).stdout
        for kind, label in OCIOPERF_LABELS.items():
            line = next(x for x in text.splitlines() if label in x)
            # "[first, mean of the rest, mean of all] ms"
            wheel = float(re.search(r"\[[^,]+, ([^,]+),", line).group(1))
            ours = port[case["name"]][kind]
            rows.append(f"| {case['name']} | {kind} | {wheel:.2f} | {ours:.2f} | {ours / wheel:.2f} |")
    text = chr(10).join(rows)
    print(text)
    label = f"-{args.label}" if args.label else ""
    (root / f"ocioperf{label}.md").write_text(text + chr(10), encoding="utf-8")


def pinned(cpu, command):
    if sys.platform == "win32":
        mask = f"{1 << cpu:x}"
        return ["cmd", "/c", "start", "", "/b", "/wait", "/affinity", mask, *command]
    return ["taskset", "-c", str(cpu), *command]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel-child", nargs=2)
    parser.add_argument("--port-exe")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--filter", default="")
    parser.add_argument("--iters", type=int, default=5)
    parser.add_argument("--rounds", type=int, default=2)
    parser.add_argument("--cpu", type=int, default=12)
    parser.add_argument("--size", default="1920x1080")
    parser.add_argument("--label", default="")
    parser.add_argument("--ocioperf", action="store_true", help="only the ocioperf table")
    args = parser.parse_args()

    if args.wheel_child:
        wheel_child(*args.wheel_child)
        return

    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=True)
    names = build_config(root)
    width, height = (int(v) for v in args.size.split("x"))
    cases = [
        c for c in build_cases(root, names) if not args.filter or re.search(args.filter, c["name"])
    ]
    if args.ocioperf:
        ocioperf_table(args, root, cases)
        return

    plan = {"width": width, "height": height, "iters": args.iters, "cases": cases}
    plan_path = root / "plan.json"
    plan_path.write_text(json.dumps(plan), encoding="utf-8")

    best = {"wheel": {}, "port": {}}
    python_call_ns = None
    for round_ in range(args.rounds):
        wheel_out = root / f"wheel-{round_}.json"
        port_out = root / f"port-{round_}.json"
        wheel_cmd = [sys.executable, __file__, "--wheel-child", str(plan_path), str(wheel_out)]
        subprocess.run(pinned(args.cpu, wheel_cmd), check=True)
        subprocess.run(pinned(args.cpu, [args.port_exe, str(plan_path), str(port_out)]), check=True)
        for side, path in (("wheel", wheel_out), ("port", port_out)):
            data = json.loads(path.read_text(encoding="utf-8"))
            python_call_ns = data.get("python_call_ns", python_call_ns)
            for name, entry in data["cases"].items():
                if name not in best[side] or entry["ns"] < best[side][name]["ns"]:
                    best[side][name] = entry

    megapixels = width * height / 1e6
    print(f"image {width}x{height}, cpu {args.cpu}, {args.iters} x {args.rounds} runs")
    print(f"python call overhead: {python_call_ns / 1e3:.1f} us per apply")
    print()
    rows = [
        "| case | wheel ms | port ms | port/wheel | wheel ns/px | port ns/px | bits |",
        "|---|---:|---:|---:|---:|---:|---|",
    ]
    for case in cases:
        name = case["name"]
        wheel, port = best["wheel"][name], best["port"][name]
        same = "same" if wheel["dst"] == port["dst"] else "DIFFER"
        if wheel["src"] != port["src"]:
            same = "SRC DIFFERS"
        ratio = port["ns"] / wheel["ns"]
        rows.append(
            f"| {name} | {wheel['ns'] / 1e6:.2f} | {port['ns'] / 1e6:.2f} | {ratio:.2f} "
            f"| {wheel['ns'] / 1e6 / megapixels:.2f} | {port['ns'] / 1e6 / megapixels:.2f} "
            f"| {same} |"
        )
    text = "\n".join(rows)
    print(text)
    label = f"-{args.label}" if args.label else ""
    (root / f"results{label}.json").write_text(json.dumps(best, indent=1), encoding="utf-8")
    (root / f"results{label}.md").write_text(text + "\n", encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
