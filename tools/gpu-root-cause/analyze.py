# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Summarize direct platform observations, without asserting non-finite results."""
import array
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(sys.argv[1])
F32 = struct.Struct("<f")
sha = lambda raw: hashlib.sha256(raw).hexdigest()


def floats(path):
    raw = path.read_bytes()
    assert len(raw) % 4 == 0
    values = array.array("f", raw)
    assert values.itemsize == 4 and sys.byteorder == "little"
    return raw, values


def classify_hex(raw):
    value = F32.unpack(bytes.fromhex(raw)[::-1])[0]
    if math.isnan(value):
        return "NaN"
    if math.isinf(value):
        return "+Inf" if value > 0 else "-Inf"
    return "finite"


profiles = {}
for optimization in ("O0", "O2"):
    directory = ROOT / optimization
    results = json.loads((directory / "results.json").read_text())
    assert results["completed"] and len(results["rows"]) == 252
    assert "llvmpipe" in results["renderer"]
    assert "Mesa 25.2.7" in results["version"]
    rows = {row["name"]: row for row in results["rows"]}
    assert len(rows) == 252
    for name, row in rows.items():
        assert row["upload_exact"] and row["gl_error"] == 0
        row["first_pixel_classes"] = [classify_hex(v) for v in row["first_output_bits"][:4]]
        row["output_sha256"] = sha((directory / "raw" / name / "output.bin").read_bytes())
        row["shader_sha256"] = sha((directory / "raw" / name / "fragment.glsl").read_bytes())
    for filtering in ("linear", "nearest"):
        captured = directory / "raw" / ("actual_identity_" + filtering)
        input_raw, source = floats(captured / "input.bin")
        assert sha(input_raw) == "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af"
        output_raw, sampled = floats(captured / "output.bin")
        changed = [i for i in range(len(source)) if input_raw[i*4:i*4+4] != output_raw[i*4:i*4+4]]
        row = rows["actual_identity_" + filtering]
        row["input_changed_components"] = changed
        row["sampled_component_classes"] = [{"component": i, "input_bits": input_raw[i*4:i*4+4][::-1].hex(), "output_bits": output_raw[i*4:i*4+4][::-1].hex()} for i in changed]
        coordinate_raw, coordinates = floats(directory / "raw" / ("actual_coordinate_echo_" + filtering) / "output.bin")
        assert all(math.isfinite(value) for value in coordinates)
        for y in range(256):
            for x in range(256):
                expected = ((x+0.5)/256, (y+0.5)/256, 0, 1)
                for channel, value in enumerate(expected):
                    i = (y*256+x)*4+channel
                    assert coordinate_raw[i*4:i*4+4] == F32.pack(value), (optimization, filtering, x, y, channel)
        rows["actual_coordinate_echo_" + filtering]["all_65536_coordinates_exact_finite_pixel_centers"] = True
    finite_controls = [row for name, row in rows.items() if name.startswith("finite_identity") or "_finite" in name]
    profiles[optimization] = {"renderer": results["renderer"], "version": results["version"], "glsl": results["glsl"], "probe_rows": 252, "all_uploads_exact_all_gl_errors_zero": True, "finite_control_rows": len(finite_controls), "rows": rows}

same = all(profiles["O0"]["rows"][name]["output_sha256"] == profiles["O2"]["rows"][name]["output_sha256"] for name in profiles["O0"]["rows"])
summary = {"purpose": "Platform diagnostic evidence only; no port tests accepted or waived", "probe_rows_per_profile": 252, "profiles": ["O0", "O2"], "all_uploads_exact_all_gl_errors_zero": True, "default_upstream_input_sha256_verified": True, "all_actual_coordinates_finite_exact_pixel_centers": True, "o0_o2_all_output_bytes_identical": same, "renderer": profiles["O0"]["renderer"], "version": profiles["O0"]["version"], "representative_outputs": {name: profiles["O0"]["rows"][name]["first_output_bits"][:4] for name in ("neighbor_1_linear", "neighbor_1_nearest", "echo_special1_channel0", "matrix_uniform_special1_channel0", "matrix_literal_special1_channel0", "scalar_uniform_zero_special1_channel0", "scalar_literal_zero_special1_channel0", "matrix_uniform_special1_channel0_glsl450_bits")}, "actual_linear_sampling_changes": profiles["O0"]["rows"]["actual_identity_linear"]["sampled_component_classes"], "actual_nearest_sampling_changes": profiles["O0"]["rows"]["actual_identity_nearest"]["sampled_component_classes"]}
(ROOT / "analysis.json").write_text(json.dumps({"summary": summary, "profiles": profiles}, indent=2) + "\n")
print(json.dumps(summary, indent=2))
