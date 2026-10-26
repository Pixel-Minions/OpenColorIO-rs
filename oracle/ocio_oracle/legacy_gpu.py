# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""O7.1: the pinned wheel's actual legacy GPU processor through a native public-API client.

Original transforms are transported as binary getter values, preserving allocation/file ops
and IEEE coefficients. Before trusting any legacy result, the same client's ordinary GPU
result is compared with gpu_shader, including every field, blob, exception and log byte.
The client is memory-limited to 8 GiB and every invocation has a time limit. A dying boundary
probe is reported, never synthesized. No expected processor result is computed here.
"""

import os
import subprocess

import numpy as np
import PyOpenColorIO as OCIO

from . import gpu, native_build, native_transport, spec
from .checks import PROCESSOR_KEYS, check_keys, check_uint, dump, f64_bits
from .child_env import allowed_environment
from .commands import RAISED, _processor, captured_log, command, exception_result, wheel_raised

MAGIC = 0x374F4349

# Only deliberate OCIO processor controls can differ from the oracle's initially sanitized
# environment (Processor.cpp:354-377, Caching.h:35-110 @ v2.5.2). Both the Python proof and
# native calls must observe them. Unrelated caller variables never reach the helper.
PROCESSOR_ENVIRONMENT = ("OCIO_OPTIMIZATION_FLAGS", "OCIO_DISABLE_ALL_CACHES",
                         "OCIO_DISABLE_PROCESSOR_CACHES", "OCIO_DISABLE_CACHE_FALLBACK")


def _config(w, value):
    if value is None:
        kind, source, off = 0, "", False
    elif "create" in value:
        # Kind 5 extends Create with one unsigned major after the old prefix;
        # kinds 0..4 retain their byte-for-byte layouts for existing requests.
        kind = 5 if "major_version" in value["create"] else 1
        source, off = "", "processor_cache" in value["create"]
    elif "builtin" in value:
        kind, source, off = 3, value["builtin"], False
    elif "yaml" in value:
        kind, source, off = 2, value["yaml"], False
    elif "file" in value:
        kind, source, off = 4, value["file"], False
    else:
        raise ValueError(f"unknown config spec {value!r}")
    w.u(kind); w.text(source); w.u(off)
    if kind == 5:
        w.u(check_uint("a create config's major_version", value["create"]["major_version"]))


def _message(args, blobs, requests):
    w = native_transport.Writer()
    w.u(MAGIC)
    _config(w, args.get("config"))
    w.u("transform" in args)
    if "transform" in args:
        native_transport.write_transform(w, spec.transform(args["transform"], blobs))
    else:
        w.text(args["src"]); w.text(args["dst"])
    w.u(getattr(OCIO, args.get("direction", "TRANSFORM_DIR_FORWARD")))
    w.u(len(requests))
    for request in requests:
        w.u(request["kind"] == "legacy")
        w.u(spec.flags(request.get("optimization")))
        w.u(request.get("edge_len", 32))
    # Send the original setter arguments. Resource-prefix normalization is not idempotent,
    # and applying a Python setter here would move a descriptor error before a legacy error.
    settings = args.get("shader") or {}
    desc = OCIO.GpuShaderDesc.CreateShaderDesc()
    language = OCIO.GpuLanguage.__members__[settings["language"]] if "language" in settings else desc.getLanguage()
    w.u(language)
    for value in (settings.get("function_name", desc.getFunctionName()), settings.get("pixel_name", desc.getPixelName()),
                  settings.get("resource_prefix", desc.getResourcePrefix()), settings.get("uid", desc.getUniqueID())):
        w.text(value)
    descriptor = settings.get("descriptor_set", {"index": desc.getDescriptorSetIndex(), "texture_binding_start": desc.getTextureBindingStart()})
    for value in (descriptor["index"], descriptor["texture_binding_start"], settings.get("texture_max_width", desc.getTextureMaxWidth()),
                  settings.get("allow_texture_1d", desc.getAllowTexture1D())):
        w.u(value)
    return bytes(w.data)


def _property(r):
    kind = OCIO.DynamicPropertyType(r.u())
    if kind in gpu.DOUBLE_PROPERTIES:
        value = {"f64": f64_bits(r.d())}
    else:
        if kind == OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY:
            raw = native_transport.read_primary(r)
        elif kind == OCIO.DYNAMIC_PROPERTY_GRADING_TONE:
            raw = native_transport.read_tone(r)
        elif kind in (OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE, OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE):
            raw = native_transport.read_curves(r, kind == OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE)
        else:
            raise ValueError(f"native helper returned unknown dynamic property {kind}")
        arrays = []
        value = dump(raw, arrays)
        if arrays:
            raise ValueError("native dynamic property unexpectedly contains arrays")
    return {"type": kind.name, "value": value}


def _shader(r, out):
    result = {"cache_id": r.text(), "text": len(out)}
    out.append(r.blob())
    result["language"] = OCIO.GpuLanguage(r.u()).name
    for key in ("function_name", "pixel_name", "resource_prefix", "uid"):
        result[key] = r.text()
    for key in ("descriptor_set_index", "texture_binding_start", "texture_max_width"):
        result[key] = r.u()
    result["allow_texture_1d"] = bool(r.u())
    result["uniform_buffer_size"] = r.u()
    uniforms = []
    for _ in range(r.u()):
        name, kind, offset = r.text(), OCIO.UniformDataType(r.u()), r.u()
        if kind == OCIO.UNIFORM_DOUBLE:
            value = f64_bits(r.d())
        elif kind == OCIO.UNIFORM_BOOL:
            value = bool(r.u())
        elif kind in (OCIO.UNIFORM_FLOAT3, OCIO.UNIFORM_VECTOR_FLOAT):
            values = np.frombuffer(r.blob(), dtype="<f4")
            if kind == OCIO.UNIFORM_FLOAT3:
                # Its Python getter widens float to double before gpu._uniform narrows it.
                values = values.astype("<f8").astype("<f4")
            value = values.view("<u4").tolist()
        elif kind == OCIO.UNIFORM_VECTOR_INT:
            value = np.frombuffer(r.blob(), dtype="<i4").tolist()
        else:
            value = None
        uniforms.append({"name": name, "type": kind.name, "buffer_offset": offset, "value": value})
    result["uniforms"] = uniforms
    textures = []
    for _ in range(r.u()):
        texture = {"name": r.text(), "sampler_name": r.text(), "width": r.u(), "height": r.u(),
                   "channel": OCIO.GpuShaderCreator.TextureType(r.u()).name,
                   "dimensions": OCIO.GpuShaderCreator.TextureDimensions(r.u()).name,
                   "interpolation": OCIO.Interpolation(r.u()).name, "binding_index": r.u(), "values": len(out)}
        out.append(r.blob()); textures.append(texture)
    result["textures"] = textures
    textures = []
    for _ in range(r.u()):
        texture = {"name": r.text(), "sampler_name": r.text(), "edge_len": r.u(),
                   "interpolation": OCIO.Interpolation(r.u()).name, "binding_index": r.u(), "values": len(out)}
        out.append(r.blob()); textures.append(texture)
    result["textures_3d"] = textures
    result["dynamic_properties"] = [_property(r) for _ in range(r.u())]
    return result


def _decode(data):
    r = native_transport.Reader(data)
    if r.u() != MAGIC:
        raise ValueError("the native legacy helper returned the wrong protocol")
    provenance = {"wheel_version": r.text(), "abi": r.text()}
    kind, message, stage = r.text(), r.text(), r.text()
    result = {"log": [r.text() for _ in range(r.u())]}
    processor_id = r.text() if r.u() else None
    if r.u():
        result.update(processor_cache_id=processor_id, gpu_cache_id=r.text(),
                      gpu_processor={"isNoOp": bool(r.u()), "hasChannelCrosstalk": bool(r.u())})
    out = []
    if r.u():
        result["shader"] = _shader(r, out)
    if kind:
        result.update(exception={"type": kind, "message": message}, stage=stage)
    r.done()
    return result, out, provenance


def _native(program, data):
    try:
        process = subprocess.run([str(program)], input=data, capture_output=True,
                                 env=allowed_environment({name: os.environ[name]
                                     for name in PROCESSOR_ENVIRONMENT if name in os.environ}),
                                 timeout=120, check=False)
    except subprocess.TimeoutExpired as error:
        return {"crashed": {"returncode": None, "timeout_seconds": 120,
                            "stderr": {"bytes": (error.stderr or b"").hex()}}}, [], {}
    if process.returncode:
        if process.returncode == 3:
            raise RuntimeError(process.stderr.decode("utf-8", "replace"))
        return {"crashed": {"returncode": process.returncode,
                            "stderr": {"bytes": process.stderr.hex()}}}, [], {}
    return _decode(process.stdout)


def _requests(args):
    before = args.get("before", [])
    if not isinstance(before, list):
        raise ValueError("legacy_gpu_shader before must be a list")
    requests = []
    for item in before:
        check_keys("legacy GPU before", item, {"kind", "optimization", "edge_len"})
        if item.get("kind") not in ("default", "legacy"):
            raise ValueError("legacy GPU before kind must be 'default' or 'legacy'")
        if item["kind"] == "default" and "edge_len" in item:
            raise ValueError("a default GPU before request takes no edge_len")
        if "edge_len" in item:
            check_uint("edge_len", item["edge_len"])
        requests.append(item)
    edge = check_uint("edge_len", args.get("edge_len", 32))
    requests.append({"kind": "legacy", "edge_len": edge, "optimization": args.get("optimization")})
    return requests


@command
def legacy_gpu_shader(args, blobs):
    """The actual wheel's getOptimizedLegacyGPUProcessor, with gpu_shader's result/blob layout.

    Takes gpu_shader's processor, optimization and shader settings, plus edge_len (default32)
    and before (default[]): earlier {kind:'default'|'legacy', optimization, edge_len} requests
    on the SAME processor prime its shared flags-only GPU cache; default requests omit edge_len.
    The final request is legacy. oracle_native holds separate ABI/source/library provenance.

    Every call first proves the native ordinary-GPU result equals gpu_shader completely, then
    returns the legacy result. A mismatch refuses the request. Each child has an 8GiB memory cap
    and a 120-second timeout; a crash/timeout is reported as crashed and cannot enter the cache.
    """
    check_keys("legacy_gpu_shader", args, PROCESSOR_KEYS | {"optimization", "shader", "edge_len", "before"})
    requests = _requests(args)
    gpu._check_settings(args.get("shader") or {})
    ordinary_args = {key: value for key, value in args.items() if key not in {"edge_len", "before"}}
    expected, expected_blobs = gpu.gpu_shader(ordinary_args, blobs)
    # Config/transform/processor failures are the directly observed wheel's, before transport.
    if expected.get("stage") in {"config", "transform", "processor"}:
        return expected, expected_blobs
    stage = ["config"]
    with captured_log():
        try:
            _processor(ordinary_args, stage, blobs)
            ordinary = [{"kind": "default", "optimization": args.get("optimization")}]
            ordinary_data = _message(ordinary_args, blobs, ordinary)
        except RAISED as error:
            if not wheel_raised(error):
                raise
            # A setting can fail only after the GPU processor exists. gpu_shader reports that.
            if "exception" in expected:
                return expected, expected_blobs
            raise ValueError(f"native legacy transport could not copy the processor: {exception_result(error)}") from error
    program, provenance = native_build.executable()
    actual, actual_blobs, abi = _native(program, ordinary_data)
    if actual != expected or actual_blobs != expected_blobs:
        raise ValueError(f"native ordinary GPU transport does not match gpu_shader: "
                         f"wheel {expected!r}, native {actual!r}; blob equality {actual_blobs == expected_blobs}")
    result, out, actual_abi = _native(program, _message(ordinary_args, blobs, requests))
    provenance.update(actual_abi or abi)
    result["oracle_native"] = provenance
    return result, out
