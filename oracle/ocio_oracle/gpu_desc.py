# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for GPU shader descriptions themselves (Phase 1, card p1-gpu-infra, 1.7c).

- gpu_shader_desc: a GpuShaderDesc made by CreateShaderDesc() and driven through the calls
  Python has, in order; what each call returns or raises, then everything the description
  holds, as gpu_shader reports it.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import numpy as np
import PyOpenColorIO as OCIO

from .checks import UNSIGNED_MAX, check_bool, check_member, check_uint
from .commands import captured_log, command, exception_result, wheel_raised
from .gpu import C_SPACE, RAISED, SAMPLER_READ, _shader

# The largest C `int`: the texture iterators take their index as one.
INT_MAX = 2 ** 31 - 1

# The calls a request can make, with the kinds of their arguments:
#   str: a string; uint: an integer a C `unsigned` holds; index: an integer a C `int` holds,
#   from 0; bool: true or false; language, channel, dimensions, interpolation, property: a
#   member's name of GpuLanguage, TextureType, TextureDimensions, Interpolation or
#   DynamicPropertyType; values: the index of a request blob of little-endian float32.
CALLS = {
    "setUniqueID": ["str"],
    "setLanguage": ["language"],
    "setFunctionName": ["str"],
    "setPixelName": ["str"],
    "setResourcePrefix": ["str"],
    "setDescriptorSetIndex": ["uint", "uint"],
    "setTextureMaxWidth": ["uint"],
    "setAllowTexture1D": ["bool"],
    "getNextResourceIndex": [],
    "getCacheID": [],
    "begin": ["str"],
    "end": [],
    "addToParameterDeclareShaderCode": ["str"],
    "addToTextureDeclareShaderCode": ["str"],
    "addToHelperShaderCode": ["str"],
    "addToFunctionHeaderShaderCode": ["str"],
    "addToFunctionShaderCode": ["str"],
    "addToFunctionFooterShaderCode": ["str"],
    "createShaderText": ["str"] * 6,
    "finalize": [],
    "getShaderText": [],
    "addTexture": ["str", "str", "uint", "uint", "channel", "dimensions", "interpolation",
                   "values"],
    "add3DTexture": ["str", "str", "uint", "interpolation", "values"],
    "getTexture": ["index"],
    "get3DTexture": ["index"],
    "getNumDynamicProperties": [],
    "getDynamicProperty": ["property"],
    "getDynamicPropertyAt": ["index"],
    "clone": [],
}

ENUMS = {
    "language": OCIO.GpuLanguage,
    "channel": OCIO.GpuShaderDesc.TextureType,
    "dimensions": OCIO.GpuShaderDesc.TextureDimensions,
    "interpolation": OCIO.Interpolation,
    "property": OCIO.DynamicPropertyType,
}

# The languages whose class wrapper replaces the declarations with its header, which the
# command doesn't follow (see _Declarations).
WRAPPED = {OCIO.GPU_LANGUAGE_MSL_2_0, OCIO.LANGUAGE_OSL_1}


def _c_string(s):
    """The bytes a `const char *` argument gives the library: UTF-8, up to the first NUL."""
    return s.encode("utf-8").split(b"\0", 1)[0]


def _reads_past_a_line(declarations):
    """Whether the Metal class wrapper reads past a line of `declarations` (bytes), parsing them
    as extractFunctionParameters does (GpuShaderClassWrapper.cpp:285-354 @ v2.5.2): after a line
    that starts with "texture" past white space (so not a comment), it takes the next line for
    the texture's sampler, and reads it from find("sampler") + 7. That is 6 when the line has no
    "sampler", which a line shorter than 6 bytes can't hold, so it reads past exactly the lines
    shorter than 6 bytes. std::getline gives each line without its line feed, and once the
    stream is at its end, leaves the line as it was."""
    pos, eof = 0, False

    def getline(line):
        nonlocal pos, eof
        if eof:
            return line
        end = declarations.find(b"\n", pos)
        if end < 0:
            line, pos, eof = declarations[pos:], len(declarations), True
        else:
            line, pos = declarations[pos:end], end + 1
        return line

    line = b""
    while not eof:
        line = getline(line)
        stripped = line.lstrip(C_SPACE.encode())
        if not stripped.startswith(b"texture"):
            continue
        line = getline(line)
        if len(line) < SAMPLER_READ:
            return True
    return False


