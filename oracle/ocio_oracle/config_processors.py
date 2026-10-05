# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for the processors a config makes (card p3-oracle, chunk O3.5).

- config_processor: a processor from a config through any of the 13 Config.getProcessor
  overloads of the binding, with or without a context, in an environment and a directory of
  files the request gives; then what processor_ops, cpu_apply, image_apply and gpu_shader
  report of a processor, each as those commands report it.

It reuses those commands' helpers by import (processor_ops.py, image.py, gpu.py) and
config_calls' engine (config_api.py), so no shared file changes, and each part means what it
means there.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import copy
import functools

import numpy as np
import PyOpenColorIO as OCIO

from . import gpu, image, processor_ops, spec
from .checks import check_keys, check_member
from .commands import DTYPES, command
from .config_api import (REPORTED, LogCapture, RequestError, directory, environment,
                         make_config, raised, run_calls, text, value_out)


def _failed(exc):
    """A part's failure: {"undecodable": hex} or {"exception": ...}, as config_api reports."""
    if isinstance(exc, UnicodeDecodeError):
        return {"undecodable": bytes(exc.object).hex()}
    return {"exception": raised(exc)}


def _helper(fn, *args):
    """fn(*args), a helper of another oracle module. Its ValueError is that module's refusal
    (checks.dump's depth, gpu_shader's texture widths and grading values), so the request's:
    RequestError. What the library raises inside it passes through."""
    try:
        return fn(*args)
    except (OCIO.Exception, OCIO.ExceptionMissingFile, UnicodeDecodeError):
        raise
    except ValueError as exc:
        raise RequestError(f"{getattr(fn, '__name__', fn)}: {exc}") from exc

# Overload name -> (its arguments' kinds, whether a context variant exists). The kinds:
# "name" (text), "color_space" (a name, looked up with getColorSpace: a name it doesn't find
# passes None), "named_transform" (likewise with getNamedTransform), "transform" (a transform
# spec), "direction" (a TRANSFORM_DIR_* name). The keyword names are the binding's
# (PyConfig.cpp), so each call reaches exactly the overload it names.
OVERLOADS = {
    "color_spaces": (("srcColorSpace", "color_space"), ("dstColorSpace", "color_space")),
    "names": (("srcColorSpaceName", "name"), ("dstColorSpaceName", "name")),
    "display_view": (("srcColorSpaceName", "name"), ("display", "name"), ("view", "name"),
                     ("direction", "direction")),
    "named_transform": (("namedTransform", "named_transform"), ("direction", "direction")),
    "named_transform_name": (("namedTransformName", "name"), ("direction", "direction")),
    "transform": (("transform", "transform"),),
    "transform_direction": (("transform", "transform"), ("direction", "direction")),
}
# The overloads without a context variant in the binding.
NO_CONTEXT = {"transform"}


def _arguments(config, overload, values):
    """The keyword arguments of the overload, from the request's values."""
    kinds = OVERLOADS[overload]
    if not isinstance(values, list) or len(values) != len(kinds):
        raise RequestError(f"overload {overload} takes {len(kinds)} values: "
                           f"{[k for k, _ in kinds]}")
    kwargs = {}
    for (keyword, kind), value in zip(kinds, values):
        if kind == "name":
            kwargs[keyword] = text(keyword, value)
        elif kind == "color_space":
            kwargs[keyword] = config.getColorSpace(text(keyword, value))
        elif kind == "named_transform":
            kwargs[keyword] = config.getNamedTransform(text(keyword, value))
        elif kind == "transform":
            kwargs[keyword] = spec.transform(value)
        else:
            kwargs[keyword] = check_member(keyword, value, OCIO.TransformDirection)
    return kwargs


def _check_overload(args):
    overload = args.get("overload")
    if not isinstance(overload, dict) or len(overload) != 1 or next(iter(overload)) not in OVERLOADS:
        raise RequestError(f"overload must be one of {sorted(OVERLOADS)}: {{name: [values]}}, "
                           f"not {overload!r}")
    (name, values), = overload.items()
    if name in NO_CONTEXT and "context" in args:
        raise RequestError(f"the binding has no context variant of overload {name}")
    return name, values


