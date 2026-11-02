# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""P87: the wheel's live legacy GPU uniforms/properties after extraction and setters.

Only the wheel's public getters/setters produce observations. Original transforms
carry update values losslessly through the already verified native transport. The
initial state must equal legacy_gpu_shader completely before later states are used.
"""

import os
import subprocess

import PyOpenColorIO as OCIO

from . import gpu, legacy_gpu, native_build, native_transport, spec
from .checks import PROCESSOR_KEYS, check_keys, check_member
from .child_env import allowed_environment
from .commands import command

MAGIC = 0x38504349


def _updates(args, blobs):
    updates = args.get("updates")
    if not isinstance(updates, list):
        raise ValueError("legacy_gpu_updates updates must be a list")
    out = []
    for index, update in enumerate(updates):
        check_keys(f"updates[{index}]", update, {"from", "property", "value_transform"})
        if set(update) != {"from", "property", "value_transform"}:
            raise ValueError(f"updates[{index}] needs from, property and value_transform")
        if update.get("from") not in ("processor", "shader"):
            raise ValueError(f"updates[{index}].from must be processor or shader")
        check_member(f"updates[{index}].property", update.get("property"), OCIO.DynamicPropertyType)
        kind = OCIO.DynamicPropertyType.__members__[update["property"]]
        expected = {
            OCIO.DYNAMIC_PROPERTY_EXPOSURE: OCIO.ExposureContrastTransform,
            OCIO.DYNAMIC_PROPERTY_CONTRAST: OCIO.ExposureContrastTransform,
            OCIO.DYNAMIC_PROPERTY_GAMMA: OCIO.ExposureContrastTransform,
            OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY: OCIO.GradingPrimaryTransform,
            OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE: OCIO.GradingRGBCurveTransform,
            OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE: OCIO.GradingHueCurveTransform,
            OCIO.DYNAMIC_PROPERTY_GRADING_TONE: OCIO.GradingToneTransform,
        }[kind]
        value = spec.transform(update["value_transform"], blobs)
        if not isinstance(value, expected):
            raise ValueError(f"updates[{index}].value_transform must be {expected.__name__}")
        out.append((update["from"], kind, value))
    return out


def _message(args, blobs, requests):
    original = {k: v for k, v in args.items() if k != "updates"}
    data = legacy_gpu._message(original, blobs, requests)
    writer = native_transport.Writer()
    writer.u(MAGIC)
    updates = _updates(args, blobs)
    writer.u(len(updates))
    for source, kind, value in updates:
        writer.u(source == "shader"); writer.u(kind)
        native_transport.write_transform(writer, value)
    return data + bytes(writer.data)


def _decode(data):
    reader = native_transport.Reader(data)
    if reader.u() != MAGIC:
        # Construction can fail before the optional extension is reached. Those
        # are the original helper's observed failure, never invented states.
        return legacy_gpu._decode(data)
    provenance = {"wheel_version": reader.text(), "abi": reader.text()}
    kind, message, stage = reader.text(), reader.text(), reader.text()
    result = {"log": [reader.text() for _ in range(reader.u())], "states": []}
    blobs = []
    for _ in range(reader.u()):
        state = native_transport.Reader(reader.blob())
        observed = {"processor_cache_id": state.text(), "gpu_cache_id": state.text(),
                    "gpu_processor": {"isNoOp": bool(state.u()), "hasChannelCrosstalk": bool(state.u())}}
        observed["shader"] = legacy_gpu._shader(state, blobs)
        observed["processor_properties"] = [legacy_gpu._property(state) for _ in range(state.u())]
        observed["log"] = [state.text() for _ in range(state.u())]
        state.done()
        result["states"].append(observed)
    if kind:
        result.update(exception={"type": kind, "message": message}, stage=stage)
    reader.done()
    return result, blobs, provenance


def _native(program, data):
    try:
        process = subprocess.run([str(program)], input=data, capture_output=True,
            env=allowed_environment({name: os.environ[name]
                for name in legacy_gpu.PROCESSOR_ENVIRONMENT if name in os.environ}),
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


@command
def legacy_gpu_updates(args, blobs):
    """legacy_gpu_shader inputs plus updates=[{from:processor|shader, property,
    value_transform}]. Extract exactly ONCE, then copy each matching public donor
    transform getter into the requested dynamic property setter. Read the ORIGINAL
    descriptor's uniforms, properties, all shader/texture bytes and processor/cache
    queries before updates and after every successful setter. Source processor
    properties are read too, to expose sharing and isolation. A setter exception
    retains the completed prefix and its actual failure; no new extraction occurs.
    """
    check_keys("legacy_gpu_updates", args, PROCESSOR_KEYS | {"optimization", "shader", "edge_len", "before", "updates"})
    _updates(args, blobs)
    original = {key: value for key, value in args.items() if key != "updates"}
    initial, initial_blobs = legacy_gpu.legacy_gpu_shader(original, blobs)
    if "exception" in initial or "crashed" in initial:
        return initial, initial_blobs
    program, provenance = native_build.executable()
    result, out, abi = _native(program, _message(args, blobs, legacy_gpu._requests(original)))
    if "crashed" not in result:
        if not result.get("states"):
            raise ValueError(f"native dynamic helper returned no initial state: {result!r}")
        first = {key: value for key, value in result["states"][0].items() if key != "processor_properties"}
        expected = {key: value for key, value in initial.items() if key != "oracle_native"}
        if first != expected or out[:len(initial_blobs)] != initial_blobs:
            raise ValueError("native dynamic initial state does not match legacy_gpu_shader")
        if "exception" in result:
            if result.get("stage") != "update" or not 1 <= len(result["states"]) <= len(args["updates"]):
                raise ValueError("native dynamic helper returned an impossible failed-update prefix")
        elif len(result["states"]) != len(args["updates"]) + 1:
            raise ValueError("native dynamic helper dropped or duplicated update states")
    provenance.update(abi)
    result["oracle_native"] = provenance
    return result, out