class _Declarations:
    """The description's declarations as the library keeps them (GpuShaderDesc.cpp:284-300 @
    v2.5.2), to refuse a finalize that would make the Metal class wrapper read past a line.
    After a finalize in MSL or OSL, whose wrappers replace them, they are no longer known, and
    the command refuses any later finalize in MSL."""

    def __init__(self):
        self.parameters, self.textures, self.known = b"", b"", True

    def copy(self):
        other = _Declarations()
        other.parameters, other.textures, other.known = self.parameters, self.textures, self.known
        return other

    def add_parameters(self, code):
        if not self.parameters:
            self.parameters += b"\n// Declaration of all variables\n\n"
        self.parameters += _c_string(code)

    def add_textures(self, code):
        if not self.textures:
            self.textures += b"\n// Declaration of all textures\n\n"
        self.textures += _c_string(code)

    def check_finalize(self, language):
        if language != OCIO.GPU_LANGUAGE_MSL_2_0:
            return
        if not self.known:
            raise ValueError(
                "gpu_shader_desc refuses a finalize in MSL after a finalize in MSL or OSL: it "
                "doesn't follow the declarations the class wrapper made, which the Metal class "
                "wrapper would read back")
        if _reads_past_a_line(self.parameters + b"\n" + self.textures):
            raise ValueError(
                "gpu_shader_desc refuses this finalize in MSL: a declaration that starts with "
                "'texture' is followed by a line shorter than 6 bytes, which the wheel's Metal "
                "class wrapper reads past")

    def finalized(self, language):
        if language in WRAPPED:
            self.known = False


def _argument(name, i, kind, value, blobs):
    """The call's argument `value`, checked against its kind, as the binding takes it."""
    what = f"{name} argument {i}"
    if kind == "str":
        if not isinstance(value, str):
            raise ValueError(f"{what} must be a string, not {value!r}")
        return value
    if kind == "uint":
        return check_uint(what, value)
    if kind == "index":
        return check_uint(what, value, INT_MAX)
    if kind == "bool":
        return check_bool(what, value)
    if kind == "values":
        check_uint(what, value, len(blobs) - 1 if blobs else -1)
        return np.frombuffer(blobs[value], dtype="<f4")
    return check_member(what, value, ENUMS[kind])


def _check_texture_size(name, args):
    """Refuses texture values that don't hold the count the binding checks
    (PyGpuShaderDesc.cpp:116-202 @ v2.5.2), which it would refuse itself, and a 1D LUT's
    texture whose float count would wrap in a C `unsigned`. The wheel then stores fewer values
    than the binding reads back (CreateArray, GpuShader.cpp:24-37, and getValues,
    PyGpuShaderDesc.cpp:259-283). A 3D texture's count can't wrap below 130 texels a side, and
    the wheel refuses 130 before it copies anything; the binding's own count wraps, as it
    does in the wheel."""
    if name == "addTexture":
        _, _, width, height, channel, _, _, values = args
        channels = 3 if channel == OCIO.GpuShaderDesc.TEXTURE_RGB_CHANNEL else 1
        count = width * height * channels
        if count > UNSIGNED_MAX:
            raise ValueError(f"gpu_shader_desc refuses {name} of {count} floats: the wheel "
                             f"counts them in a C unsigned, which wraps")
    else:
        _, _, edgelen, _, values = args
        count = (edgelen * edgelen * edgelen * 3) % (UNSIGNED_MAX + 1)
    if len(values) != count:
        raise ValueError(f"{name} needs {count} floats, not {len(values)}")


def _texture(texture, out):
    """A texture from an iterator, as gpu_shader reports it. Its values go to `out`."""
    result = {
        "name": texture.textureName,
        "sampler_name": texture.samplerName,
        "interpolation": texture.interpolation.name,
        "binding_index": texture.textureShaderBindingIndex,
        "values": len(out),
    }
    if hasattr(texture, "edgeLen"):
        result["edge_len"] = texture.edgeLen
    else:
        result.update(width=texture.width, height=texture.height,
                      channel=texture.channel.name, dimensions=texture.dimensions.name)
    out.append(np.asarray(texture.getValues(), dtype="<f4").tobytes())
    return result


