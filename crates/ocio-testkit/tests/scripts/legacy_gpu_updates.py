# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Direct-wheel contracts for P87's native post-extraction observations.

This compares only two clients of the official library. No Rust SDK supplies an
expected value, and no processor math is reimplemented. All numbers below are
authored setter inputs, including the nondefault grading inputs in the existing
native_ordinary_gpu_matches_the_python_wheel contract.
"""
import json

import PyOpenColorIO as OCIO

from ocio_oracle import commands, gpu, legacy_gpu, legacy_gpu_updates as updates, native_build, spec


def obj(name, args=None, attrs=None, calls=None):
    return {"object": {"class": name, "args": args or [], "attrs": attrs or [], "calls": calls or []}}


def input_cases():
    ec = {"class": "ExposureContrastTransform", "calls": [
        ["makeExposureDynamic"], ["makeContrastDynamic"], ["makeGammaDynamic"]]}
    donor = {"class": "ExposureContrastTransform", "calls": [
        ["setExposure", 1.25], ["setContrast", 0.75], ["setGamma", 1.125]]}
    cases = [(kind, ec, donor) for kind in ["DYNAMIC_PROPERTY_EXPOSURE", "DYNAMIC_PROPERTY_CONTRAST", "DYNAMIC_PROPERTY_GAMMA"]]
    curve = obj("GradingBSplineCurve", [[-0.5, -0.4, 0.0, 0.1, 1.0, 0.9]], calls=[["setSlopes", [0.75, 1.0, 0.5]]])
    hue_curve = obj("GradingBSplineCurve", [[0.0, 0.8, 0.5, 1.2, 1.0, 1.0], {"enum": "LUM_SAT"}], calls=[["setSlopes", [0.75, 1.0, 0.5]]])
    for kind, name, style, value in [
        ("DYNAMIC_PROPERTY_GRADING_PRIMARY", "GradingPrimaryTransform", "GRADING_LIN",
         obj("GradingPrimary", [{"enum": "GRADING_LIN"}], [["saturation", 0.75]])),
        ("DYNAMIC_PROPERTY_GRADING_TONE", "GradingToneTransform", "GRADING_LOG",
         obj("GradingTone", [{"enum": "GRADING_LOG"}], [["scontrast", 1.125]])),
        ("DYNAMIC_PROPERTY_GRADING_RGBCURVE", "GradingRGBCurveTransform", "GRADING_LOG",
         obj("GradingRGBCurve", [{"enum": "GRADING_LOG"}], [["red", curve]])),
        ("DYNAMIC_PROPERTY_GRADING_HUECURVE", "GradingHueCurveTransform", "GRADING_LOG",
         obj("GradingHueCurve", [{"enum": "GRADING_LOG"}], [["lum_sat", hue_curve]])),
    ]:
        cases.append((kind,
            {"class": name, "args": {"style": {"enum": style}}, "calls": [["makeDynamic"]]},
            {"class": name, "args": {"style": {"enum": style}}, "calls": [["setValue", value]]}))
    return cases


def snapshot(proc, gpu_proc, desc, log, out):
    return {"processor_cache_id": proc.getCacheID(), "gpu_cache_id": gpu_proc.getCacheID(),
            "gpu_processor": {"isNoOp": gpu_proc.isNoOp(), "hasChannelCrosstalk": gpu_proc.hasChannelCrosstalk()},
            "shader": gpu._shader(desc, out),
            "processor_properties": [gpu._dynamic_property(proc.getDynamicProperty(kind)) for kind in [
                OCIO.DYNAMIC_PROPERTY_EXPOSURE, OCIO.DYNAMIC_PROPERTY_CONTRAST, OCIO.DYNAMIC_PROPERTY_GAMMA,
                OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY, OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE,
                OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE, OCIO.DYNAMIC_PROPERTY_GRADING_TONE]
                if proc.hasDynamicProperty(kind)], "log": list(log)}


def direct_ordinary(args):
    """The Python wheel's public getters and typed setters on ONE descriptor."""
    states, out, stage = [], [], ["config"]
    getters = {"DYNAMIC_PROPERTY_EXPOSURE": "getExposure", "DYNAMIC_PROPERTY_CONTRAST": "getContrast", "DYNAMIC_PROPERTY_GAMMA": "getGamma"}
    with commands.captured_log() as log:
        try:
            _, proc = commands._processor(args, stage, [])
            stage[0] = "gpu_processor"
            gpu_proc = proc.getOptimizedGPUProcessor(spec.flags(args.get("optimization")))
            stage[0] = "shader_desc"
            desc = gpu._shader_desc(args.get("shader") or {})
            stage[0] = "extract"
            gpu_proc.extractGpuShaderInfo(desc)
            states.append(snapshot(proc, gpu_proc, desc, log, out))
            for update in args["updates"]:
                stage[0] = "update"
                kind = OCIO.DynamicPropertyType.__members__[update["property"]]
                source = desc if update["from"] == "shader" else proc
                prop = source.getDynamicProperty(kind)
                donor = spec.transform(update["value_transform"], [])
                if kind in gpu.DOUBLE_PROPERTIES:
                    prop.setDouble(getattr(donor, getters[kind.name])())
                else:
                    getattr(prop, "set" + gpu.GRADING_GETTERS[kind][3:])(donor.getValue())
                states.append(snapshot(proc, gpu_proc, desc, log, out))
        except commands.RAISED as error:
            if not commands.wheel_raised(error):
                raise
            return {"log": list(log), "states": states, "exception": commands.exception_result(error), "stage": stage[0]}, out
    return {"log": list(log), "states": states}, out