def _context(config, spec_, objects, log):
    """The context a request gives: {"base": "current" (a copy of the config's current
    context, the default) or "new" (Context()), "calls": calls on it (config_api.run_calls,
    on "context")}. Returns the calls' results."""
    check_keys("context", spec_, {"base", "calls"})
    base = spec_.get("base", "current")
    if base == "current":
        objects["context"] = copy.deepcopy(config.getCurrentContext())
    elif base == "new":
        objects["context"] = OCIO.Context()
    else:
        raise RequestError(f"context base {base!r}: current or new")
    return run_calls(spec_.get("calls", []), objects, "context", log)


def _bit_depth(key, args):
    return check_member(key, args.get(key, "BIT_DEPTH_F32"), OCIO.BitDepth)


def _ops(proc, args, out):
    """processor_ops' result for the processor."""
    check_keys("ops", args, {"in_bitdepth", "out_bitdepth", "optimization"})
    key = (_bit_depth("in_bitdepth", args), _bit_depth("out_bitdepth", args),
           spec.flags(args.get("optimization")))
    stage, result = ["group"], {}
    try:
        result["processor"] = _helper(processor_ops._processor_dump, proc, out)
        stage[0] = "optimize"
        optimized = proc.getOptimizedProcessor(*key)
        stage[0] = "optimized_group"
        result["optimized"] = _helper(processor_ops._processor_dump, optimized, out)
    except REPORTED as exc:
        result = {**_failed(exc), "stage": stage[0]}
    return result


def _cpu_processor(proc, args):
    """The CPU processor cpu_apply and image_apply choose, and its (in, out, flags)."""
    in_bd, out_bd = _bit_depth("in_bitdepth", args), _bit_depth("out_bitdepth", args)
    if "optimization" in args or in_bd != OCIO.BIT_DEPTH_F32 or out_bd != OCIO.BIT_DEPTH_F32:
        key = (in_bd, out_bd, spec.flags(args.get("optimization")))
        return proc.getOptimizedCPUProcessor(*key), key
    key = (OCIO.BIT_DEPTH_F32, OCIO.BIT_DEPTH_F32, OCIO.OPTIMIZATION_DEFAULT)
    return proc.getDefaultCPUProcessor(), key


def _cpu(proc, args, blobs, out):
    """cpu_apply's result for the processor: packed pixels from a request blob."""
    check_keys("cpu", args, {"in_bitdepth", "out_bitdepth", "optimization", "channels", "blob"})
    in_bd, out_bd = _bit_depth("in_bitdepth", args), _bit_depth("out_bitdepth", args)
    spec.flags(args.get("optimization"))
    channels = args.get("channels", 4)
    if isinstance(channels, bool) or channels not in (3, 4):
        raise RequestError(f"channels {channels!r}: 3 or 4")
    index = args.get("blob")
    if isinstance(index, bool) or not isinstance(index, int) or not 0 <= index < len(blobs):
        raise RequestError(f"blob {index!r}: the index of one of the {len(blobs)} request blobs")
    src = np.frombuffer(blobs[index], dtype=DTYPES[in_bd.name]).copy()
    npix = src.size // channels
    if npix * channels != src.size:
        raise RequestError(f"the blob holds {src.size} channels, not whole pixels")
    stage, result = ["cpu_processor"], {}
    try:
        cpu, _ = _cpu_processor(proc, args)
        stage[0] = "apply"
        dst = np.zeros(npix * channels, dtype=DTYPES[out_bd.name])
        src_desc = OCIO.PackedImageDesc(src, npix, 1, channels, in_bd, src.itemsize,
                                        src.itemsize * channels, src.itemsize * channels * npix)
        dst_desc = OCIO.PackedImageDesc(dst, npix, 1, channels, out_bd, dst.itemsize,
                                        dst.itemsize * channels, dst.itemsize * channels * npix)
        cpu.apply(src_desc, dst_desc)
        result = {"cpu_cache_id": cpu.getCacheID(), "pixels": len(out)}
        out.append(dst.tobytes())
    except REPORTED as exc:
        result = {**_failed(exc), "stage": stage[0]}
    return result


IMAGE_KEYS = {"buffers", "images", "apply", "data_getters", "in_bitdepth", "out_bitdepth",
              "optimization"}