def _check_calls(args, blobs):
    """The request's calls, each its name and its arguments as the binding takes them."""
    calls = args.get("calls")
    if set(args) - {"calls", "debug_log"} or not isinstance(calls, list):
        raise ValueError(f"gpu_shader_desc takes {{'calls': [...], 'debug_log': bool}}, "
                         f"not {args!r}")
    checked = []
    for call in calls:
        if not isinstance(call, list) or not call or call[0] not in CALLS:
            raise ValueError(f"gpu_shader_desc: unknown call {call!r}")
        name, kinds = call[0], CALLS[call[0]]
        if len(call) - 1 != len(kinds):
            raise ValueError(f"{name} takes {len(kinds)} arguments, not {len(call) - 1}")
        values = [_argument(name, i, kind, v, blobs)
                  for i, (kind, v) in enumerate(zip(kinds, call[1:]))]
        if name in ("addTexture", "add3DTexture"):
            _check_texture_size(name, values)
        checked.append((name, values))
    return checked


def _call(desc, name, values, out):
    """Makes one call; returns the description after it, and what the call returned."""
    if name == "clone":
        return desc.clone(), None
    if name == "getTexture":
        return desc, _texture(desc.getTextures()[values[0]], out)
    if name == "get3DTexture":
        return desc, _texture(desc.get3DTextures()[values[0]], out)
    if name == "getNumDynamicProperties":
        return desc, len(desc.getDynamicProperties())
    if name == "getDynamicPropertyAt":
        return desc, {"type": desc.getDynamicProperties()[values[0]].getType().name}
    if name == "getDynamicProperty":
        return desc, {"type": desc.getDynamicProperty(values[0]).getType().name}
    returned = getattr(desc, name)(*values)
    return desc, returned


def _run(checked, out):
    """Makes the calls on a new description; returns their results and the description."""
    desc = OCIO.GpuShaderDesc.CreateShaderDesc()
    declarations = _Declarations()
    results = []
    for name, values in checked:
        if name == "finalize":
            declarations.check_finalize(desc.getLanguage())
        try:
            desc, returned = _call(desc, name, values, out)
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            results.append({"exception": exception_result(exc)})
            continue
        if name == "clone":
            declarations = declarations.copy()
        elif name == "addToParameterDeclareShaderCode":
            declarations.add_parameters(values[0])
        elif name == "addToTextureDeclareShaderCode":
            declarations.add_textures(values[0])
        elif name == "finalize":
            declarations.finalized(desc.getLanguage())
        results.append({"returned": returned})
    return results, desc


@command
def gpu_shader_desc(args, blobs):
    """Drives a GpuShaderDesc through the calls Python has, and reports what it holds.

    args:
      calls     in order, each [name, arguments...] (see CALLS for the names and the kinds of
                their arguments). "getTexture", "get3DTexture" and "getDynamicPropertyAt" index
                the description's iterators, "getNumDynamicProperties" is the length of its
                dynamic properties' one, and "clone" replaces the description with its clone().
      debug_log optional: true makes the calls at LOGGING_LEVEL_DEBUG, at which finalize logs
                the shader, then restores the level
    blobs: the texture values the calls name, little-endian float32.

    Before a call, the command refuses the request (it raises, so the call fails) where the
    wheel would do something undefined: a finalize in MSL whose declarations would make the
    Metal class wrapper read past a line (_reads_past_a_line), or one after a finalize in MSL
    or OSL, whose declarations it doesn't follow; and a 1D LUT's texture whose float count
    would wrap in a C unsigned (_check_texture_size). An unknown call, an argument of the wrong
    kind, or texture values of the wrong count is refused too.

    result:
      calls     per call, in order: {"returned": what it returns (null for nothing; a texture
                as gpu_shader reports it, its values a response blob; a dynamic property as
                {"type": its DynamicPropertyType name})}, or {"exception": {"type",
                "message"}}. A call that raises doesn't stop the next ones.
      shader    the description after the calls, as gpu_shader reports it
      log       OCIO's log messages
    blobs: the values of the textures the calls return, in order, then the shader text and the
    values of the description's textures
    """
    checked = _check_calls(args, blobs)
    debug_log = check_bool("debug_log", args.get("debug_log", False))
    out = []
    level = OCIO.GetLoggingLevel()
    with captured_log() as log:
        if debug_log:
            OCIO.SetLoggingLevel(OCIO.LOGGING_LEVEL_DEBUG)
        try:
            results, desc = _run(checked, out)
            shader = _shader(desc, out)
        finally:
            OCIO.SetLoggingLevel(level)
    return {"calls": results, "shader": shader, "log": log}, out
