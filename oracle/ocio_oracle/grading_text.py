# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The cases of the `grading_text` fixture group (card p5-m3-parity, chunk O5.3): generated
ExposureContrast and grading transforms and grading values, and what the library says about
them.

Each case is a spec (spec.py): a transform spec, built the way the binding's constructors and
setters build it, or an object spec of a grading value. The specs are inputs only; what the
fixtures hold for them comes from the wheel (`texts`, `serialized`):
- for every case, repr() (upstream's operator<<, PyUtils.h defRepr) and what validate()
  raises, or what building it raised (a setter's refusal);
- for every transform the wheel builds, its CTF text (GroupTransform.write("Color Transform
  Format") of a group holding it, with the raw config), or what writing raised;
- per transform class, Config.serialize() of a config with a color space per transform the
  wheel builds, the transform as its to_reference transform: each transform's YAML.

The values are each class's defaults per style, typical values, every numeric field ("slot")
in every style set to a NaN, -0.0 and +Inf (the logic of the writers: a value compared with its
default, a style's own fields), and a list of doubles that print in their own way (NaNs,
infinities, zeros, subnormals, the extremes, values at the edges of 6, 7, 15 and 17
significant digits) each in two slots. Directions and dynamic flags vary across the cases.

Where the platforms' C++ libraries spell a value differently (I-7: iostream writes a negative
NaN `-nan(ind)` on Windows and `-nan` on Linux, a signalling one `nan(snan)` on Windows), the
case is `"spelled": true`: only its YAML is a fixture, as yaml-cpp writes every NaN `.nan`;
the port's repr and CTF text of those cases are compared live
(crates/ocio/tests/grading_text_oracle.rs).

The binding has no repr() for GradingRGBCurve (PyGradingData.cpp:470 @ v2.5.2 binds
GradingHueCurve's twice), so a GradingRGBCurve's operator<< is only seen inside its
transform's.
"""

import json
import math
import struct

import PyOpenColorIO as OCIO

from . import spec
from .commands import RAISED, captured_log, exception_result, wheel_raised


def f64(x):
    """A double as a value spec, by its bits (NaN payloads and -0.0 pass exactly)."""
    return {"f64": struct.unpack("<Q", struct.pack("<d", x))[0]}


def _bits(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def _f32(x):
    """The float nearest x (round to nearest even, as the binding's conversion does)."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


# Doubles that print the same on both platforms, each with a name for the case labels.
DOUBLES = [
    ("nan", _bits(0x7FF8000000000000)),
    ("nan payload", _bits(0x7FF80000DEADBEEF)),
    ("inf", math.inf),
    ("-inf", -math.inf),
    ("0", 0.0),
    ("-0", -0.0),
    ("min subnormal", _bits(1)),
    ("-min subnormal", -_bits(1)),
    ("min normal", 2.2250738585072014e-308),
    ("max", 1.7976931348623157e308),
    ("lowest", -1.7976931348623157e308),
    ("1e300", 1e300),
    ("-1e-300", -1e-300),
    ("float max", 3.4028234663852886e38),
    ("float min normal", 1.1754943508222875e-38),
    ("0.1", 0.1),
    ("1/3", 1.0 / 3.0),
    ("-2/3", -2.0 / 3.0),
    ("1e-7", 1e-7),
    ("1e-5", 1e-5),
    ("0.0001", 0.0001),
    ("0.01", 0.01),
    ("below 0.01", 0.009999999999999998),
    ("123456.5", 123456.5),
    ("999999.4", 999999.4),
    ("999999.5", 999999.5),
    ("1e6", 1e6),
    ("1234567", 1234567.0),
    ("9999999.5", 9999999.5),
    ("9999999999999998", 9999999999999998.0),
    ("1e15+0.3", 1e15 + 0.3),
    ("1e16", 1e16),
    ("1.2345678901234568e17", 123456789012345680.0),
    ("1e21", 1e21),
    ("0.12345678901234568", 0.123456789012345678),
    ("1+ulp", 1.0000000000000002),
    ("1-ulp", 0.9999999999999999),
    ("0.5", 0.5),
    ("-1", -1.0),
    ("2", 2.0),
]

# Doubles the platforms' iostreams spell differently (I-7): YAML only.
SPELLED_DOUBLES = [
    ("-nan", _bits(0xFFF8000000000000)),
    ("-nan payload", _bits(0xFFF80000DEADBEEF)),
    ("snan", _bits(0x7FF0000000000001)),
    ("-snan", _bits(0xFFF0000000000001)),
]

# Floats (a curve's control points and slopes): what the binding's conversion of a double to
# float gives. A NaN keeps its sign through the conversion, a signalling one is quieted.
FLOATS = [
    ("nan", _bits(0x7FF8000000000000)),
    ("inf", math.inf),
    ("-inf", -math.inf),
    ("0", 0.0),
    ("-0", -0.0),
    ("min subnormal", _f32(1.401298464324817e-45)),
    ("max", 3.4028234663852886e38),
    ("lowest", -3.4028234663852886e38),
    ("min normal", 1.1754943508222875e-38),
    ("0.1", _f32(0.1)),
    ("1/3", _f32(1.0 / 3.0)),
    ("1e-7", _f32(1e-7)),
    ("123456.5", 123456.5),
    ("999999.4", _f32(999999.4)),
    ("1234567", 1234567.0),
    ("16777217", 16777216.0),
    ("0.12345679", _f32(0.123456789)),
    ("1+ulp", _f32(1.0000001192092896)),
    ("1-ulp", _f32(0.9999999403953552)),
    ("0.5", 0.5),
    ("-1", -1.0),
    ("2", 2.0),
]

SPELLED_FLOATS = [
    ("-nan", _bits(0xFFF8000000000000)),
]

# The values every slot takes in every style: the writers compare a value with its default
# (NaN never equals it, -0.0 equals 0.0) and print a style's own fields.
LOGIC = [("nan", _bits(0x7FF8000000000000)), ("-0", -0.0), ("inf", math.inf)]

GRADING_STYLES = ["GRADING_LOG", "GRADING_LIN", "GRADING_VIDEO"]
DIRECTIONS = ["TRANSFORM_DIR_FORWARD", "TRANSFORM_DIR_INVERSE"]


def _enum(name):
    return {"enum": name}


def _case(label, kind, case_spec, spelled=False):
    return {"label": label, "kind": kind, "spec": case_spec, "spelled": spelled}


# ExposureContrastTransform

EC_STYLES = ["EXPOSURE_CONTRAST_LINEAR", "EXPOSURE_CONTRAST_VIDEO",
             "EXPOSURE_CONTRAST_LOGARITHMIC"]
EC_SLOTS = ["Exposure", "Contrast", "Gamma", "Pivot", "LogExposureStep", "LogMidGray"]
EC_DYNAMIC = [(), ("Exposure", "Contrast", "Gamma"), ("Exposure",), ("Contrast",), ("Gamma",)]


def _ec(style, direction, dynamic, values):
    """ExposureContrastTransform(), then setStyle, a setter per value, make*Dynamic and
    setDirection (PyExposureContrastTransform.cpp @ v2.5.2)."""
    calls = [["setStyle", _enum(style)]]
    calls += [[f"set{slot}", f64(v)] for slot, v in values]
    calls += [[f"make{d}Dynamic"] for d in dynamic]
    calls.append(["setDirection", _enum(direction)])
    return {"class": "ExposureContrastTransform", "calls": calls}


def ec_cases():
    cases = []
    typical = [("Exposure", 0.25), ("Contrast", 1.3), ("Gamma", 0.8), ("Pivot", 0.18),
               ("LogExposureStep", 0.1), ("LogMidGray", 0.435)]
    for style in EC_STYLES:
        for direction in DIRECTIONS:
            for dynamic in EC_DYNAMIC:
                label = f"{style} {direction} dynamic {'+'.join(dynamic) or 'none'}"
                cases.append(_case(f"{label} default", "transform",
                                   _ec(style, direction, dynamic, [])))
            for dynamic in EC_DYNAMIC[:2]:
                label = f"{style} {direction} dynamic {'+'.join(dynamic) or 'none'} typical"
                cases.append(_case(label, "transform", _ec(style, direction, dynamic, typical)))
    k = 0
    for style in EC_STYLES:
        for slot in EC_SLOTS:
            for name, v in LOGIC:
                direction = DIRECTIONS[k % 2]
                dynamic = (slot,) if slot in ("Exposure", "Contrast", "Gamma") and k % 3 == 0 \
                    else ()
                k += 1
                cases.append(_case(f"{style} {direction} {slot} = {name} dynamic {bool(dynamic)}",
                                   "transform", _ec(style, direction, dynamic, [(slot, v)])))
    for spelled, values in ((False, DOUBLES), (True, SPELLED_DOUBLES)):
        for i, (name, v) in enumerate(values):
            for j in range(2):
                slot = EC_SLOTS[(2 * i + 3 * j) % len(EC_SLOTS)]
                style = EC_STYLES[(i + j) % 3]
                direction = DIRECTIONS[(i // 3 + j) % 2]
                cases.append(_case(f"{style} {direction} {slot} = {name}", "transform",
                                   _ec(style, direction, (), [(slot, v)]), spelled))
    return cases


# GradingPrimaryTransform

RGBM = ["red", "green", "blue", "master"]
PRIMARY_RGBM = ["brightness", "contrast", "gamma", "offset", "exposure", "lift", "gain"]
PRIMARY_SCALARS = ["saturation", "pivot", "pivotBlack", "pivotWhite", "clampBlack",
                   "clampWhite"]
PRIMARY_SLOTS = [(f, c) for f in PRIMARY_RGBM for c in RGBM] + \
    [(f, None) for f in PRIMARY_SCALARS]


def _rgbm(values):
    return {"object": {"class": "GradingRGBM", "args": [f64(v) for v in values]}}


def _primary_value(style, changes):
    """GradingPrimary(style), then each field `changes` {(field, component): value} touches
    set, in the order of PRIMARY_SLOTS: a GradingRGBM field whole, its other components the
    style's defaults the wheel gives (so the spec names every value it sets)."""
    default = OCIO.GradingPrimary(_enum_value(style))
    touched = {field for field, _ in changes}
    attrs = []
    for field in PRIMARY_RGBM:
        if field in touched:
            rgbm = getattr(default, field)
            attrs.append([field, _rgbm([changes.get((field, c), getattr(rgbm, c))
                                        for c in RGBM])])
    for field in PRIMARY_SCALARS:
        if field in touched:
            attrs.append([field, f64(changes[(field, None)])])
    return {"object": {"class": "GradingPrimary", "args": [_enum(style)], "attrs": attrs}}


def _enum_value(name):
    return spec.value(_enum(name))


def _grading(cls, style, direction, dynamic, value=None, calls=()):
    """`cls`(style, dynamic, dir), then setValue(value) when there is one, then `calls`
    (PyGrading*Transform.cpp @ v2.5.2)."""
    out = {"class": cls, "args": {"style": _enum(style), "dynamic": dynamic,
                                  "dir": _enum(direction)}, "calls": []}
    if value is not None:
        out["calls"].append(["setValue", value])
    out["calls"].extend(list(c) for c in calls)
    return out


def _slot_name(slot):
    field, component = slot
    return field if component is None else f"{field}.{component}"


def _slot_cases(cls, slots, value_of, typical_of):
    """Defaults and typical values per style, direction and dynamic flag; each slot set to
    each LOGIC value in every style; each of DOUBLES and SPELLED_DOUBLES in two slots."""
    cases = []
    for style in GRADING_STYLES:
        for direction in DIRECTIONS:
            for dynamic in (False, True):
                label = f"{style} {direction} dynamic {dynamic}"
                cases.append(_case(f"{label} default", "transform",
                                   _grading(cls, style, direction, dynamic)))
                cases.append(_case(f"{label} typical", "transform",
                                   _grading(cls, style, direction, dynamic,
                                            value_of(style, typical_of(style)))))
    k = 0
    for slot in slots:
        for name, v in LOGIC:
            for style in GRADING_STYLES:
                direction = DIRECTIONS[k % 2]
                dynamic = k % 5 == 0
                k += 1
                cases.append(_case(
                    f"{style} {direction} dynamic {dynamic} {_slot_name(slot)} = {name}",
                    "transform",
                    _grading(cls, style, direction, dynamic, value_of(style, {slot: v}))))
    for spelled, values in ((False, DOUBLES), (True, SPELLED_DOUBLES)):
        for i, (name, v) in enumerate(values):
            for j in range(2):
                slot = slots[(7 * i + 13 * j) % len(slots)]
                style = GRADING_STYLES[(i + j) % 3]
                direction = DIRECTIONS[(i // 3 + j) % 2]
                cases.append(_case(f"{style} {direction} {_slot_name(slot)} = {name}",
                                   "transform",
                                   _grading(cls, style, direction, False,
                                            value_of(style, {slot: v})),
                                   spelled))
    return cases


def primary_cases():
    def typical(style):
        t = {}
        for i, field in enumerate(PRIMARY_RGBM):
            for j, c in enumerate(RGBM):
                t[(field, c)] = [1.1, 0.9, 1.2, 1.05][j] + 0.01 * i
        t[("brightness", "red")] = 0.1
        t[("offset", "green")] = -0.02
        t[("exposure", "blue")] = -0.25
        t[("lift", "master")] = 0.02
        t.update({("saturation", None): 1.3, ("pivot", None): 0.123456789,
                  ("pivotBlack", None): 0.05, ("pivotWhite", None): 0.95,
                  ("clampBlack", None): -0.1, ("clampWhite", None): 1.2})
        return t
    cases = _slot_cases("GradingPrimaryTransform", PRIMARY_SLOTS, _primary_value, typical)
    # The clamps print and save only when they clamp: the NoClamp bounds and their neighbours.
    no_black = OCIO.GradingPrimary.NoClampBlack
    no_white = OCIO.GradingPrimary.NoClampWhite
    for style in GRADING_STYLES:
        for black, white in ((no_black, no_white), (-1e300, no_white), (no_black, 1e300),
                             (0.0, 1.0)):
            changes = {("clampBlack", None): black, ("clampWhite", None): white}
            cases.append(_case(f"{style} clamps {black!r} {white!r}", "transform",
                               _grading("GradingPrimaryTransform", style, DIRECTIONS[0], False,
                                        _primary_value(style, changes))))
    return cases


# GradingToneTransform

TONE_RGBMSW = ["blacks", "shadows", "midtones", "highlights", "whites"]
RGBMSW = ["red", "green", "blue", "master", "start", "width"]
TONE_SLOTS = [(f, c) for f in TONE_RGBMSW for c in RGBMSW] + [("scontrast", None)]


def _rgbmsw(values):
    return {"object": {"class": "GradingRGBMSW", "args": [f64(v) for v in values]}}


def _tone_value(style, changes):
    """GradingTone(style), then each field `changes` touches set, as _primary_value does."""
    default = OCIO.GradingTone(_enum_value(style))
    touched = {field for field, _ in changes}
    attrs = []
    for field in TONE_RGBMSW:
        if field in touched:
            rgbmsw = getattr(default, field)
            attrs.append([field, _rgbmsw([changes.get((field, c), getattr(rgbmsw, c))
                                          for c in RGBMSW])])
    if "scontrast" in touched:
        attrs.append(["scontrast", f64(changes[("scontrast", None)])])
    return {"object": {"class": "GradingTone", "args": [_enum(style)], "attrs": attrs}}


def tone_cases():
    def typical(style):
        t = {}
        for i, field in enumerate(TONE_RGBMSW):
            for j, c in enumerate(RGBM):
                t[(field, c)] = [1.1, 0.9, 1.2, 1.05][j] - 0.02 * i
        t[("scontrast", None)] = 1.3
        return t
    return _slot_cases("GradingToneTransform", TONE_SLOTS, _tone_value, typical)


# GradingRGBCurveTransform

def _points(curve):
    return [(p.x, p.y) for p in curve.getControlPoints()]


def _bspline(points, slopes=None, curve_type=None):
    """GradingBSplineCurve([x0, y0, ...]) (with the hue curve type when there is one), then
    setSlopes when there are slopes."""
    args = [[f64(c) for p in points for c in p]]
    if curve_type is not None:
        args.append(_enum(curve_type))
    out = {"class": "GradingBSplineCurve", "args": args}
    if slopes is not None:
        out["calls"] = [["setSlopes", [f64(s) for s in slopes]]]
    return {"object": out}


RGB_CURVES = ["red", "green", "blue", "master"]


def _curves_value(cls, names, hue, style, changes):
    """`cls`(style), then each curve `changes` {(curve, point, coordinate): value} touches set
    through its property (the binding copies the curve in, PyGradingData.cpp:22-33 @ v2.5.2),
    in the order of `names`: the style's default curve the wheel gives, as
    GradingBSplineCurve([x0, y0, ...]) (with the hue curve type when `hue`), with the changes
    applied, coordinate 0 for x, 1 for y and "slope" for the point's slope (then set with
    setSlopes, the other points' slopes 0, their defaults)."""
    default = getattr(OCIO, cls)(_enum_value(style))
    touched = {curve for curve, _, _ in changes}
    attrs = []
    for name in names:
        if name not in touched:
            continue
        points = [list(p) for p in _points(getattr(default, name))]
        slopes = None
        for (curve, point, coord), v in changes.items():
            if curve != name:
                continue
            if coord == "slope":
                slopes = slopes or [0.0] * len(points)
                slopes[point] = v
            else:
                points[point][coord] = v
        attrs.append([name, _bspline(points, slopes, name.upper() if hue else None)])
    return {"object": {"class": cls, "args": [_enum(style)], "attrs": attrs}}


def _rgb_curve_value(style, changes):
    return _curves_value("GradingRGBCurve", RGB_CURVES, False, style, changes)


def _curve_cases(cls, slots, value_of, typical_of, extra, excluded=lambda slot, v: False):
    """As _slot_cases, for curves: LOGIC in each slot with the style cycling, FLOATS and
    SPELLED_FLOATS each in two slots. A value `excluded` leaves out of a slot is left out of
    it (LOGIC), or goes to the next slot that takes it (FLOATS)."""
    cases = []
    for style in GRADING_STYLES:
        for direction in DIRECTIONS:
            for dynamic in (False, True):
                label = f"{style} {direction} dynamic {dynamic}"
                cases.append(_case(f"{label} default", "transform",
                                   _grading(cls, style, direction, dynamic)))
                cases.append(_case(f"{label} typical", "transform",
                                   _grading(cls, style, direction, dynamic,
                                            value_of(style, typical_of(style)))))
    cases.extend(extra)
    k = 0
    for slot in slots:
        for name, v in LOGIC:
            if excluded(slot, v):
                continue
            style = GRADING_STYLES[k % 3]
            direction = DIRECTIONS[k // 3 % 2]
            dynamic = k % 5 == 0
            k += 1
            cases.append(_case(f"{style} {direction} dynamic {dynamic} {slot} = {name}",
                               "transform",
                               _grading(cls, style, direction, dynamic, value_of(style, {slot: v}))))
    for spelled, values in ((False, FLOATS), (True, SPELLED_FLOATS)):
        for i, (name, v) in enumerate(values):
            for j in range(2):
                n = 5 * i + 11 * j
                while excluded(slots[n % len(slots)], v):
                    n += 1
                slot = slots[n % len(slots)]
                style = GRADING_STYLES[(i + j) % 3]
                direction = DIRECTIONS[(i // 3 + j) % 2]
                cases.append(_case(f"{style} {direction} {slot} = {name}", "transform",
                                   _grading(cls, style, direction, False,
                                            value_of(style, {slot: v})),
                                   spelled))
    return cases


def rgb_curve_cases():
    slots = [(c, p, xy) for c in ("red", "master") for p in range(3) for xy in (0, 1)] + \
        [("green", p, "slope") for p in range(3)]

    def typical(style):
        return {("red", 1, 1): 0.6, ("green", 1, 0): 0.4, ("blue", 0, 1): 0.05,
                ("master", 2, 1): 0.95, ("master", 1, "slope"): 1.25}
    extra = []
    for style in GRADING_STYLES:
        for direction in DIRECTIONS:
            extra.append(_case(f"{style} {direction} bypass lin to log", "transform",
                               _grading("GradingRGBCurveTransform", style, direction, False,
                                        value_of_rgb(style, typical(style)),
                                        [["setBypassLinToLog", True]])))
            extra.append(_case(f"{style} {direction} setSlope", "transform",
                               _grading("GradingRGBCurveTransform", style, direction, True,
                                        None, [["setSlope", _enum("RGB_BLUE"), 1, f64(0.75)]])))
    # A NaN x coordinate makes the wheel read past the control points as it fits the curve
    # (docs/improvements.md, U-35): the port refuses it, and no fixture holds one.
    def nan_x(slot, v):
        return slot[2] == 0 and math.isnan(v)
    return _curve_cases("GradingRGBCurveTransform", slots, value_of_rgb, typical, extra, nan_x)


value_of_rgb = _rgb_curve_value


# GradingHueCurveTransform

HUE_CURVES = ["hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum",
              "hue_fx"]


def _hue_curve_value(style, changes):
    return _curves_value("GradingHueCurve", HUE_CURVES, True, style, changes)


def hue_curve_cases():
    slots = [(c, p, xy) for c in HUE_CURVES for p, xy in ((0, 0), (0, 1), (1, 1))] + \
        [(c, 1, "slope") for c in HUE_CURVES]

    def typical(style):
        return {("hue_hue", 1, 1): 0.2, ("hue_sat", 2, 1): 1.2, ("lum_sat", 0, 1): 1.1,
                ("sat_sat", 1, 1): 0.55, ("lum_lum", 1, 1): 0.45, ("hue_fx", 3, 1): 0.1,
                ("sat_sat", 1, "slope"): 1.2}
    extra = []
    for style in GRADING_STYLES:
        for direction in DIRECTIONS:
            extra.append(_case(f"{style} {direction} hsy none", "transform",
                               _grading("GradingHueCurveTransform", style, direction, False,
                                        _hue_curve_value(style, typical(style)),
                                        [["setRGBToHSY", _enum("HSY_TRANSFORM_NONE")]])))
            extra.append(_case(f"{style} {direction} setSlope", "transform",
                               _grading("GradingHueCurveTransform", style, direction, True,
                                        None, [["setSlope", _enum("HUE_SAT"), 2, f64(0.5)]])))
    return _curve_cases("GradingHueCurveTransform", slots, _hue_curve_value, typical, extra)


# The grading values on their own.

def value_cases():
    cases = []

    def obj(label, cls, args=None, attrs=None, calls=None, spelled=False):
        o = {"class": cls}
        if args is not None:
            o["args"] = args
        if attrs:
            o["attrs"] = attrs
        if calls:
            o["calls"] = calls
        cases.append(_case(label, "object", o, spelled))

    obj("GradingRGBM()", "GradingRGBM")
    obj("GradingRGBMSW()", "GradingRGBMSW")
    obj("GradingRGBMSW(start, width)", "GradingRGBMSW", [f64(0.2), f64(0.7)])
    obj("GradingControlPoint()", "GradingControlPoint")
    for spelled, values in ((False, DOUBLES), (True, SPELLED_DOUBLES)):
        for i, (name, v) in enumerate(values):
            rest = [f64(0.25 * (i % 5)), f64(-1.5), f64(2.0)]
            obj(f"GradingRGBM {name}", "GradingRGBM", [f64(v)] + rest[:3], spelled=spelled)
            obj(f"GradingRGBMSW {name}", "GradingRGBMSW",
                [f64(0.5), f64(v), f64(1.0), f64(1.0), f64(0.1), f64(v)], spelled=spelled)
    for spelled, values in ((False, FLOATS), (True, SPELLED_FLOATS)):
        for name, v in values:
            obj(f"GradingControlPoint {name}", "GradingControlPoint", [f64(v), f64(0.5)],
                spelled=spelled)
            obj(f"GradingBSplineCurve {name}", "GradingBSplineCurve",
                [[f64(0.0), f64(0.0), f64(v), f64(0.5), f64(1.0), f64(1.0)]],
                calls=[["setSlopes", [f64(0.0), f64(v), f64(1.0)]]], spelled=spelled)
    for style in GRADING_STYLES:
        obj(f"GradingPrimary({style})", "GradingPrimary", [_enum(style)])
        obj(f"GradingTone({style})", "GradingTone", [_enum(style)])
        obj(f"GradingHueCurve({style})", "GradingHueCurve", [_enum(style)])
    for size in (0, 1, 4):
        obj(f"GradingBSplineCurve({size})", "GradingBSplineCurve", [size])
    for name in HUE_CURVES:
        obj(f"GradingBSplineCurve(3, {name.upper()})", "GradingBSplineCurve",
            [3, _enum(name.upper())])
    for spline in ("B_SPLINE", "DIAGONAL_B_SPLINE", "HUE_HUE_B_SPLINE", "PERIODIC_1_B_SPLINE",
                   "PERIODIC_0_B_SPLINE", "HORIZONTAL1_B_SPLINE"):
        obj(f"GradingBSplineCurve(2, {spline})", "GradingBSplineCurve",
            {"size": 2, "splinetype": _enum(spline)})
    # Curves validate() refuses: points out of order, slopes of the wrong count.
    obj("GradingBSplineCurve out of order", "GradingBSplineCurve",
        [[f64(0.0), f64(0.0), f64(0.7), f64(0.5), f64(0.5), f64(1.0)]])
    return cases


# What the wheel says.

def _validate(obj, case_spec):
    """What obj.validate() raised, None, or "no validate" for a class without it.
    GradingPrimary validates for a style (PyGradingData.cpp:118 @ v2.5.2): its own, the first
    argument of its spec."""
    method = getattr(obj, "validate", None)
    if method is None:
        return "no validate"
    args = []
    if isinstance(obj, OCIO.GradingPrimary):
        args = [spec.value(case_spec["args"][0])]
    try:
        method(*args)
        return None
    except RAISED as exc:
        if not wheel_raised(exc):
            raise
        return exception_result(exc)


def _ctf(transform):
    group = OCIO.GroupTransform()
    group.appendTransform(transform)
    try:
        return {"text": group.write("Color Transform Format", OCIO.Config.CreateRaw())}
    except RAISED as exc:
        if not wheel_raised(exc):
            raise
        return {"exception": exception_result(exc)}


def texts(cases):
    """For each case, in order: {"label", "repr", "validate", and for a transform "ctf"}, or
    {"label", "exception"} where building it raised; repr and ctf are left out of a spelled
    case. Also the transforms the wheel built, in order, for the YAML."""
    out, built = [], []
    for case in cases:
        entry = {"label": case["label"]}
        try:
            if case["kind"] == "transform":
                obj = spec.transform(case["spec"])
            else:
                obj = spec.value_object(case["spec"])
        except RAISED as exc:
            if not wheel_raised(exc):
                raise
            entry["exception"] = exception_result(exc)
            out.append(entry)
            continue
        if not case["spelled"]:
            entry["repr"] = repr(obj)
        entry["validate"] = _validate(obj, case["spec"])
        if case["kind"] == "transform":
            built.append(obj)
            if not case["spelled"]:
                entry["ctf"] = _ctf(obj)
        out.append(entry)
    return out, built


def serialized(transforms):
    """Config.serialize() of a new config with a color space per transform, named t0000,
    t0001, ..., the transform its to_reference transform."""
    config = OCIO.Config()
    for i, transform in enumerate(transforms):
        cs = OCIO.ColorSpace(name=f"t{i:04d}")
        cs.setTransform(transform, OCIO.COLORSPACE_DIR_TO_REFERENCE)
        config.addColorSpace(cs)
    return config.serialize()


# The fixture's classes, each with its cases.
CLASSES = {
    "ExposureContrastTransform": ec_cases,
    "GradingPrimaryTransform": primary_cases,
    "GradingToneTransform": tone_cases,
    "GradingRGBCurveTransform": rgb_curve_cases,
    "GradingHueCurveTransform": hue_curve_cases,
    "values": value_cases,
}


def dumps(value):
    """JSON as the fixture files hold it: ASCII, one level of indent, a final newline."""
    return json.dumps(value, ensure_ascii=True, indent=1, allow_nan=False) + "\n"


def dumps_lines(values):
    """A JSON list as the fixture files hold the specs: ASCII, one compact item per line."""
    items = [json.dumps(v, ensure_ascii=True, separators=(",", ":"), allow_nan=False)
             for v in values]
    return "[\n" + ",\n".join(items) + "\n]\n"


def files():
    """{relative path: text} of the group, with the log messages of each class."""
    out = {}
    for name, make in CLASSES.items():
        with captured_log() as log:
            cases = make()
            results, built = texts(cases)
            yaml = serialized(built) if name != "values" else None
        out[f"{name}/spec.json"] = dumps_lines(cases)
        out[f"{name}/text.json"] = dumps(results)
        if yaml is not None:
            out[f"{name}/serialize.ocio"] = yaml
        out[f"{name}/log.txt"] = "".join(m if m.endswith("\n") else m + "\n" for m in log)
    return out