def _image(proc, args, blobs, out):
    """image_apply's result for the processor (image.image_apply, with this processor): the
    same checks before apply, the same refusals, the same result; every buffer, then the data
    getters' copies, appended to the response blobs, at the indices "buffers" and the getters
    give."""
    check_keys("image", args, IMAGE_KEYS)
    _bit_depth("in_bitdepth", args), _bit_depth("out_bitdepth", args)
    spec.flags(args.get("optimization"))
    buffers = image._buffers(args.get("buffers") or [], blobs)
    images = args.get("images") or []
    makers = [image._prepare(spec_, buffers) for spec_ in images]
    apply = [int(i) for i in args.get("apply") or []]
    if len(apply) > 2 or any(i < 0 or i >= len(images) for i in apply):
        raise RequestError(f"apply {apply} with {len(images)} images")
    stage, result, copies, descs = ["cpu_processor"], {"images": []}, [], []
    constructing = False
    first = len(out)
    try:
        if apply:
            cpu, key = _cpu_processor(proc, args)
            result.update(image._processor_result(proc, cpu))
        stage[0] = "image"
        for make in makers:
            constructing = True
            desc = make()
            constructing = False
            descs.append(desc)
            result["images"].append(image._getters(desc))
        if apply:
            for i in sorted(set(apply)):
                image._check_inside(i, descs[i], images[i], buffers)
            image._check_sizes(descs, apply)
            if image._reads_source(descs, apply, cpu):
                looks_up = functools.cache(lambda: image._starts_with_forward_lut1d(proc, key))
                image._check_codes(apply[0], descs[apply[0]], images[apply[0]], buffers,
                                   looks_up)
                image._check_overlap(descs, images, apply, looks_up)
            stage[0] = "apply"
            cpu.apply(*[descs[i] for i in apply])
        if args.get("data_getters"):
            for desc, spec_, getters in zip(descs, images, result["images"]):
                getters.update(image._data_getters(desc, spec_, buffers, first + len(buffers),
                                                   copies))
    except (*image.RAISED, TypeError) as exc:
        if isinstance(exc, TypeError) and not constructing:
            raise
        result.update(**_failed(exc), stage=stage[0])
        if stage[0] == "image":
            result["image"] = len(descs)
    result["buffers"] = list(range(first, first + len(buffers)))
    out.extend(memory.tobytes() for memory in buffers)
    out.extend(copies)
    return result


def _gpu(proc, args, log, kept, out):
    """gpu_shader's result for the processor. The probe extraction gpu_shader makes to check the
    texture widths logs what the real one logs again: its lines are dropped from `log` (what
    was logged before it is moved to `kept`)."""
    check_keys("gpu", args, {"optimization", "shader"})
    settings = args.get("shader") or {}
    gpu._check_settings(settings)
    stage, result, shader_blobs = ["gpu_processor"], {}, []
    try:
        if "optimization" in args:
            processor = proc.getOptimizedGPUProcessor(spec.flags(args["optimization"]))
        else:
            processor = proc.getDefaultGPUProcessor()
        result.update({
            "gpu_cache_id": processor.getCacheID(),
            "gpu_processor": {"isNoOp": processor.isNoOp(),
                              "hasChannelCrosstalk": processor.hasChannelCrosstalk()},
        })
        stage[0] = "shader_desc"
        desc = gpu._shader_desc(settings)
        kept.extend(log.take())
        _helper(gpu._check_textures, processor, settings, desc.getTextureMaxWidth(), [])
        log.discard()
        stage[0] = "extract"
        processor.extractGpuShaderInfo(desc)
        result["shader"] = _helper(gpu._shader, desc, shader_blobs)
    except REPORTED as exc:
        return {**result, **_failed(exc), "stage": stage[0]}
    # gpu._shader numbers its blobs from 0: move them after the response's earlier blobs.
    base = len(out)
    shader = result["shader"]
    shader["text"] += base
    for texture in shader["textures"] + shader["textures_3d"]:
        texture["values"] += base
    out.extend(shader_blobs)
    return result


