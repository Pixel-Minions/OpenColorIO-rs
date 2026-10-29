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
    assert results["completed"] and len(results["rows"]) == 260
    assert "llvmpipe" in results["renderer"]
    assert "Mesa 25.2.7" in results["version"]
    rows = {row["name"]: row for row in results["rows"]}
    assert len(rows) == 260
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
    image_profiles = {}
    for name, digest in {"actual_identity": "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af", "profile_nan_only": "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642", "profile_inf_only": "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489", "profile_finite_wide": "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94", "profile_finite_unit": "394608a63bca0842914575c2b977347e75db40ea3f5f085a2b788687effaf4d2"}.items():
        images = {}
        for filtering in ("linear", "nearest"):
            folder = directory / "raw" / (name + "_" + filtering)
            input_raw, values = floats(folder / "input.bin")
            output_raw, sampled = floats(folder / "output.bin")
            assert sha(input_raw) == digest
            changed = [i for i in range(len(values)) if input_raw[i*4:i*4+4] != output_raw[i*4:i*4+4]]
            images[filtering] = {"input_sha256": digest, "output_sha256": sha(output_raw), "changed_components": changed, "source_nonfinite_components": [i for i, v in enumerate(values) if not math.isfinite(v)], "sampled_nonfinite_components": [i for i, v in enumerate(sampled) if not math.isfinite(v)]}
            if filtering == "nearest" or name.startswith("profile_finite"):
                assert not changed, (name, filtering, changed)
        image_profiles[digest] = {"name": name, "filters": images}
    profiles[optimization] = {"renderer": results["renderer"], "version": results["version"], "glsl": results["glsl"], "probe_rows": 260, "all_uploads_exact_all_gl_errors_zero": True, "finite_control_rows": len(finite_controls), "image_profiles": image_profiles, "rows": rows}

same = all(profiles["O0"]["rows"][name]["output_sha256"] == profiles["O2"]["rows"][name]["output_sha256"] for name in profiles["O0"]["rows"])
summary = {"purpose": "Platform diagnostic evidence only; no port tests accepted or waived", "probe_rows_per_profile": 260, "profiles": ["O0", "O2"], "all_uploads_exact_all_gl_errors_zero": True, "default_upstream_input_sha256_verified": True, "all_actual_coordinates_finite_exact_pixel_centers": True, "o0_o2_all_output_bytes_identical": same, "renderer": profiles["O0"]["renderer"], "version": profiles["O0"]["version"], "representative_outputs": {name: profiles["O0"]["rows"][name]["first_output_bits"][:4] for name in ("neighbor_1_linear", "neighbor_1_nearest", "echo_special1_channel0", "matrix_uniform_special1_channel0", "matrix_literal_special1_channel0", "scalar_uniform_zero_special1_channel0", "scalar_literal_zero_special1_channel0", "matrix_uniform_special1_channel0_glsl450_bits")}, "actual_linear_sampling_changes": profiles["O0"]["rows"]["actual_identity_linear"]["sampled_component_classes"], "actual_nearest_sampling_changes": profiles["O0"]["rows"]["actual_identity_nearest"]["sampled_component_classes"]}
(ROOT / "analysis.json").write_text(json.dumps({"summary": summary, "profiles": profiles}, indent=2) + "\n")
print(json.dumps(summary, indent=2))
