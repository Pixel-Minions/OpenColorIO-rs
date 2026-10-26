# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Lossless public-API transport to the wheel-linked legacy helper (O7.1).

The original transforms, not createGroupTransform's reconstructed ops, are copied with the
getters/setters of OpenColorTransforms.h @ v2.5.2. Thus allocation and file no-ops survive.
All doubles and floats travel as IEEE bytes; ordinary YAML/CTF serialization rounds them.
The paired C++ reader is oracle/native/transport.h. This module computes no processor output.
"""

import struct

import numpy as np
import PyOpenColorIO as OCIO

PRIMARY_RGB = ("brightness", "contrast", "gamma", "offset", "exposure", "lift", "gain")
PRIMARY_SCALAR = ("saturation", "pivot", "pivotBlack", "pivotWhite", "clampBlack", "clampWhite")
TONE_RGB = ("blacks", "shadows", "midtones", "highlights", "whites")
RGBM = ("red", "green", "blue", "master")
RGBMSW = RGBM + ("start", "width")
LOG_VECTORS = ("LogSideSlopeValue", "LogSideOffsetValue", "LinSideSlopeValue", "LinSideOffsetValue")
RGB_CURVES = ("red", "green", "blue", "master")
HUE_CURVES = ("hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx")


class Writer:
    """A little-endian wire message, copied without decimal conversion."""

    def __init__(self):
        self.data = bytearray()

    def u(self, value):
        self.data.extend(struct.pack("<I", int(value)))

    def d(self, value):
        self.data.extend(struct.pack("<d", value))

    def text(self, value):
        self.blob(value.encode("utf-8"))

    def blob(self, data):
        self.u(len(data))
        self.data.extend(data)

    def doubles(self, values):
        for value in values:
            self.d(value)

    def floats(self, values):
        array = np.asarray(values, dtype="<f4")
        self.blob(array.tobytes())


class Reader:
    """A checked reader of the helper's byte response; trailing bytes are an error."""

    def __init__(self, data):
        self.data = memoryview(data)
        self.at = 0

    def take(self, size):
        if size < 0 or self.at + size > len(self.data):
            raise ValueError("the native legacy helper returned a truncated response")
        result = self.data[self.at:self.at + size]
        self.at += size
        return result

    def u(self):
        return struct.unpack("<I", self.take(4))[0]

    def d(self):
        return struct.unpack("<d", self.take(8))[0]

    def blob(self):
        return self.take(self.u()).tobytes()

    def text(self):
        return self.blob().decode("utf-8")

    def done(self):
        if self.at != len(self.data):
            raise ValueError("the native legacy helper returned trailing response bytes")


def write_metadata(w, metadata):
    """FormatMetadata's public tree, including attribute order (OpenColorTransforms.h:27-110)."""
    w.text(metadata.getElementName())
    w.text(metadata.getElementValue())
    attributes = list(metadata.getAttributes())
    w.u(len(attributes))
    for key, value in attributes:
        w.text(key)
        w.text(value)
    children = list(metadata.getChildElements())
    w.u(len(children))
    for child in children:
        write_metadata(w, child)


def write_primary(w, value):
    """GradingPrimary's public fields (OpenColorTransforms.h:461-493 @ v2.5.2)."""
    for name in PRIMARY_RGB:
        w.doubles(getattr(getattr(value, name), component) for component in RGBM)
    w.doubles(getattr(value, name) for name in PRIMARY_SCALAR)


def read_primary(r):
    value = OCIO.GradingPrimary(OCIO.GRADING_LOG)
    for name in PRIMARY_RGB:
        setattr(value, name, OCIO.GradingRGBM(*(r.d() for _ in RGBM)))
    for name in PRIMARY_SCALAR:
        setattr(value, name, r.d())
    return value


def write_tone(w, value):
    """GradingTone's public fields (OpenColorTransforms.h:681-716 @ v2.5.2)."""
    for name in TONE_RGB:
        w.doubles(getattr(getattr(value, name), component) for component in RGBMSW)
    w.d(value.scontrast)


def read_tone(r):
    value = OCIO.GradingTone(OCIO.GRADING_LOG)
    for name in TONE_RGB:
        setattr(value, name, OCIO.GradingRGBMSW(*(r.d() for _ in RGBMSW)))
    value.scontrast = r.d()
    return value


def write_curves(w, value, hue):
    """Curves, points and authored slopes (OpenColorTransforms.h:511-636 @ v2.5.2)."""
    for name in HUE_CURVES if hue else RGB_CURVES:
        curve = getattr(value, name)
        w.u(curve.getSplineType())
        points = list(curve.getControlPoints())
        w.u(len(points))
        for point in points:
            w.doubles((point.x, point.y))
        w.u(not curve.slopesAreDefault())
        if not curve.slopesAreDefault():
            w.doubles(curve.getSlopes())
    if hue:
        w.u(value.getDrawCurveOnly())


