# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle commands for image descriptions and CPUProcessor.apply (Phase 1, chunk O1.2).

- image_apply: images described with PackedImageDesc and PlanarImageDesc in every form the
  Python binding offers, over buffers the request supplies; CPUProcessor.apply on them; every
  buffer returned in full, padding included.
- image_apply_rgb: CPUProcessor.applyRGB and applyRGBA on an array or a list.

Like every oracle command, these report what the library and its Python binding do, and
never compute expected values. Two things they leave out:
- a description's repr(), which prints the address of the binding's shared_ptr rather than
  the description (PyImageDesc.cpp:20-25), so it changes from run to run;
- the C++ CPUProcessor::applyRGB(float *) and applyRGBA(float *), which Python can't reach:
  its applyRGB and applyRGBA wrap the values in a PackedImageDesc and call apply
  (PyCPUProcessor.cpp:94-249).
"""

import functools

import numpy as np
import PyOpenColorIO as OCIO

from . import spec
from .commands import DTYPES, _processor, captured_log, command, exception_result

# Buffers start on this boundary, so an image's alignment depends only on the request.
ALIGNMENT = 64

# Bytes of one channel, per bit depth the binding accepts (the storage types of DTYPES).
CHANNEL_BYTES = {name: np.dtype(dtype).itemsize for name, dtype in DTYPES.items()}

# What OCIO and the binding raise: OCIO's exceptions (in PyOpenColorIO, ExceptionMissingFile
# doesn't derive from OCIO.Exception) and the binding's buffer checks (std::runtime_error).
RAISED = (OCIO.Exception, OCIO.ExceptionMissingFile, RuntimeError)


def _check_keys(what, spec, allowed):
    """Refuses a spec with a key the command doesn't know, so that a misspelled key can't pass
    unnoticed (a misspelled "fill" would otherwise fill with zeros)."""
    if not isinstance(spec, dict):
        raise ValueError(f"{what} must be an object, not {spec!r}")
    unknown = sorted(set(spec) - set(allowed))
    if unknown:
        raise ValueError(f"{what}: unknown keys {unknown}; it takes {sorted(allowed)}")


# The keys of the processor, as cpu_apply takes them.
PROCESSOR_KEYS = {"config", "transform", "src", "dst", "direction", "optimization",
                  "in_bitdepth", "out_bitdepth"}


def _buffers(specs, blobs):
    """Fresh writable uint8 arrays, each starting on an ALIGNMENT boundary."""
    out = []
    for index, buffer in enumerate(specs):
        _check_keys(f"buffers[{index}]", buffer,
                    {"blob"} if isinstance(buffer, dict) and "blob" in buffer else {"size", "fill"})
        if "blob" in buffer:
            data = np.frombuffer(blobs[buffer["blob"]], dtype=np.uint8)
        elif "size" in buffer:
            size = int(buffer["size"])
            pattern = bytes(buffer.get("fill", [0]))
            if size < 0 or not pattern:
                raise ValueError(f"buffers[{index}]: a size of {size} bytes, a fill of {pattern!r}")
            data = np.frombuffer((pattern * -(-size // len(pattern)))[:size], dtype=np.uint8)
        else:
            raise ValueError(f"buffers[{index}] needs 'blob' or 'size': {buffer!r}")
        raw = np.empty(data.size + ALIGNMENT, dtype=np.uint8)
        start = -raw.ctypes.data % ALIGNMENT
        memory = raw[start:start + data.size]
        memory[...] = data
        out.append(memory)
    return out


def _enum(cls, value):
    """An OCIO enum value from its name, or from any integer (the binding accepts values
    outside the enum; pybind11 doesn't check them)."""
    if isinstance(value, str):
        return getattr(OCIO, value)
    return cls(int(value))


def _stride(value):
    return OCIO.AutoStride if value in (None, "AutoStride") else int(value)


def _vehicle(buffers, data, dtype, entries):
    """The Python buffer object the binding receives for an image or a plane.

    Its first entry is `offset` bytes into its buffer: the image's (or plane's) data pointer.
    Every entry aliases the first (a stride of 0), so the object reaches no byte past its first
    entry, whatever its entry count: the binding reads only its pointer, format and entry count.
    """
    _check_keys("data", data, {"buffer", "offset", "entries", "dtype"})
    memory = buffers[data["buffer"]]
    offset = int(data.get("offset", 0))
    dt = np.dtype(data.get("dtype", dtype))
    entries = int(data.get("entries", entries))
    end = offset + (dt.itemsize if entries > 0 else 0)
    if offset < 0 or end > memory.nbytes:
        raise ValueError(f"data {data!r}: a first entry of {dt.itemsize} bytes at offset {offset} "
                         f"isn't inside buffers[{data['buffer']}] ({memory.nbytes} bytes)")
    return np.ndarray(shape=(entries,), dtype=dt, buffer=memory, offset=offset, strides=(0,))


def _storage_dtype(bitdepth):
    """The binding's buffer type for a bit depth, float32 where it has none."""
    return DTYPES.get(bitdepth.name, np.float32) if bitdepth is not None else np.float32


# The binding's channel count per ordering (PyUtils.cpp:113-127), to give a request the entry
# count the binding requires by default.
ORDER_CHANNELS = {
    OCIO.CHANNEL_ORDERING_RGBA: 4, OCIO.CHANNEL_ORDERING_BGRA: 4, OCIO.CHANNEL_ORDERING_ABGR: 4,
    OCIO.CHANNEL_ORDERING_RGB: 3, OCIO.CHANNEL_ORDERING_BGR: 3,
}


def _prepare(image, buffers):
    """A function constructing the description of one image spec. Its arguments are built
    here, in the constructor's order, so that only the constructor runs when it is called."""
    common = {"kind", "width", "height", "bitdepth", "x_stride", "y_stride", "positional"}
    if image.get("kind") == "packed":
        _check_keys("a packed image", image, common | {"data", "num_channels", "chan_order",
                                                       "chan_stride"})
        if ("num_channels" in image) == ("chan_order" in image):
            raise ValueError(f"a packed image takes num_channels or chan_order: {image!r}")
    elif image.get("kind") == "planar":
        _check_keys("a planar image", image, common | {"planes"})
    width, height = int(image["width"]), int(image["height"])
    bitdepth = _enum(OCIO.BitDepth, image["bitdepth"]) if "bitdepth" in image else None
    dtype = _storage_dtype(bitdepth)
    if image["kind"] == "packed":
        if "num_channels" in image:
            name, channels = "numChannels", int(image["num_channels"])
            count = channels
        else:
            name, channels = "chanOrder", _enum(OCIO.ChannelOrdering, image["chan_order"])
            count = ORDER_CHANNELS.get(channels, 0)
        cls = OCIO.PackedImageDesc
        strides = [("chanStrideBytes", "chan_stride"), ("xStrideBytes", "x_stride"),
                   ("yStrideBytes", "y_stride")]
        kwargs = {"data": _vehicle(buffers, image["data"], dtype, width * height * count),
                  "width": width, "height": height, name: channels}
    elif image["kind"] == "planar":
        planes = image["planes"]
        if len(planes) not in (3, 4):
            raise ValueError(f"a planar image has 3 or 4 planes, not {len(planes)}")
        cls = OCIO.PlanarImageDesc
        strides = [("xStrideBytes", "x_stride"), ("yStrideBytes", "y_stride")]
        kwargs = {name: _vehicle(buffers, plane, dtype, width * height)
                  for name, plane in zip(("rData", "gData", "bData", "aData"), planes)}
        kwargs.update(width=width, height=height)
    else:
        raise ValueError(f"unknown image kind {image['kind']!r}")
    if bitdepth is not None:
        kwargs["bitDepth"] = bitdepth
        kwargs.update((arg, _stride(image.get(key))) for arg, key in strides)
    elif any(key in image for key in ("chan_stride", "x_stride", "y_stride")):
        raise ValueError("strides need a bitdepth: only the constructors with one take them")
    if image.get("positional"):
        return lambda: cls(*kwargs.values())
    return lambda: cls(**kwargs)


def _getters(desc):
    out = {
        "getBitDepth": desc.getBitDepth().name,
        "getWidth": desc.getWidth(),
        "getHeight": desc.getHeight(),
        "getXStrideBytes": desc.getXStrideBytes(),
        "getYStrideBytes": desc.getYStrideBytes(),
        "isRGBAPacked": desc.isRGBAPacked(),
        "isFloat": desc.isFloat(),
    }
    if isinstance(desc, OCIO.PackedImageDesc):
        out["getChannelOrder"] = desc.getChannelOrder().name
        out["getNumChannels"] = desc.getNumChannels()
        out["getChanStrideBytes"] = desc.getChanStrideBytes()
    return out


def _extent(start, width, height, x_stride, y_stride, item):
    """The bytes [lo, hi) holding `item` bytes at start + x * x_stride + y * y_stride, for
    every x < width and y < height."""
    dx, dy = (width - 1) * x_stride, (height - 1) * y_stride
    return start + min(0, dx) + min(0, dy), start + max(0, dx) + max(0, dy) + item


# Where alpha is in a packed pixel, per channel ordering (ImageDesc.cpp:143-186 @ v2.5.2).
ALPHA_POSITION = {OCIO.CHANNEL_ORDERING_RGBA: 3, OCIO.CHANNEL_ORDERING_BGRA: 3,
                  OCIO.CHANNEL_ORDERING_ABGR: 0}


def _channel_starts(desc, image, colors_only=False):
    """(buffer, byte offset) of each channel of pixel (0, 0), from the description's getters:
    data + k * chanStride for channel k of a packed image, or each plane of a planar one.
    With colors_only, alpha is left out."""
    if image["kind"] == "packed":
        data = image["data"]
        alpha = ALPHA_POSITION.get(desc.getChannelOrder()) if colors_only else None
        return [(data["buffer"], int(data.get("offset", 0)) + k * desc.getChanStrideBytes())
                for k in range(desc.getNumChannels()) if k != alpha]
    planes = image["planes"][:3] if colors_only else image["planes"]
    return [(plane["buffer"], int(plane.get("offset", 0))) for plane in planes]


def _check_inside(index, desc, image, buffers):
    """Refuses an image whose pixels aren't all in its buffers: channel c of pixel (x, y) is
    at the channel's start + x * xStride + y * yStride, AutoStride resolved by the library.

    An image the library calls RGBA-packed is read and written a row at a time instead: 4 *
    width contiguous channels from the data pointer plus y * yStride (ScanlineHelper.cpp:129-136,
    158-164; the ops work in a packed float destination's rows directly). Those rows are checked
    too: isRGBAPacked tests the x stride truncated to an int (ImageDesc.cpp:264), so an x stride
    of 4 channels plus a multiple of 2^32 is packed, and its rows are not where its pixels are."""
    item = CHANNEL_BYTES[desc.getBitDepth().name]
    width, height, y_stride = desc.getWidth(), desc.getHeight(), desc.getYStrideBytes()
    regions = [("pixels", buffer,
                *_extent(start, width, height, desc.getXStrideBytes(), y_stride, item))
               for buffer, start in _channel_starts(desc, image)]
    if desc.isRGBAPacked():
        data = image["data"]
        regions.append(("packed rows", data["buffer"],
                        *_extent(int(data.get("offset", 0)), 1, height, 0, y_stride,
                                 4 * item * width)))
    for what, buffer, lo, hi in regions:
        if lo < 0 or hi > buffers[buffer].nbytes:
            raise ValueError(
                f"image_apply refuses to apply to images[{index}]: its {what} span bytes "
                f"[{lo}, {hi}) of buffers[{buffer}], which has {buffers[buffer].nbytes} bytes, "
                f"so the library would read or write outside it")


# The largest code of the integer bit depths stored in wider types.
TOP_CODE = {"BIT_DEPTH_UINT10": 1023, "BIT_DEPTH_UINT12": 4095}


def _check_codes(index, desc, image, buffers, looks_up):
    """Refuses a 10- or 12-bit source image holding a red, green or blue code above the bit
    depth's largest, when the wheel would look the code up in a table of 1024 or 4096 entries,
    unchecked: that reads outside the table, which is undefined behavior (it crashed the
    oracle). It does so when the processor's first op is a forward 1D LUT, which
    CreateCPUEngine renders from the input bit depth (CPUProcessor.cpp:140-146), and which the
    default optimization makes for integer inputs: the forward renderers resample the LUT to
    the input's codes and index it with each color code (ops/lut1d/Lut1DOpCPU.cpp:58-64,
    388-409, 635-650). Alpha is scaled, not looked up (:646); the inverse renderers search the
    LUT (:1241-1284); every other first op converts codes with a multiply. `looks_up()` says
    whether the processor starts with a forward 1D LUT; it is asked only for a code above the
    largest.

    Each channel is read through a view with the image's strides, which _check_inside has kept
    inside the buffer; a stride of 0 (every pixel of a row or column the same bytes) is read
    once."""
    top = TOP_CODE.get(desc.getBitDepth().name)
    if top is None:
        return
    strides = (desc.getYStrideBytes(), desc.getXStrideBytes())
    shape = tuple(1 if stride == 0 else n
                  for n, stride in zip((desc.getHeight(), desc.getWidth()), strides))
    for buffer, start in _channel_starts(desc, image, colors_only=True):
        codes = np.ndarray(shape=shape, dtype="<u2", buffer=buffers[buffer], offset=start,
                           strides=strides)
        code = int(codes.max())
        if code > top and looks_up():
            raise ValueError(
                f"image_apply refuses to apply to images[{index}]: it holds the color code "
                f"{code}, above {top}, the largest of {desc.getBitDepth().name}, and the "
                f"processor starts with a forward 1D LUT, which the wheel would index with it "
                f"outside its table")


def _starts_with_forward_lut1d(proc, key):
    """Whether the processor that `proc.getOptimizedCPUProcessor(*key)` renders starts with a
    forward 1D LUT. getOptimizedProcessor(in, out, flags) runs the same finalize, optimize and
    bit-depth steps as the CPU processor's (Processor.cpp:382-401 and CPUProcessor.cpp:311-339);
    the CPU processor only adds an identity matrix when that leaves no op, which isn't a LUT.
    If OCIO raises here, where the CPU processor didn't, the command refuses rather than guess."""
    try:
        group = proc.getOptimizedProcessor(*key).createGroupTransform()
    except RAISED as exc:
        raise ValueError(f"image_apply can't tell which op the processor starts with: {exc}")
    return (len(group) > 0 and isinstance(group[0], OCIO.Lut1DTransform)
            and group[0].getDirection() == OCIO.TRANSFORM_DIR_FORWARD)


def _data_getters(desc, image, buffers, first_blob, out):
    """The binding's data getters, each called only if every byte it copies is in its buffer."""
    item = CHANNEL_BYTES[desc.getBitDepth().name]

    def copy(getter, data, count, stride):
        if data is None:
            return None
        lo, hi = _extent(int(data.get("offset", 0)), count, 1, stride, 0, item)
        if lo < 0 or hi > buffers[data["buffer"]].nbytes:
            return None
        out.append(np.asarray(getter()).tobytes())
        return first_blob + len(out) - 1

    pixels = desc.getWidth() * desc.getHeight()
    if image["kind"] == "packed":
        # PyPackedImageDesc.cpp:102-110: width * height * channels entries at the chan stride.
        return {"getData": copy(desc.getData, image["data"], pixels * desc.getNumChannels(),
                                desc.getChanStrideBytes())}
    # PyPlanarImageDesc.cpp:154-189: width * height contiguous entries from each plane.
    planes = image["planes"] + [None] * (4 - len(image["planes"]))
    return {name: copy(getattr(desc, name), plane, pixels, item)
            for name, plane in zip(("getRData", "getGData", "getBData", "getAData"), planes)}


def _cpu_processor(args, stage):
    """The processor and CPU processor of a request, chosen as cpu_apply chooses them: the
    default CPU processor, or getOptimizedCPUProcessor with optimization or non-F32 depths;
    and the (in, out, flags) the CPU processor was made with (the default one's are F32, F32
    and OPTIMIZATION_DEFAULT, Processor.cpp:527-535)."""
    _, proc = _processor(args, stage)
    stage[0] = "cpu_processor"
    in_bd = args.get("in_bitdepth", "BIT_DEPTH_F32")
    out_bd = args.get("out_bitdepth", "BIT_DEPTH_F32")
    if "optimization" in args or in_bd != "BIT_DEPTH_F32" or out_bd != "BIT_DEPTH_F32":
        key = (getattr(OCIO, in_bd), getattr(OCIO, out_bd), spec.flags(args.get("optimization")))
        cpu = proc.getOptimizedCPUProcessor(*key)
    else:
        key = (OCIO.BIT_DEPTH_F32, OCIO.BIT_DEPTH_F32, OCIO.OPTIMIZATION_DEFAULT)
        cpu = proc.getDefaultCPUProcessor()
    return proc, cpu, key


def _reads_source(descs, apply, cpu):
    """Whether cpu.apply(*[descs[i] for i in apply]) gets as far as reading the source: the
    scanline helper first checks both images' bit depths and dimensions
    (ImageDesc.cpp:93-96, ScanlineHelper.cpp:51-61, 85-90)."""
    src, dst = descs[apply[0]], descs[apply[-1]]
    return (src.getBitDepth() == cpu.getInputBitDepth()
            and dst.getBitDepth() == cpu.getOutputBitDepth()
            and (src.getWidth(), src.getHeight()) == (dst.getWidth(), dst.getHeight()))


def _processor_result(proc, cpu):
    return {
        "processor_cache_id": proc.getCacheID(),
        "cpu_cache_id": cpu.getCacheID(),
        "cpu_processor": {
            "getInputBitDepth": cpu.getInputBitDepth().name,
            "getOutputBitDepth": cpu.getOutputBitDepth().name,
            "isNoOp": cpu.isNoOp(),
            "isIdentity": cpu.isIdentity(),
            "hasChannelCrosstalk": cpu.hasChannelCrosstalk(),
        },
    }


@command
def image_apply(args, blobs):
    """Applies a CPU processor to images described with PackedImageDesc or PlanarImageDesc,
    in every form the Python binding offers, and returns the buffers they describe in full.

    args:
      buffers   the memory the images describe, a list of
                  {"blob": i}                     the bytes of request blob i
                  {"size": n, "fill": [b, ...]}   n bytes of the pattern b, ... repeated
                each copied into a fresh writable allocation that starts on a 64-byte boundary
      images    image specs, constructed in order, with keyword arguments so that the
                constructor is the one the spec names:
                {"kind": "packed", "data": data, "width": w, "height": h,
                 "num_channels": n, or "chan_order": CHANNEL_ORDERING_* name or integer,
                 "bitdepth": BIT_DEPTH_* name or integer (optional: with it, the constructor
                    with bitDepth and strides),
                 "chan_stride", "x_stride", "y_stride": bytes or "AutoStride" (the default),
                 "positional": true (optional: positional arguments instead, as a script
                    writes `PackedImageDesc(buf, w, h, CHANNEL_ORDERING_BGR)`; the binding
                    then takes the ChannelOrdering as numChannels, since it tries that
                    overload first and its enums convert to int)}
                {"kind": "planar", "planes": [R, G, B] or [R, G, B, A], each a data,
                 "width", "height", "bitdepth", "x_stride", "y_stride", "positional": as above}
                data = {"buffer": i, "offset": bytes (default 0), "entries": n, "dtype": name}
                is the Python buffer object the binding receives for the image or a plane. Its
                first entry is `offset` bytes into buffers[i]: that is the image's (or plane's)
                data pointer. `entries` (default: what the binding requires, width * height *
                channels for a packed image, width * height for a plane) and `dtype` (a numpy
                dtype; default: the bit depth's storage type, float32 without a bit depth) can
                be set to probe the binding's checks. Every entry of the object aliases the
                first, so the object reaches no byte past its first entry: the image's strides
                alone decide which bytes the library addresses. That is how padded, flipped
                and offset images, which upstream's tests use, reach the library from Python:
                the binding wants exactly width * height * channels entries whatever the
                strides (checkBufferSize), which in a script means a numpy view such as
                `image[::-1]` or `image[:, :w]`.
      apply     [] (the default: construct the images only, build no processor),
                [i] for cpu.apply(images[i]) in place, or [i, j] for
                cpu.apply(images[i], images[j]) (i == j allowed)
      data_getters  true: after the call, also the binding's getData() (packed) and
                getRData(), getGData(), getBData(), getAData() (planar). They copy width *
                height * channels entries at the channel stride (packed), or width * height
                contiguous entries (planar), whatever the image's strides. Each is called only
                if every byte it copies is in its buffer, and getAData() only with an alpha
                plane (without one, the binding returns uninitialized memory).
      config, transform or src/dst, direction, optimization, in_bitdepth, out_bitdepth
                the processor, as cpu_apply takes them; built first, only when apply isn't []

    Before calling apply, the command refuses the request (it raises, so the call fails) where
    the wheel would read or write outside its memory:
    - an image it applies to reaches outside its buffer: channel k of pixel (x, y) is at data +
      k * chanStride + x * xStride + y * yStride (packed) or plane + x * xStride + y * yStride
      (planar), from the description's own getters, so the library resolves AutoStride itself;
      and an image the library calls RGBA-packed, which it reads and writes a row at a time,
      also has its rows of 4 * width contiguous channels checked (see _check_inside);
    - the source is a 10- or 12-bit image holding a red, green or blue code above 1023 or
      4095, and the processor starts with a forward 1D LUT, which the wheel would index with
      the code outside its table (see _check_codes).
    Constructing a description reads no pixels, so a request that only constructs is never
    refused. Nor is a request whose apply raises before it reads the source (bit depths or
    dimensions that don't match).

    A spec with a key the command doesn't know is refused too, so a misspelled key can't pass
    unnoticed.

    result:
      images    per constructed image, its getters: getBitDepth, getWidth, getHeight,
                getXStrideBytes, getYStrideBytes, isRGBAPacked, isFloat, and for a packed image
                getChannelOrder, getNumChannels and getChanStrideBytes (enums by name); with
                data_getters, getData or getRData ... getAData: the index of its copy among the
                response blobs, or null where it wasn't called
      processor_cache_id, cpu_cache_id, cpu_processor (getInputBitDepth, getOutputBitDepth,
                isNoOp, isIdentity, hasChannelCrosstalk): when a processor was built
      exception, stage   when OCIO or the binding raised: {"type", "message"}, and where:
                "config", "transform", "processor", "cpu_processor" (as in cpu_apply), "image"
                (constructing images[image]: the binding's buffer checks, then the library's
                validation; also pybind11's TypeError when no constructor takes the arguments,
                such as a width beyond a C long), or "apply"
      image     with stage "image", the index of the image
      log       OCIO's log messages
    blobs: every buffer after the call, in order, also when OCIO raised; then the copies of the
           data getters
    """
    _check_keys("image_apply", args, PROCESSOR_KEYS | {"buffers", "images", "apply",
                                                       "data_getters"})
    buffers = _buffers(args.get("buffers") or [], blobs)
    images = args.get("images") or []
    makers = [_prepare(image, buffers) for image in images]
    apply = [int(i) for i in args.get("apply") or []]
    if len(apply) > 2 or any(i < 0 or i >= len(images) for i in apply):
        raise ValueError(f"apply {apply} with {len(images)} images")
    stage, result, copies, descs = ["config"], {"images": []}, [], []
    constructing = False
    with captured_log() as log:
        try:
            if apply:
                proc, cpu, key = _cpu_processor(args, stage)
                result.update(_processor_result(proc, cpu))
            stage[0] = "image"
            for make in makers:
                constructing = True
                desc = make()
                constructing = False
                descs.append(desc)
                result["images"].append(_getters(desc))
            if apply:
                for i in sorted(set(apply)):
                    _check_inside(i, descs[i], images[i], buffers)
                if _reads_source(descs, apply, cpu):
                    looks_up = functools.cache(lambda: _starts_with_forward_lut1d(proc, key))
                    _check_codes(apply[0], descs[apply[0]], images[apply[0]], buffers, looks_up)
                stage[0] = "apply"
                cpu.apply(*[descs[i] for i in apply])
            if args.get("data_getters"):
                for desc, image, getters in zip(descs, images, result["images"]):
                    getters.update(_data_getters(desc, image, buffers, len(buffers), copies))
        except (*RAISED, TypeError) as exc:
            # A TypeError is the binding's only while constructing an image (pybind11 found no
            # constructor for the arguments); anywhere else it is a bug in the request or here.
            if isinstance(exc, TypeError) and not constructing:
                raise
            result.update(exception=exception_result(exc), stage=stage[0])
            if stage[0] == "image":
                result["image"] = len(descs)
    result["log"] = log
    return result, [memory.tobytes() for memory in buffers] + copies


@command
def image_apply_rgb(args, blobs):
    """CPUProcessor.applyRGB or applyRGBA as Python calls them: on an array (any object with
    the buffer protocol), which they process in place, or on a list of floats, for which they
    return a new list. For either, the binding describes the values as one row of packed RGB
    or RGBA pixels, a PackedImageDesc with the array's bit depth (uint16 is BIT_DEPTH_UINT16)
    or F32 for a list, and calls CPUProcessor::apply on it (PyCPUProcessor.cpp:94-249).

    args:
      call      "applyRGB" or "applyRGBA"
      list      true: a list of Python floats (default: an array)
      dtype, shape, strides, offset
                the array: a numpy view of a fresh writable copy of blob 0 that starts on a
                64-byte boundary, with this dtype (default float32), shape (default: one
                dimension, the rest of the blob), strides in bytes (default: C-contiguous) and
                byte offset (default 0). numpy refuses a view that reaches outside the copy, and
                the binding refuses an array that isn't C-contiguous before the library sees
                it, so the library touches the view's own entries only
      config, transform or src/dst, direction, optimization, in_bitdepth, out_bitdepth
                the processor, as cpu_apply takes them
    blobs: [the array's memory, or the list's values as little-endian float64]
    result:
      processor_cache_id, cpu_cache_id, cpu_processor: as in image_apply, once built
      exception, stage   when OCIO or the binding raised, as in image_apply: "config", ...,
                "cpu_processor", or "apply" (the binding's checks, then the library's)
      log       OCIO's log messages
    blobs: [the array's whole memory after the call, also when OCIO raised; or the returned
            list as little-endian float64, when nothing raised]
    """
    array_keys = set() if args.get("list") else {"dtype", "shape", "strides", "offset"}
    _check_keys("image_apply_rgb", args, PROCESSOR_KEYS | {"call", "list"} | array_keys)
    call = args["call"]
    if call not in ("applyRGB", "applyRGBA"):
        raise ValueError(f"call {call!r}: applyRGB or applyRGBA")
    if args.get("list"):
        memory, data = None, np.frombuffer(blobs[0], dtype="<f8").tolist()
    else:
        (memory,) = _buffers([{"blob": 0}], blobs)
        dtype = np.dtype(args.get("dtype", "float32"))
        offset = int(args.get("offset", 0))
        shape = args.get("shape") or [(memory.nbytes - offset) // dtype.itemsize]
        strides = args.get("strides")
        data = np.ndarray(shape=tuple(shape), dtype=dtype, buffer=memory, offset=offset,
                          strides=tuple(strides) if strides is not None else None)
    stage, result, out = ["config"], {}, []
    with captured_log() as log:
        try:
            proc, cpu, _ = _cpu_processor(args, stage)
            result.update(_processor_result(proc, cpu))
            stage[0] = "apply"
            returned = getattr(cpu, call)(data)
            if memory is None:
                out.append(np.array(returned, dtype="<f8").tobytes())
        except RAISED as exc:
            result.update(exception=exception_result(exc), stage=stage[0])
    result["log"] = log
    return result, out if memory is None else [memory.tobytes()]
