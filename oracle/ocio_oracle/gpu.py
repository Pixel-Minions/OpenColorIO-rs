# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for GPU shaders (Phase 1, chunk O1.3).

- gpu_shader: GPUProcessor.extractGpuShaderInfo into a GpuShaderDesc set up with any of its
  settings, and everything the description then holds: the shader text, the uniforms with their
  values (dynamic properties' included), the 1D, 2D and 3D textures with their values, the
  dynamic properties and the cache ID.

Like every oracle command, it reports what the library does and never computes expected
values.

What it reports differs between the Windows and Linux wheels (D12) in two places, over 1230
shaders compared in all 10 languages: a negative NaN prints "-nan(ind)" on Windows and "-nan"
on Linux (the platform's iostream), and the ACES 2 output transform's tables, which libm
computes, hold different values.
"""

import re

import numpy as np
import PyOpenColorIO as OCIO

from . import spec
from .checks import PROCESSOR_KEYS, check_keys, f32_bits, f64_bits
from .commands import _processor, captured_log, command, exception_result

# What OCIO raises (in PyOpenColorIO, ExceptionMissingFile doesn't derive from OCIO.Exception).
RAISED = (OCIO.Exception, OCIO.ExceptionMissingFile)

# The GpuShaderDesc settings, in the order the command applies them.
SETTINGS = ["language", "function_name", "pixel_name", "resource_prefix", "uid",
            "descriptor_set", "texture_max_width", "allow_texture_1d"]

# The widest texture a probe extraction allows: the largest unsigned, so a 1D LUT's texture is
# one row of its whole length (ops/lut1d/Lut1DOpGPU.cpp:153-157 @ v2.5.2).
UNLIMITED_WIDTH = 2 ** 32 - 1

# The textures of the ACES 2 output transform's tables, which the wheel doesn't pad: it refuses
# a table wider than the limit instead ("1D LUT size exceeds the maximum",
# ops/fixedfunction/FixedFunctionOpGPU.cpp:511-515, 814-818 and GpuShader.cpp:212-219).
UNPADDED = re.compile(r"(reach_m|gamut_cusp)_table_\d+$")


def _check_name(what, value):
    """Refuses a name holding a character outside printable ASCII. The wheel passes the names'
    characters to std::isalpha, std::isalnum, std::isdigit and std::isspace
    (GPUProcessor.cpp:180-188, GpuShaderClassWrapper.cpp:157, 226, 307-348 @ v2.5.2), which are
    undefined for the negative `char` values of non-ASCII bytes. The MSL class wrapper also reads
    the shader's declarations back a line at a time, and past the end of a line that a line
    break in a name cuts short (GpuShaderClassWrapper.cpp:332-333)."""
    if not isinstance(value, str) or any(not " " <= c <= "~" for c in value):
        raise ValueError(f"gpu_shader refuses {what} {value!r}: only printable ASCII names are "
                         f"defined in the wheel")
    return value


def _shader_desc(settings, max_width=None):
    """A GpuShaderDesc with the default settings of CreateShaderDesc(), then each setting the
    request gives, through its setter. With max_width, the texture width limit is that instead."""
    desc = OCIO.GpuShaderDesc.CreateShaderDesc()
    if "language" in settings:
        desc.setLanguage(getattr(OCIO.GpuLanguage, settings["language"]))
    if "function_name" in settings:
        desc.setFunctionName(_check_name("function_name", settings["function_name"]))
    if "pixel_name" in settings:
        desc.setPixelName(_check_name("pixel_name", settings["pixel_name"]))
    if "resource_prefix" in settings:
        desc.setResourcePrefix(_check_name("resource_prefix", settings["resource_prefix"]))
    if "uid" in settings:
        desc.setUniqueID(_check_name("uid", settings["uid"]))
    if "descriptor_set" in settings:
        descriptor_set = settings["descriptor_set"]
        check_keys("descriptor_set", descriptor_set, {"index", "texture_binding_start"})
        desc.setDescriptorSetIndex(int(descriptor_set["index"]),
                                   int(descriptor_set["texture_binding_start"]))
    width = settings.get("texture_max_width") if max_width is None else max_width
    if width is not None:
        desc.setTextureMaxWidth(int(width))
    if "allow_texture_1d" in settings:
        desc.setAllowTexture1D(bool(settings["allow_texture_1d"]))
    return desc


def _padding_fits(length, max_width):
    """Whether the wheel can lay out a 1D LUT of `length` entries in a texture at most
    `max_width` texels wide. It makes the texture min(length, max_width) wide and
    length / max_width + 1 high, and in more than one row repeats each row's last entry at the
    start of the next (ops/lut1d/Lut1DOpGPU.cpp:19-141, 153-177 @ v2.5.2):
    - a width of 0 divides by zero;
    - a width of 1 never advances along the LUT, and repeats entries forever;
    - otherwise the padded entries can outnumber the texture's texels (a length of 8191 does at
      the default width of 4096). The count of texels left to fill, an unsigned difference,
      then wraps, and the wheel appends entries until memory runs out."""
    if max_width == 0:
        return False
    width = min(length, max_width)
    height = length // max_width + 1
    if height == 1:
        return True
    step = width - 1
    if step == 0:
        return False
    rows = -(-(length - step) // step)
    padded = rows * width + (length - rows * step)
    return padded <= width * height


def _check_textures(gpu, settings, max_width):
    """Refuses the request where the wheel's 1D LUT textures wouldn't fit the width limit
    `max_width` (see _padding_fits). A probe extraction with the request's settings but no width
    limit gives each texture's length: a 1D LUT then lays out in one row. If the probe raises,
    the textures registered before it are still checked: the real extraction stops there too,
    or sooner. Every 1D or 2D texture is taken for a 1D LUT's, but the ACES 2 tables
    (UNPADDED)."""
    probe = _shader_desc(settings, UNLIMITED_WIDTH)
    try:
        gpu.extractGpuShaderInfo(probe)
    except RAISED:
        pass
    for texture in probe.getTextures():
        if UNPADDED.search(texture.textureName):
            continue
        if not _padding_fits(texture.width, max_width):
            raise ValueError(
                f"gpu_shader refuses a texture width limit of {max_width}: the wheel can't lay "
                f"out the {texture.width} entries of {texture.textureName} in it without "
                f"dividing by zero, looping forever or running out of memory")


def _uniform(name, data):
    """A uniform's name, type, buffer offset and value, read with the getter of its type."""
    kind = data.type
    if kind == OCIO.UNIFORM_DOUBLE:
        value = f64_bits(data.getDouble())
    elif kind == OCIO.UNIFORM_BOOL:
        value = bool(data.getBool())
    elif kind == OCIO.UNIFORM_FLOAT3:
        value = f32_bits(data.getFloat3())
    elif kind == OCIO.UNIFORM_VECTOR_FLOAT:
        value = f32_bits(data.getVectorFloat())
    elif kind == OCIO.UNIFORM_VECTOR_INT:
        value = np.asarray(data.getVectorInt(), dtype=np.int32).tolist()
    else:
        value = None
    return {"name": name, "type": kind.name, "buffer_offset": int(data.bufferOffset),
            "value": value}


# The dynamic property types whose value is a double; the others hold grading values.
DOUBLE_PROPERTIES = {OCIO.DYNAMIC_PROPERTY_EXPOSURE, OCIO.DYNAMIC_PROPERTY_CONTRAST,
                     OCIO.DYNAMIC_PROPERTY_GAMMA}

GRADING_GETTERS = {
    OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY: "getGradingPrimary",
    OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE: "getGradingRGBCurve",
    OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE: "getGradingHueCurve",
    OCIO.DYNAMIC_PROPERTY_GRADING_TONE: "getGradingTone",
}


def _dynamic_property(prop):
    kind = prop.getType()
    if kind in DOUBLE_PROPERTIES:
        return {"type": kind.name, "double": f64_bits(prop.getDouble())}
    return {"type": kind.name, "repr": repr(getattr(prop, GRADING_GETTERS[kind])())}


def _shader(desc, blobs):
    """Everything the description holds after the extraction. Texture values go to `blobs`."""
    result = {
        "cache_id": desc.getCacheID(),
        "text": len(blobs),
        "language": desc.getLanguage().name,
        "function_name": desc.getFunctionName(),
        "pixel_name": desc.getPixelName(),
        "resource_prefix": desc.getResourcePrefix(),
        "uid": desc.getUniqueID(),
        "descriptor_set_index": desc.getDescriptorSetIndex(),
        "texture_binding_start": desc.getTextureBindingStart(),
        "texture_max_width": desc.getTextureMaxWidth(),
        "allow_texture_1d": desc.getAllowTexture1D(),
        "uniform_buffer_size": desc.getUniformBufferSize(),
        "uniforms": [_uniform(name, data) for name, data in desc.getUniforms()],
        "textures": [],
        "textures_3d": [],
        "dynamic_properties": [_dynamic_property(p) for p in desc.getDynamicProperties()],
    }
    blobs.append(desc.getShaderText().encode("utf-8"))
    for texture in desc.getTextures():
        result["textures"].append({
            "name": texture.textureName,
            "sampler_name": texture.samplerName,
            "width": texture.width,
            "height": texture.height,
            "channel": texture.channel.name,
            "dimensions": texture.dimensions.name,
            "interpolation": texture.interpolation.name,
            "binding_index": texture.textureShaderBindingIndex,
            "values": len(blobs),
        })
        blobs.append(np.asarray(texture.getValues(), dtype="<f4").tobytes())
    for texture in desc.get3DTextures():
        result["textures_3d"].append({
            "name": texture.textureName,
            "sampler_name": texture.samplerName,
            "edge_len": texture.edgeLen,
            "interpolation": texture.interpolation.name,
            "binding_index": texture.textureShaderBindingIndex,
            "values": len(blobs),
        })
        blobs.append(np.asarray(texture.getValues(), dtype="<f4").tobytes())
    return result


@command
def gpu_shader(args, blobs):
    """Extracts a GPU processor's shader into a GpuShaderDesc and reports what it holds.

    args:
      config, transform or src/dst, direction
                the processor, as cpu_apply takes them
      optimization
                flags (see spec.flags): getOptimizedGPUProcessor(flags); absent, the default GPU
                processor (getDefaultGPUProcessor)
      shader    the description's settings, each set with its setter on CreateShaderDesc()'s
                defaults, in this order (all optional):
                  language          GpuLanguage name, e.g. "GPU_LANGUAGE_GLSL_4_0",
                                    "GPU_LANGUAGE_HLSL_DX11" (HLSL SM 5.0), "LANGUAGE_OSL_1"
                  function_name, pixel_name, resource_prefix, uid
                                    strings of printable ASCII (see _check_name)
                  descriptor_set    {"index": n, "texture_binding_start": n}
                                    (setDescriptorSetIndex)
                  texture_max_width setTextureMaxWidth
                  allow_texture_1d  setAllowTexture1D

    Before extracting, the command refuses the request (it raises, so the call fails) where
    the wheel would do something undefined: a name outside printable ASCII (_check_name), or a
    1D LUT that wouldn't fit its texture width limit, which divides by zero, loops forever or
    exhausts memory (_padding_fits; the default limit of 4096 can't hold a LUT of 8191
    entries). An unknown key is refused too.

    result:
      processor_cache_id, gpu_cache_id, gpu_processor (isNoOp, hasChannelCrosstalk)
      shader    after the extraction:
                  cache_id          getCacheID()
                  text              the index of the shader text among the response blobs (UTF-8)
                  language, function_name, pixel_name, resource_prefix, uid,
                  descriptor_set_index, texture_binding_start, texture_max_width,
                  allow_texture_1d, uniform_buffer_size
                                    the getters (enums by name)
                  uniforms          in order, {"name", "type" (UniformDataType name),
                                    "buffer_offset", "value"}: a double's bits (UNIFORM_DOUBLE),
                                    a bool, the bits of 3 floats (UNIFORM_FLOAT3: the binding
                                    passes them as Python floats, which quiets a signalling NaN),
                                    the bits of the floats (UNIFORM_VECTOR_FLOAT), the ints
                                    (UNIFORM_VECTOR_INT), or null (UNIFORM_UNKNOWN); dynamic
                                    properties' as they are at extraction
                  textures          1D and 2D, in order: {"name", "sampler_name", "width",
                                    "height", "channel", "dimensions", "interpolation",
                                    "binding_index", "values" (a blob index: little-endian
                                    float32, width * height * channels)}
                  textures_3d       in order: {"name", "sampler_name", "edge_len",
                                    "interpolation", "binding_index", "values" (a blob index:
                                    edge_len^3 * 3 float32)}
                  dynamic_properties  in order: {"type", "double": bits} for exposure, contrast
                                    and gamma, {"type", "repr": repr() of the grading value}
                                    for the grading properties
      exception, stage   when OCIO raised: {"type", "message"}, and where: "config",
                "transform", "processor" (as in cpu_apply), "gpu_processor", "shader_desc"
                (the setters) or "extract"
      log       OCIO's log messages
    blobs: [the shader text, then each texture's values: 1D and 2D, then 3D], when nothing raised
    """
    check_keys("gpu_shader", args, PROCESSOR_KEYS | {"optimization", "shader"})
    settings = args.get("shader") or {}
    check_keys("shader", settings, set(SETTINGS))
    if "language" in settings and not hasattr(OCIO.GpuLanguage, str(settings["language"])):
        raise ValueError(f"unknown GpuLanguage {settings['language']!r}")
    for key in ("function_name", "pixel_name", "resource_prefix", "uid"):
        if key in settings:
            _check_name(key, settings[key])
    stage, result, out = ["config"], {}, []
    with captured_log() as log:
        try:
            _, proc = _processor(args, stage)
            stage[0] = "gpu_processor"
            if "optimization" in args:
                gpu = proc.getOptimizedGPUProcessor(spec.flags(args["optimization"]))
            else:
                gpu = proc.getDefaultGPUProcessor()
            result.update({
                "processor_cache_id": proc.getCacheID(),
                "gpu_cache_id": gpu.getCacheID(),
                "gpu_processor": {"isNoOp": gpu.isNoOp(),
                                  "hasChannelCrosstalk": gpu.hasChannelCrosstalk()},
            })
            stage[0] = "shader_desc"
            desc = _shader_desc(settings)
            _check_textures(gpu, settings, desc.getTextureMaxWidth())
            stage[0] = "extract"
            gpu.extractGpuShaderInfo(desc)
            result["shader"] = _shader(desc, out)
        except RAISED as exc:
            result.update(exception=exception_result(exc), stage=stage[0])
            out = []
    result["log"] = log
    return result, out