def read_curves(r, hue):
    value = OCIO.GradingHueCurve(OCIO.GRADING_LOG) if hue else OCIO.GradingRGBCurve(OCIO.GRADING_LOG)
    for name in HUE_CURVES if hue else RGB_CURVES:
        spline, count = r.u(), r.u()
        points = [OCIO.GradingControlPoint(r.d(), r.d()) for _ in range(count)]
        curve = getattr(value, name)
        curve.setSplineType(OCIO.BSplineType(spline))
        curve.setNumControlPoints(count)
        target = curve.getControlPoints()
        for index, point in enumerate(points):
            target[index] = point
        if r.u():
            curve.setSlopes([r.d() for _ in range(count)])
    if hue:
        value.setDrawCurveOnly(bool(r.u()))
    return value


def write_transform(w, transform):
    """Original transform fields; paired with read_transform in native/transport.h.

    Public getter/setter contracts: OpenColorTransforms.h:152-2213 @ v2.5.2. Grading styles
    precede their values (setStyle resets them), and LUT dimensions precede their raw values.
    """
    name = type(transform).__name__
    w.text(name)
    w.u(transform.getDirection())
    if name == "GroupTransform":
        w.u(len(transform))
        for child in transform:
            write_transform(w, child)
    elif name == "AllocationTransform":
        w.u(transform.getAllocation())
        w.floats(transform.getVars())
    elif name == "BuiltinTransform":
        w.text(transform.getStyle())
    elif name == "CDLTransform":
        w.u(transform.getStyle())
        w.doubles(transform.getSOP())
        w.d(transform.getSat())
    elif name in ("ColorSpaceTransform", "LookTransform"):
        w.text(transform.getSrc())
        w.text(transform.getDst())
        if name == "LookTransform":
            w.text(transform.getLooks())
            w.u(transform.getSkipColorSpaceConversion())
        else:
            w.u(transform.getDataBypass())
    elif name == "DisplayViewTransform":
        for field in ("Src", "Display", "View"):
            w.text(getattr(transform, "get" + field)())
        w.u(transform.getLooksBypass())
        w.u(transform.getDataBypass())
    elif name == "ExponentTransform":
        w.u(transform.getNegativeStyle())
        w.doubles(transform.getValue())
    elif name == "ExponentWithLinearTransform":
        w.u(transform.getNegativeStyle())
        w.doubles(transform.getGamma())
        w.doubles(transform.getOffset())
    elif name == "ExposureContrastTransform":
        w.u(transform.getStyle())
        for field in ("Exposure", "Contrast", "Gamma", "Pivot", "LogExposureStep", "LogMidGray"):
            w.d(getattr(transform, "get" + field)())
        for field in ("Exposure", "Contrast", "Gamma"):
            w.u(getattr(transform, "is" + field + "Dynamic")())
    elif name == "FileTransform":
        w.text(transform.getSrc())
        w.text(transform.getCCCId())
        w.u(transform.getCDLStyle())
        w.u(transform.getInterpolation())
    elif name == "FixedFunctionTransform":
        w.u(transform.getStyle())
        params = list(transform.getParams())
        w.u(len(params))
        w.doubles(params)
    elif name.startswith("Grading") and name.endswith("Transform"):
        w.u(transform.getStyle())
        if name == "GradingPrimaryTransform":
            write_primary(w, transform.getValue())
        elif name == "GradingToneTransform":
            write_tone(w, transform.getValue())
        else:
            hue = name == "GradingHueCurveTransform"
            write_curves(w, transform.getValue(), hue)
            w.u(transform.getRGBToHSY() if hue else transform.getBypassLinToLog())
        w.u(transform.isDynamic())
    elif name in ("LogAffineTransform", "LogCameraTransform", "LogTransform"):
        if name == "LogCameraTransform":
            w.doubles(transform.getLinSideBreakValue())
        w.d(transform.getBase())
        if name != "LogTransform":
            for field in LOG_VECTORS:
                w.doubles(getattr(transform, "get" + field)())
        if name == "LogCameraTransform":
            # Python's getter drops the C++ bool indicating presence. operator<< preserves
            # that bool, without using its rounded numbers (LogCameraTransform.cpp:154-176).
            present = ", linearSlope=[" in str(transform)
            w.u(present)
            if present:
                w.doubles(transform.getLinearSlopeValue())
    elif name in ("Lut1DTransform", "Lut3DTransform"):
        w.u(transform.getLength() if name == "Lut1DTransform" else transform.getGridSize())
        if name == "Lut1DTransform":
            w.u(transform.getInputHalfDomain())
            w.u(transform.getOutputRawHalfs())
            w.u(transform.getHueAdjust())
        w.u(transform.getInterpolation())
        w.u(transform.getFileOutputBitDepth())
        w.floats(transform.getData())
    elif name == "MatrixTransform":
        w.doubles(transform.getMatrix())
        w.doubles(transform.getOffset())
        w.u(transform.getFileInputBitDepth())
        w.u(transform.getFileOutputBitDepth())
    elif name == "RangeTransform":
        w.u(transform.getStyle())
        w.u(transform.getFileInputBitDepth())
        w.u(transform.getFileOutputBitDepth())
        for field in ("MinInValue", "MaxInValue", "MinOutValue", "MaxOutValue"):
            present = getattr(transform, "has" + field)()
            w.u(present)
            if present:
                w.d(getattr(transform, "get" + field)())
    else:
        raise ValueError(f"native legacy transport has no reader for {name}")
    metadata = getattr(transform, "getFormatMetadata", None)
    w.u(metadata is not None)
    if metadata is not None:
        write_metadata(w, metadata())
