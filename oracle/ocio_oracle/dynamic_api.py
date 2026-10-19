# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for dynamic properties (Phase 5, chunk O5.1).

- dynamic_apply: a processor, its CPU processor and optionally a GPU shader description, then
  steps in order on their dynamic properties: take one, set its value, read it, apply the CPU
  processor to images, read the description's uniforms. The way to compare a dynamic op's
  pixels and uniforms after each set.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import functools

import PyOpenColorIO as OCIO

from . import gpu, image, spec
from .checks import check_keys, check_member, dump, f64_bits
from .commands import RAISED, captured_log, command, exception_result, wheel_raised

# Where a "get" step takes a property from.
SOURCES = ("processor", "cpu", "gpu")

# The setters and getters of a DynamicProperty; a step names one, or the property's type
# chooses it.
SETTERS = {"setDouble", "setGradingPrimary", "setGradingRGBCurve", "setGradingHueCurve",
           "setGradingTone"}
GETTERS = {"getDouble", "getGradingPrimary", "getGradingRGBCurve", "getGradingHueCurve",
           "getGradingTone"}

# The step kinds and the keys each takes.
STEP_KEYS = {
    "get": {"get", "from", "as"},
    "set": {"set", "call", "value"},
    "read": {"read", "call"},
    "apply": {"apply"},
    "uniforms": {"uniforms"},
}


def _check_step(index, step, images, has_gpu, names):
    """Refuses a step of an unknown kind, with a key its kind doesn't take, naming a property
    no earlier "get" step names, a source, setter, getter or type that doesn't exist, images
    that don't exist, or uniforms without a GPU description. `names` collects the names the
    "get" steps bind."""
    kinds = [kind for kind in STEP_KEYS if isinstance(step, dict) and kind in step]
    if len(kinds) != 1:
        raise ValueError(f"steps[{index}] must have exactly one of {sorted(STEP_KEYS)}: {step!r}")
    kind = kinds[0]
    check_keys(f"steps[{index}]", step, STEP_KEYS[kind])
    if kind == "get":
        check_member(f"steps[{index}].get", step["get"], OCIO.DynamicPropertyType)
        if step.get("from") not in SOURCES:
            raise ValueError(f"steps[{index}].from must be one of {SOURCES}: {step!r}")
        if step["from"] == "gpu" and not has_gpu:
            raise ValueError(f"steps[{index}] takes a property from the GPU description, but "
                             f"the request has no gpu")
        if not isinstance(step.get("as"), str):
            raise ValueError(f"steps[{index}].as must name the property: {step!r}")
        names.add(step["as"])
    elif kind in ("set", "read"):
        if step[kind] not in names:
            raise ValueError(f"steps[{index}] names {step[kind]!r}, which no earlier get step "
                             f"binds")
        allowed = SETTERS if kind == "set" else GETTERS
        if "call" in step and step["call"] not in allowed:
            raise ValueError(f"steps[{index}].call must be one of {sorted(allowed)}: {step!r}")
        if kind == "set" and "value" not in step:
            raise ValueError(f"steps[{index}] sets no value: {step!r}")
    elif kind == "apply":
        apply = step["apply"]
        if (not isinstance(apply, list) or not 1 <= len(apply) <= 2
                or any(isinstance(i, bool) or not isinstance(i, int) or not 0 <= i < len(images)
                       for i in apply)):
            raise ValueError(f"steps[{index}].apply must be [i] or [i, j] of the {len(images)} "
                             f"images: {step!r}")
    elif step["uniforms"] is not True or not has_gpu:
        raise ValueError(f"steps[{index}]: uniforms is true, with a gpu in the request: "
                         f"{step!r}")


def _property_value(prop, call=None):
    """The property's type and value as gpu.py writes them; with `call`, that getter's value
    instead of the one of the property's type."""
    if call is None:
        return gpu._dynamic_property(prop)
    result = getattr(prop, call)()
    if call == "getDouble":
        return {"type": prop.getType().name, "value": {"f64": f64_bits(result)}}
    arrays = []
    value = dump(result, arrays)
    if arrays:
        raise ValueError(f"dynamic_apply can't report {call}: its value holds arrays")
    return {"type": prop.getType().name, "value": value}


def _default_setter(prop):
    """The setter of the property's type."""
    kind = prop.getType()
    if kind in gpu.DOUBLE_PROPERTIES:
        return "setDouble"
    return "s" + gpu.GRADING_GETTERS[kind][1:]