def prove_ordinary(program, args):
    expected, expected_blobs = direct_ordinary(args)
    native, blobs, abi = updates._native(program, updates._message(args, [], [{"kind": "default", "optimization": args.get("optimization")}]))
    assert native == expected and blobs == expected_blobs, (args, native, expected, blobs == expected_blobs)
    assert abi["wheel_version"] == "2.5.2", abi
    return expected


def main():
    program, provenance = native_build.executable()
    cases = input_cases()
    compared, successful, refused = 0, 0, 0
    for language in OCIO.GpuLanguage.__members__:
        for kind, transform, donor in cases:
            for source in ["shader", "processor"]:
                args = {"transform": transform, "optimization": "OPTIMIZATION_NONE", "shader": {"language": language},
                        "updates": [{"from": source, "property": kind, "value_transform": donor},
                                    {"from": source, "property": kind, "value_transform": transform}]}
                result = prove_ordinary(program, args)
                compared += 1
                if "exception" in result:
                    refused += 1
                else:
                    assert len(result["states"]) == len(args["updates"]) + 1
                    successful += 1
    # A missing property must keep its completed initial state and actual setter failure.
    missing = {"transform": {"class": "MatrixTransform"}, "updates": [
        {"from": "shader", "property": "DYNAMIC_PROPERTY_EXPOSURE", "value_transform": cases[0][2]}]}
    result = prove_ordinary(program, missing)
    assert result["stage"] == "update" and len(result["states"]) == 1
    # Exercise the public legacy command and its independent initial-state proof.
    args = {"transform": cases[0][1], "edge_len": 2, "optimization": "OPTIMIZATION_NONE", "updates": [
        {"from": "shader", "property": cases[0][0], "value_transform": cases[0][2]}]}
    observed, blobs = updates.legacy_gpu_updates(args, [])
    assert "exception" not in observed and len(observed["states"]) == 2, observed
    assert observed["states"][0]["shader"]["dynamic_properties"] != observed["states"][1]["shader"]["dynamic_properties"]
    assert observed["oracle_native"]["source_key"] == provenance["source_key"]
    print(json.dumps({"ordinary_comparisons": compared, "languages": len(OCIO.GpuLanguage.__members__),
                      "property_types": [c[0] for c in cases], "successful": successful, "exact_refusals": refused,
                      "missing_property_prefix": True, "legacy_live_update": True, "source_key": provenance["source_key"]}))


if __name__ == "__main__":
    main()