@command
def config_processor(args, blobs):
    """A processor from a config by any getProcessor overload, and what the processor commands
    report of it.

    args:
      config    the config (a config_calls source; see config_api.make_config)
      env, files  the request's environment and directory, as in config_calls
      context   optional: the context to pass, {"base": "current" (a copy of the config's
                current context, the default) or "new" (Context()), "calls": calls on it, as
                config_calls runs them on "context" (the config is "config")}. With it, the
                overload's context variant is called
      overload  {name: [values]}, the binding's overloads (PyConfig.cpp):
                  "color_spaces": [src, dst]        getProcessor([context,] srcColorSpace,
                                                    dstColorSpace), the color spaces found by
                                                    getColorSpace (None where it finds none)
                  "names": [src, dst]               getProcessor([context,] srcColorSpaceName,
                                                    dstColorSpaceName)
                  "display_view": [src, display, view, direction]
                  "named_transform": [name, direction]   the named transform found by
                                                    getNamedTransform (None where none)
                  "named_transform_name": [name, direction]
                  "transform": [spec]               getProcessor(transform): no context variant
                  "transform_direction": [spec, direction]
                names are text or {"bytes": hex}, transforms are transform specs (spec.py),
                directions TRANSFORM_DIR_* names
      ops       optional, processor_ops' settings: {"in_bitdepth", "out_bitdepth",
                "optimization"}
      cpu       optional, a list of cpu_apply's settings: {"in_bitdepth", "out_bitdepth",
                "optimization", "channels", "blob": index of the request blob of input pixels}
      image     optional, image_apply's settings: {"buffers", "images", "apply",
                "data_getters", "in_bitdepth", "out_bitdepth", "optimization"}, buffers'
                "blob" indices into the request blobs
      gpu       optional, gpu_shader's settings: {"optimization", "shader"}
    result:
      dir           the request's directory
      config        null, or {"exception"} (nothing else runs then)
      config_log    what OCIO logged while making the config: [{"bytes": hex}] (config_api)
      context       the context calls' results, each with its log (with a context; when one
                    raised, nothing else runs)
      processor     {"cache_id": {"bytes": hex}}, or {"exception"} or {"undecodable"} when
                    getProcessor failed (no part runs then)
      ops           as processor_ops: {"processor", "optimized"} or {"exception", "stage"}
      cpu           per entry, as cpu_apply: {"cpu_cache_id", "pixels": blob index} or
                    {"exception", "stage"}
      image         as image_apply: {"images", "cpu_processor", ..., "buffers": the blob
                    indices of the buffers after the call}, the data getters' blob indices
                    absolute
      gpu           as gpu_shader: {"gpu_cache_id", "gpu_processor", "shader"} with absolute
                    blob indices, or {"exception", "stage"}
      config_context_cache_id   the config's current context's cache ID after the parts
                    ({"bytes": hex}): the context passed is a copy, which leaves it alone
      log           what OCIO logged from getProcessor on: [{"bytes": hex}]
    blobs: the parts' blobs, in the order ops, cpu, image, gpu

    The parts refuse what their commands refuse (image_apply's reads outside a buffer,
    gpu_shader's texture widths, ...), and an unknown key, overload or value is refused.
    """
    check_keys("config_processor", args, {"config", "env", "files", "context", "overload",
                                          "ops", "cpu", "image", "gpu"})
    overload, values = _check_overload(args)
    cpu_parts = args.get("cpu", [])
    if not isinstance(cpu_parts, list):
        raise RequestError("cpu must be a list")
    if "context" in args:
        check_keys("context", args["context"], {"base", "calls"})
    result, out = {}, []
    with directory(args.get("files")) as root, environment(args.get("env")), \
            LogCapture() as log:
        result["dir"] = root
        objects = {}
        try:
            objects["config"] = make_config(args.get("config", "raw"))
            result["config"] = None
        except REPORTED as exc:
            result["config"] = _failed(exc)
        result["config_log"] = log.take()
        if "config" not in objects:
            return result, out
        config = objects["config"]
        if "context" in args:
            result["context"] = _context(config, args["context"], objects, log)
            if any("result" not in c for c in result["context"]):
                return result, out
        kwargs = _arguments(config, overload, values)
        kept = []
        try:
            if "context" in args:
                proc = config.getProcessor(context=objects["context"], **kwargs)
            else:
                proc = config.getProcessor(**kwargs)
            result["processor"] = {"cache_id": value_out(proc.getCacheID())}
        except REPORTED as exc:
            result["processor"] = _failed(exc)
            proc = None
        if proc is not None:
            if "ops" in args:
                result["ops"] = _ops(proc, args["ops"], out)
            if cpu_parts:
                result["cpu"] = [_cpu(proc, part, blobs, out) for part in cpu_parts]
            if "image" in args:
                result["image"] = _image(proc, args["image"], blobs, out)
            if "gpu" in args:
                result["gpu"] = _gpu(proc, args["gpu"], log, kept, out)
        result["config_context_cache_id"] = value_out(config.getCurrentContext().getCacheID())
        result["log"] = kept + log.take()
    return result, out