def _apply(cpu, proc, key, apply, descs, images, buffers):
    """cpu.apply on the images, after image_apply's checks, which refuse the request where the
    wheel would read or write outside its memory (image.py)."""
    for i in sorted(set(apply)):
        image._check_inside(i, descs[i], images[i], buffers)
    image._check_sizes(descs, apply)
    if image._reads_source(descs, apply, cpu):
        looks_up = functools.cache(lambda: image._starts_with_forward_lut1d(proc, key))
        image._check_codes(apply[0], descs[apply[0]], images[apply[0]], buffers, looks_up)
        image._check_overlap(descs, images, apply, looks_up)
    cpu.apply(*[descs[i] for i in apply])


def _run_step(step, holders, props, descs, images, buffers, out, entry):
    """One step's result. What it learns before it can raise goes in `entry`: a get step's
    "has", and a set step's "stage" ("value" while it builds the value, "set" while the setter
    runs)."""
    proc, cpu, key, desc = holders
    if "get" in step:
        source = {"processor": proc, "cpu": cpu, "gpu": desc}[step["from"]]
        kind = OCIO.DynamicPropertyType.__members__[step["get"]]
        props.pop(step["as"], None)
        entry["has"] = source.hasDynamicProperty(kind)
        prop = source.getDynamicProperty(kind)
        props[step["as"]] = prop
        return _property_value(prop)
    if "set" in step:
        prop = props.get(step["set"])
        if prop is None:
            return {"unbound": True}
        entry["stage"] = "value"
        value = spec.value(step["value"])
        entry["stage"] = "set"
        getattr(prop, step.get("call") or _default_setter(prop))(value)
        return None
    if "read" in step:
        prop = props.get(step["read"])
        if prop is None:
            return {"unbound": True}
        return _property_value(prop, step.get("call"))
    if "apply" in step:
        _apply(cpu, proc, key, step["apply"], descs, images, buffers)
        first = len(out)
        out.extend(memory.tobytes() for memory in buffers)
        return {"buffers": list(range(first, len(out)))}
    return [gpu._uniform(name, data) for name, data in desc.getUniforms()]


@command
def dynamic_apply(args, blobs):
    """A processor's dynamic properties: taken, set and read, between CPU applies and reads of a
    GPU shader description's uniforms, in the order of the request's steps.

    args:
      config, transform or src/dst, direction, optimization, in_bitdepth, out_bitdepth
                the processor and its CPU processor, as cpu_apply takes them
      gpu       optional, {"optimization": flags (absent: getDefaultGPUProcessor()), "shader":
                settings as gpu_shader takes them}: the GPU processor, and the GpuShaderDesc
                its extractGpuShaderInfo fills, with gpu_shader's checks
      buffers, images
                the memory and the image descriptions, as image_apply takes them; the images
                are constructed once, after the processors
      steps     in order, each one of
                  {"get": DYNAMIC_PROPERTY_* name, "from": "processor", "cpu" or "gpu", "as":
                   a name}       hasDynamicProperty(type), then getDynamicProperty(type) of the
                                 processor, the CPU processor or the GPU description; the
                                 property is bound to the name for the later steps
                  {"set": name, "value": a value spec (spec.py: {"f64": bits} for a double,
                   {"object": spec} for a grading value), "call": optional setter}
                                 the property's setter: the one of its type (setDouble,
                                 setGradingPrimary, setGradingRGBCurve, setGradingHueCurve or
                                 setGradingTone), or the one named
                  {"read": name, "call": optional getter}
                                 its type and value: the getter of its type, or the one named
                  {"apply": [i] or [i, j]}
                                 cpu.apply(images[i]) in place, or cpu.apply(images[i],
                                 images[j]), with image_apply's checks
                  {"uniforms": true}
                                 the GPU description's uniforms, as gpu_shader reads them: a
                                 dynamic property's uniform reads its value now
    request blobs: the ones the buffers name, up to the last one named; then the transform
                spec's blobs (see spec.py)

    The request is refused (the call fails) for an unknown key or step, a step naming a
    property no earlier "get" step binds, an unknown source, type, setter or getter, images
    that don't exist, uniforms without a gpu, and where image_apply or gpu_shader refuse it.

    result:
      processor_cache_id, cpu_cache_id, cpu_processor
                as image_apply gives them
      is_dynamic  {"processor": isDynamic(), "cpu": isDynamic()}
      gpu_cache_id, shader
                with a gpu: as gpu_shader gives them, the description just after the
                extraction (its text and textures among the response blobs)
      images    per constructed image, its getters, as image_apply gives them
      steps     per step, in order, {"result": ..., "log": OCIO's log messages during the
                step} or {"exception": {"type", "message"}, "log": ...} when OCIO or the
                binding raised; the results:
                  get       {"type", "value"}: the value as gpu_shader writes a dynamic
                            property's (a double as {"f64": bits}, a grading value by
                            checks.dump); the entry's "has" is hasDynamicProperty's answer,
                            also when getDynamicProperty raised
                  set       null; when it raised, the entry's "stage" says where: "value"
                            (building the value: an object spec's constructor or setters) or
                            "set" (the property's setter)
                  read      {"type", "value"}
                  apply     {"buffers": the indices among the response blobs of every buffer,
                            in order, after the apply}; when the apply raised, the entry's
                            "buffers" are those indices, of the buffers as it left them
                  uniforms  the uniforms, in order, as gpu_shader writes them
                A step on a property whose "get" step raised gives {"result": {"unbound": true}}.
      exception, stage   when OCIO or the binding raised before the steps: "config",
                "transform", "processor", "cpu_processor", "gpu_processor", "shader_desc",
                "extract" or "image" (with "image": its index); no step runs then
      log       OCIO's log messages before the steps
    blobs: with a gpu, the shader's text and textures; then each apply step's buffers
    """
    image._check_keys("dynamic_apply", args,
                      image.PROCESSOR_KEYS | {"gpu", "buffers", "images", "steps"})
    gpu_args = args.get("gpu")
    if gpu_args is not None:
        check_keys("gpu", gpu_args, {"optimization", "shader"})
        settings = gpu_args.get("shader") or {}
        gpu._check_settings(settings)
    buffers = image._buffers(args.get("buffers") or [], blobs)
    images = args.get("images") or []
    makers = [image._prepare(spec_, buffers) for spec_ in images]
    steps = args.get("steps") or []
    if not isinstance(steps, list):
        raise ValueError(f"steps must be a list, not {steps!r}")
    names = set()
    for index, step in enumerate(steps):
        _check_step(index, step, images, gpu_args is not None, names)

    stage, result, out, descs = ["config"], {}, [], []
    constructing = False
    with captured_log() as log:
        try:
            proc, cpu, key = image._cpu_processor(args, stage, blobs[image._own_blobs(args):])
            result.update(image._processor_result(proc, cpu))
            result["is_dynamic"] = {"processor": proc.isDynamic(), "cpu": cpu.isDynamic()}
            desc = None
            if gpu_args is not None:
                stage[0] = "gpu_processor"
                if "optimization" in gpu_args:
                    gpu_proc = proc.getOptimizedGPUProcessor(spec.flags(gpu_args["optimization"]))
                else:
                    gpu_proc = proc.getDefaultGPUProcessor()
                result["gpu_cache_id"] = gpu_proc.getCacheID()
                stage[0] = "shader_desc"
                desc = gpu._shader_desc(settings)
                gpu._check_textures(gpu_proc, settings, desc.getTextureMaxWidth(), log)
                stage[0] = "extract"
                gpu_proc.extractGpuShaderInfo(desc)
                result["shader"] = gpu._shader(desc, out)
            stage[0] = "image"
            result["images"] = []
            for make in makers:
                constructing = True
                made = make()
                constructing = False
                descs.append(made)
                result["images"].append(image._getters(made))
        except (*RAISED, TypeError) as exc:
            # A TypeError is the binding's only while constructing an image, as in image_apply.
            if isinstance(exc, TypeError) and not constructing:
                raise
            if not isinstance(exc, TypeError) and not wheel_raised(exc):
                raise
            result.update(exception=exception_result(exc), stage=stage[0])
            if stage[0] == "image":
                result["image"] = len(descs)
            result["log"] = list(log)
            return result, out
        result["log"] = list(log)

        holders, props, result["steps"] = (proc, cpu, key, desc), {}, []
        for step in steps:
            logged, entry = len(log), {}
            try:
                entry["result"] = _run_step(step, holders, props, descs, images, buffers, out,
                                            entry)
                entry.pop("stage", None)
            except RAISED as exc:
                if not wheel_raised(exc):
                    raise
                entry["exception"] = exception_result(exc)
                if "apply" in step:
                    # The buffers as the failed apply left them, as image_apply returns them.
                    first = len(out)
                    out.extend(memory.tobytes() for memory in buffers)
                    entry["buffers"] = list(range(first, len(out)))
            entry["log"] = log[logged:]
            result["steps"].append(entry)
    return result, out
