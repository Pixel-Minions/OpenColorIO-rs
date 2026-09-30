# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle commands for text: number parsing and formatting, and YAML emission.

They feed text to the real library through its public API and report what comes back.
Nothing here computes an expected value.
"""

import itertools
import os
import struct
import tempfile

import numpy as np
import PyOpenColorIO as OCIO

from .commands import command, exception_result

_file_counter = itertools.count()


def _check_token(token, max_len=63):
    if not token or any(c.isspace() for c in token) or (max_len and len(token) > max_len):
        raise ValueError(f"token {token!r} must be non-space characters, at most {max_len}")


def _load_file_transform(path):
    """The FileTransform's ops, unoptimized, as a GroupTransform."""
    config = OCIO.Config.CreateRaw()
    transform = OCIO.FileTransform(src=path, interpolation=OCIO.INTERP_LINEAR)
    processor = config.getProcessor(transform).getOptimizedProcessor(OCIO.OPTIMIZATION_NONE)
    return processor.createGroupTransform()


def _unique_path(directory, extension):
    # A fresh name for every file: OCIO caches file reads by path.
    return os.path.join(directory, f"probe{next(_file_counter)}.{extension}")


def _spi1d_lut(directory, tokens):
    path = _unique_path(directory, "spi1d")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(f"Version 1\nFrom 0.0 1.0\nLength {len(tokens)}\nComponents 1\n{{\n")
        for token in tokens:
            f.write(token + "\n")
        f.write("}\n")
    try:
        group = _load_file_transform(path)
    except OCIO.Exception as exc:
        return {"exception": exception_result(exc)}
    luts = [t for t in group if isinstance(t, OCIO.Lut1DTransform)]
    if len(luts) != 1:
        return {"transforms": [type(t).__name__ for t in group]}
    data = np.ascontiguousarray(luts[0].getData(), dtype=np.float32)
    bits = np.frombuffer(data.tobytes(), dtype=np.uint32)[0::3]
    return {"bits": [int(b) for b in bits]}


@command
def spi1d_values(args, blobs):
    """Reads tokens as the values of a 1-component .spi1d LUT through a FileTransform.

    The reader copies each whitespace-separated token with sscanf("%63s") into a zero-filled
    64-byte buffer and calls NumberUtils::from_chars(float) on [buffer, buffer + 64)
    (FileFormatSpi1D.cpp:220-262); a token it cannot convert fails the whole file.

    args: {"tokens": [str], "separately": bool}
      separately=false: one LUT holding all tokens (at least 2);
      separately=true: one LUT per token, followed by the entry "0".
    result: one entry per LUT: {"bits": [u32 per entry]} or {"exception": {...}}
    """
    tokens = args["tokens"]
    for token in tokens:
        _check_token(token)
    with tempfile.TemporaryDirectory() as directory:
        if args.get("separately"):
            return [_spi1d_lut(directory, [t, "0"]) for t in tokens], []
        return [_spi1d_lut(directory, tokens)], []


def _clf_matrices(directory, tokens):
    """One CLF file with a 3x3 Matrix per 9 tokens; the bits of every entry, in order."""
    path = _unique_path(directory, "clf")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write('<?xml version="1.0" encoding="UTF-8"?>\n')
        f.write('<ProcessList compCLFversion="3" id="probe">\n')
        for i in range(0, len(tokens), 9):
            f.write('    <Matrix inBitDepth="32f" outBitDepth="32f">\n')
            f.write('        <Array dim="3 3">\n')
            f.write(" ".join(tokens[i:i + 9]) + "\n")
            f.write("        </Array>\n    </Matrix>\n")
        f.write("</ProcessList>\n")
    try:
        group = _load_file_transform(path)
    except OCIO.Exception as exc:
        return {"exception": exception_result(exc)}
    matrices = [t for t in group if isinstance(t, OCIO.MatrixTransform)]
    if len(matrices) != len(tokens) // 9:
        return {"transforms": [type(t).__name__ for t in group]}
    bits = []
    for matrix in matrices:
        m44 = matrix.getMatrix()
        for i in (0, 1, 2, 4, 5, 6, 8, 9, 10):
            bits.append(struct.unpack("<Q", struct.pack("<d", m44[i]))[0])
    return {"bits": bits}


@command
def clf_matrix_values(args, blobs):
    """Reads tokens as the 3x3 Arrays of CLF Matrix ops through a FileTransform.

    Each token reaches XMLReaderUtils ParseNumber<double> and NumberUtils::from_chars(double)
    (CTFReaderHelper.cpp:355-380, XMLReaderUtils.h:147-205). Tokens may be of any length.

    args: {"tokens": [str], "separately": bool}
      separately=false: one file with a Matrix per 9 tokens (the count must be a multiple
                        of 9), and one result for it;
      separately=true: one file per token, as the first entry of a Matrix followed by eight
                       "0" entries.
    result: one entry per file: {"bits": [u64 per entry]} or {"exception": {...}}
    """
    tokens = args["tokens"]
    for token in tokens:
        _check_token(token, max_len=None)
    with tempfile.TemporaryDirectory() as directory:
        if args.get("separately"):
            return [_clf_matrices(directory, [t] + ["0"] * 8) for t in tokens], []
        if len(tokens) % 9:
            raise ValueError("clf_matrix_values needs a multiple of 9 tokens")
        return [_clf_matrices(directory, tokens)], []


# YAML emission (S1): configs built through the API, then Config.serialize().


def _f64(bits):
    """The double with IEEE bits `bits`; NaN payloads pass unchanged."""
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def _f32(bits):
    """The float with IEEE bits `bits`, as the Python float the binding narrows back to it."""
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def _bits64(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def _bits32(x):
    """The bits of the float nearest to `x`."""
    return struct.unpack("<I", struct.pack("<f", x))[0]


def _build_colorspace(spec):
    cs = OCIO.ColorSpace(name=spec["name"])
    if "family" in spec:
        cs.setFamily(spec["family"])
    if "description" in spec:
        cs.setDescription(spec["description"])
    if "encoding" in spec:
        cs.setEncoding(spec["encoding"])
    for alias in spec.get("aliases", []):
        cs.addAlias(alias)
    for category in spec.get("categories", []):
        cs.addCategory(category)
    for key, value in spec.get("interchange", []):
        cs.setInterchangeAttribute(key, value)
    if "allocation_vars_bits" in spec:
        cs.setAllocationVars([_f32(b) for b in spec["allocation_vars_bits"]])
    if "matrices_bits" in spec:
        group = OCIO.GroupTransform()
        for bits in spec["matrices_bits"]:
            values = [_f64(b) for b in bits]
            group.appendTransform(OCIO.MatrixTransform(matrix=values[:16], offset=values[16:]))
        cs.setTransform(group, OCIO.COLORSPACE_DIR_TO_REFERENCE)
    if "allocations_bits" in spec:
        group = OCIO.GroupTransform()
        for bits in spec["allocations_bits"]:
            transform = OCIO.AllocationTransform()
            transform.setAllocation(OCIO.ALLOCATION_LG2)
            transform.setVars([_f32(b) for b in bits])
            group.appendTransform(transform)
        cs.setTransform(group, OCIO.COLORSPACE_DIR_FROM_REFERENCE)
    return cs


def _with_bytes(value):
    """The spec with each {"hex": h} replaced by bytes.fromhex(h): the binding passes
    bytes to std::string and const char * parameters unchanged, like a C++ caller."""
    if isinstance(value, dict):
        if set(value) == {"hex"}:
            return bytes.fromhex(value["hex"])
        return {k: _with_bytes(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_with_bytes(v) for v in value]
    return value


def build_config(spec):
    """A config built through the Python API from a spec (see serialize_built_config)."""
    spec = _with_bytes(spec)
    config = OCIO.Config()
    if "name" in spec:
        config.setName(spec["name"])
    if "description" in spec:
        config.setDescription(spec["description"])
    for name, value in spec.get("environment", []):
        config.addEnvironmentVar(name, value)
    for path in spec.get("search_paths", []):
        config.addSearchPath(path)
    if "family_separator" in spec:
        config.setFamilySeparator(spec["family_separator"])
    if "luma_bits" in spec:
        config.setDefaultLumaCoefs([_f64(b) for b in spec["luma_bits"]])
    for cs in spec.get("colorspaces", []):
        config.addColorSpace(_build_colorspace(cs))
    for role, colorspace in spec.get("roles", []):
        config.setRole(role, colorspace)
    for display, views in spec.get("displays", []):
        for view in views:
            config.addDisplayView(display=display, view=view["name"], viewTransform="",
                                  displayColorSpaceName=view["colorspace"], looks="",
                                  ruleName="", description=view.get("description", ""))
    if "active_displays" in spec:
        config.setActiveDisplays(spec["active_displays"])
    if "active_views" in spec:
        config.setActiveViews(spec["active_views"])
    if "inactive_colorspaces" in spec:
        config.setInactiveColorSpaces(spec["inactive_colorspaces"])
    if "file_rules" in spec:
        rules = OCIO.FileRules()
        for i, rule in enumerate(spec["file_rules"]):
            rules.insertRule(i, rule["name"], rule["colorspace"], rule["pattern"],
                             rule["extension"])
            for key, value in rule.get("custom", []):
                rules.setCustomKey(i, key, value)
        config.setFileRules(rules)
    if "viewing_rules" in spec:
        rules = OCIO.ViewingRules()
        for i, rule in enumerate(spec["viewing_rules"]):
            rules.insertRule(i, rule["name"])
            for colorspace in rule.get("colorspaces", []):
                rules.addColorSpace(i, colorspace)
            for key, value in rule.get("custom", []):
                rules.setCustomKey(i, key, value)
        config.setViewingRules(rules)
    return config


@command
def serialize_built_config(args, blobs):
    """Config.serialize() of a config built through the Python API from a spec.

    Unlike config_serialize, which loads a config from YAML, the values reach the config
    through its setters, so the writer sees exactly them. Numbers are IEEE bit patterns, so
    NaNs, signed zeros and subnormals pass exactly.

    args: {"spec": {...}}, every key optional:
      name, description: str
      environment: [[name, default]]; search_paths: [str]; family_separator: one character
      luma_bits: [u64, u64, u64]
      colorspaces: [{name, family, description, encoding: str; aliases, categories: [str];
                     interchange: [[key, value]]; allocation_vars_bits: [u32] (2 or 3);
                     matrices_bits: [[u64] (16 matrix values, then 4 offsets)], written as
                       a to_reference GroupTransform of MatrixTransforms;
                     allocations_bits: [[u32] (2 or 3)], written as a from_reference
                       GroupTransform of lg2 AllocationTransforms}]
      roles: [[role, colorspace]]
      displays: [[display, [{name, colorspace, description}]]]
      active_displays, active_views, inactive_colorspaces: comma-separated str
      file_rules: [{name, colorspace, pattern, extension: str; custom: [[key, value]]}],
        inserted before the default rule
      viewing_rules: [{name: str; colorspaces: [str]; custom: [[key, value]]}]
    Any string may be given as {"hex": "..."}: those bytes, which need not be UTF-8.
    result: {"bytes": n} with the UTF-8 text as blob 0, or {"exception": {...}}
    """
    try:
        text = build_config(args["spec"]).serialize().encode("utf-8")
    except OCIO.Exception as exc:
        return {"exception": exception_result(exc)}, []
    return {"bytes": len(text)}, [text]


@command
def serialize_built_config_to_file(args, blobs):
    """Config.serialize(fileName) and Config.getCacheID() of a config built from a spec (as
    serialize_built_config), for strings that are not UTF-8.

    serialize() returns a Python str, so it raises on output that is not UTF-8. The
    serialize(fileName) binding writes to a std::ofstream opened in text mode
    (PyConfig.cpp:243-249), so on Windows every "\\n" becomes "\\r\\n"; getCacheID() hashes
    the serialized text itself (Config.cpp:5264-5271).

    args: {"spec": {...}}
    result: {"cache_id": str, "bytes": n} with the file's bytes as blob 0, or
            {"exception": {...}}
    """
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, "config.ocio")
        try:
            config = build_config(args["spec"])
            config.serialize(path)
            cache_id = config.getCacheID()
        except OCIO.Exception as exc:
            return {"exception": exception_result(exc)}, []
        with open(path, "rb") as f:
            data = f.read()
    return {"cache_id": cache_id, "bytes": len(data)}, [data]


@command
def stream_reprs(args, blobs):
    """repr() of transforms holding the given numbers: the C++ operator<< into a fresh
    std::ostringstream (PyUtils.h defRepr), so the platform's iostream float output, NaN and
    infinity spellings included. MatrixTransform sets precision 16
    (MatrixTransform.cpp:340-364); ExponentTransform and AllocationTransform keep the
    default 6.

    args: {"doubles_bits": [u64], "floats_bits": [u32]}
      doubles (a multiple of 20): one MatrixTransform per 20 (16 matrix values, then 4
        offsets), and one ExponentTransform per 4 (setValue, which does not validate);
      floats (a multiple of 3): one lg2 AllocationTransform per 3 (setVars).
    result: {"matrix": [str], "exponent": [str], "allocation": [str]}
    """
    doubles = [_f64(b) for b in args["doubles_bits"]]
    floats = [_f32(b) for b in args["floats_bits"]]
    if len(doubles) % 20 or len(floats) % 3:
        raise ValueError("stream_reprs needs a multiple of 20 doubles and of 3 floats")
    matrix, exponent, allocation = [], [], []
    for i in range(0, len(doubles), 20):
        transform = OCIO.MatrixTransform(matrix=doubles[i:i + 16], offset=doubles[i + 16:i + 20])
        matrix.append(repr(transform))
    for i in range(0, len(doubles), 4):
        transform = OCIO.ExponentTransform()
        transform.setValue(doubles[i:i + 4])
        exponent.append(repr(transform))
    for i in range(0, len(floats), 3):
        transform = OCIO.AllocationTransform()
        transform.setAllocation(OCIO.ALLOCATION_LG2)
        transform.setVars(floats[i:i + 3])
        allocation.append(repr(transform))
    return {"matrix": matrix, "exponent": exponent, "allocation": allocation}, []


# The fixture cases of the `yaml_emitter` regen group: inputs only; the expected text is
# what serialize() returns for them.

_BLOCK_STRINGS = [
    "plain", " lead", "trail ", "a: b", "a #b", "a#b", "e#", "x#", "x #", "#", "-dash", "- x", "-",
    "--", "? q", "?", "?x", ": colon", ":", ":x", "k:v", "x: ", "x :y", ",comma", "a,b", "[br", "a[b",
    "]br", "{br", "}br", "#hash", "&amp", "x&y", "*star", "!bang", "a!b", "|pipe", ">gt", "'sq",
    "a'b", "\"dq", "a\"b", "%pc", "@at", "`bt", "=", "<<", "null", "Null", "NULL", "nULL", "~", "~x",
    "true", "False", "yes", "No", "on", "OFF", "y", "n", "1.5", "-2", "0x10", "1e3", ".inf",
    "-.inf", ".nan", "0o17", "a\tb", "\ttab", "tab\t", "a\nb", "a\\b", "\\", "a\bb", "a\fb", "a\rb",
    "a\x00b", "a\x01b", "a\x1fb", "a\x7fb", "a  b", "...", "---", "--- x", "... x", "a - b", "a -b",
    "", "  ", " ",
]

_NON_ASCII_STRINGS = [
    "caf\u00e9", "\u00e9t\u00e9", "\u65e5\u672c\u8a9e", "\U0001F600", "a\U0001F600b", "\u00a0lead",
    "trail\u00a0", "a\u0080b", "a\u0085b", "a\u009fb", "a\u00a0b", "a\u00adb", "a\u2028b",
    "a\u2029b", "a\ufeffb", "\ufeff", "\ufffd", "a\ue000b", "\U0010ffff", "\u0391\u03b2\u03b3",
    "\u0645\u0631\u062d\u0628\u0627", "a: \u00e9", "\u00e9: a", "- \u00e9", "\u00e9 #x", "\u00e9,\u00e9",
]

_KEY_STRINGS = [
    "plain", "Plain", "a: b", "a #b", "-dash", "? q", "[br", "{br", "#hash", "&amp", "*star",
    "!bang", "|pipe", ">gt", "'sq", "\"dq", "@at", "`bt", "null", "~", "true", "1.5", "a\tb",
    "a\nb", "a\\b", "  ", "caf\u00e9", "\U0001F600", "a,b", "x" * 1024, "y" * 1025,
    "\u00e9" * 512, "\u00e9" * 513,
]

# Descriptions that read back as written (a literal block whose first line starts with a
# space, or holding a CR, does not; see docs/spikes/s1-wp05.md).
_DESCRIPTIONS = [
    "one line", "a\nb", "a\nb\n", "a\nb\n\n\n", "\na", "\n\na\n", "a\n\n\nb", "a \nb",
    "a\n \nb", "\t\nx", "x\n\ty", "tab\there\nx", "a\n  b\n    c", "- item\n- item2", "k: v\nx",
    "# not a comment\nx", "x\n#hash", "x\n---\ny", "x\n...\ny", "x\n", "\n", "\n\n",
    "trailing space \n", " \n", "a\\nb", "caf\u00e9\n\u65e5\u672c", "a\u2028b\nc", "x\n\u00a0nbsp",
    "x\n\x01ctl", "k: v", "# c", "line with \"quotes\"\nand 'single'", "x" * 2000 + "\ny",
]

_F64_SPECIAL_BITS = [
    0x7FF0000000000000, 0xFFF0000000000000, 0x7FF8000000000000, 0xFFF8000000000000,
    0x7FF0000000000001, 0x7FF8000000000123, 0x0000000000000000, 0x8000000000000000,
    0x0000000000000001, 0x000FFFFFFFFFFFFF, 0x0010000000000000, 0x7FEFFFFFFFFFFFFF,
    0x8000000000000001, 0xFFEFFFFFFFFFFFFF,
]

_F64_EDGES = [
    123456789012344.5, 123456789012345.5, 12345678901234.25, 12345678901234.75,
    1234567890123455.0, 1234567890123465.0, 999999999999999.5, 999999999999999.4,
    99999999999999.95, 1e15, 1e16, 1e17, 123456789012345678.0, 1e-5, 1e-4,
    9.99999999999999e-5, 0.000099999999999999995, 0.1, 0.2, 0.3, 1 / 3, 2 / 3, 0.5, 1.0, -1.0,
    3.141592653589793, 2.718281828459045, 0.2126, 0.7152, 0.0722, 1e22, 1e23,
    9007199254740993.0, 4503599627370495.5, 0.30000000000000004, 1.0000000000000002,
    0.9999999999999999, 100.0, 1e100, 1e-100, 1e300, 1e-300, 123.456, -0.001234,
]

_F32_SPECIAL_BITS = [
    0x7F800000, 0xFF800000, 0x7FC00000, 0xFFC00000, 0x7F800001, 0x00000000, 0x80000000,
    0x00000001, 0x007FFFFF, 0x00800000, 0x7F7FFFFF, 0x80000001, 0xFF7FFFFF,
]

_F32_EDGES = [
    1234568.5, 1234567.5, 123456.75, 123456.25, 12345675.0, 12345665.0, 9999999.0, 9999999.5,
    16777216.0, 16777217.0, 1e-5, 1e-4, 0.1, 0.2, 1 / 3, 3.4028235e38, 1.1754944e-38, 1e-45,
    0.2126, 0.5, 1.0, 100.0, 1e10, 1e-10, 3.1415927, 1e7, 1e6, 999999.95, 0.099999994, 1.0000001,
    0.99999994,
]


def _chunks(values, size, pad):
    values = list(values)
    while len(values) % size:
        values.append(pad)
    return [values[i:i + size] for i in range(0, len(values), size)]


def emitter_cases():
    """The spec of each `yaml_emitter` fixture case, by name."""
    return {
        "empty": {},
        "block_strings": {
            "name": "a: config #1",
            "family_separator": " ",
            "search_paths": _BLOCK_STRINGS[:12],
            "colorspaces": [
                {"name": f"cs{i}", "family": s, "encoding": s}
                for i, s in enumerate(_BLOCK_STRINGS)
            ],
        },
        "flow_strings": {
            "family_separator": "\"",
            "colorspaces": [
                # (An alias may not hold the context variable tokens % and $.)
                {"name": "cs0", "aliases": [s for s in _BLOCK_STRINGS if s.strip() and "%" not in s],
                 "categories": ["plain", "a: b", "a,b", "[x]", "{x}", "#x", "x#", "caf\u00e9"]},
            ],
            "displays": [["d", [{"name": s, "colorspace": s} for s in _BLOCK_STRINGS if s.strip()]]],
            "active_displays": "d, a: b, x#y",
            "active_views": "a: b, [x], {y}, ?z",
            "inactive_colorspaces": "cs0, a#b, !x",
        },
        "keys": {
            "family_separator": "\\",
            "search_paths": ["only one: path"],
            "environment": [[k, v] for k, v in zip(_KEY_STRINGS, [""] + _KEY_STRINGS)],
            "colorspaces": [{"name": "cs0"}],
            "roles": [[k, "cs0"] for k in _KEY_STRINGS],
            "displays": [[k, [{"name": "v", "colorspace": "cs0"}]] for k in _KEY_STRINGS],
        },
        "long_strings": {
            "family_separator": "#",
            "name": "n" * 1030,
            "description": "d" * 3000,
            "colorspaces": [
                {"name": "c" * 1100, "family": "f" * 5000, "aliases": ["a" * 3000],
                 "description": "x" * 1500 + "\n" + "y" * 1500},
            ],
            "displays": [["z" * 1025, [{"name": "v" * 1500, "colorspace": "c" * 1100,
                                        "description": "w" * 1200}]]],
        },
        "descriptions": {
            "family_separator": "a",
            "description": "Config description\n\nSecond paragraph: with a colon.\n",
            "colorspaces": [
                {"name": f"cs{i}", "description": d, "interchange": [["amf_transform_ids", d]]}
                for i, d in enumerate(_DESCRIPTIONS)
            ],
            "displays": [["d", [{"name": f"v{i}", "colorspace": "cs0", "description": d}
                                for i, d in enumerate(_DESCRIPTIONS)]]],
        },
        "non_ascii": {
            "family_separator": "~",
            "name": "caf\u00e9 \u65e5\u672c",
            "colorspaces": [
                {"name": f"cs{i}", "family": s, "aliases": [f"alias{i} {s}"],
                 "interchange": [["icc_profile_name", s]]}
                for i, s in enumerate(_NON_ASCII_STRINGS)
            ],
            "roles": [[f"role{i} {s}", "cs0"] for i, s in enumerate(_NON_ASCII_STRINGS)],
        },
        "numbers": {
            "luma_bits": [_F64_SPECIAL_BITS[2], _F64_SPECIAL_BITS[0], _bits64(0.0722)],
            "colorspaces": [
                {"name": f"vars{i}", "allocation_vars_bits": bits}
                for i, bits in enumerate(_chunks(
                    _F32_SPECIAL_BITS + [_bits32(x) for x in _F32_EDGES], 3, _bits32(0.5)))
            ] + [
                {"name": "matrices",
                 "matrices_bits": _chunks(
                     _F64_SPECIAL_BITS + [_bits64(x) for x in _F64_EDGES], 20, _bits64(0.5)),
                 "allocations_bits": _chunks(
                     _F32_SPECIAL_BITS + [_bits32(x) for x in _F32_EDGES], 3, _bits32(0.5))},
                {"name": "empty group", "matrices_bits": []},
            ],
        },
    }


# Cases whose serialize() text can't be read back into the strings the config held: a flow
# map's long key is written `{ ?key: v}`, which reads back as the key "?key"; quoted and
# literal scalars replace noncharacters with U+FFFD; a literal block whose first line
# starts with a space does not parse, and one with a CR reads back as a plain line break.
# Each case is a spec with placeholders, the same config with the placeholders replaced by
# the strings; the test reads the placeholder text and writes it with the strings put back.

def _placeholder(i):
    return f"phxq{i:04d}"


def substitute(value, substitutions):
    """`value` with every string equal to a placeholder replaced by its string."""
    if isinstance(value, dict):
        return {k: substitute(v, substitutions) for k, v in value.items()}
    if isinstance(value, list):
        return [substitute(v, substitutions) for v in value]
    return substitutions.get(value, value) if isinstance(value, str) else value


_LONG_KEYS = [
    "k" * 1024, "k" * 1025, "\u00e9" * 512, "\u00e9" * 513, "a: b" * 300, "- " + "x" * 1100,
]

_NONCHARACTERS = [
    "\ufdd0", "\ufdef", "\ufffe", "\uffff", "\U0001fffe", "\U0001ffff", "\U0010fffe",
    "\U0010ffff",
]

_UNREADABLE_LITERALS = [
    " a\nb", "  a\n b", "\ta\nb", "a\r\nb", "a\rb\nc", "\r\na", "a\n\r", "x\n\u2028y\r",
]


def substitution_cases():
    """The `yaml_emitter` cases that need substitution: {"spec", "substitutions"} by name."""
    def case(spec_of, strings):
        subs = [[_placeholder(i), s] for i, s in enumerate(strings)]
        return {"spec": spec_of([p for p, _ in subs]), "substitutions": subs}

    def rules(keys):
        return {
            "colorspaces": [{"name": "cs0"}],
            "file_rules": [
                {"name": f"r{i}", "colorspace": "cs0", "pattern": "*", "extension": "*",
                 "custom": [[k, "v"]]}
                for i, k in enumerate(keys)
            ],
            "viewing_rules": [
                {"name": f"vr{i}", "colorspaces": ["cs0"], "custom": [[k, "v"]]}
                for i, k in enumerate(keys)
            ],
        }

    def noncharacters(slots):
        n = len(_NONCHARACTERS)
        family, quoted, literal, view = slots[:n], slots[n:2 * n], slots[2 * n:3 * n], slots[3 * n:]
        return {
            "colorspaces": [
                {"name": f"cs{i}", "family": family[i], "encoding": quoted[i],
                 "description": literal[i]}
                for i in range(n)
            ],
            "displays": [["d", [{"name": f"v{i}", "colorspace": "cs0", "description": view[i]}
                                for i in range(n)]]],
        }

    def literals(slots):
        n = len(_UNREADABLE_LITERALS)
        return {
            "description": slots[0],
            "colorspaces": [
                {"name": f"cs{i}", "description": slots[1 + i],
                 "interchange": [["amf_transform_ids", slots[1 + n + i]]]}
                for i in range(n)
            ],
        }

    return {
        # A flow map's long key, `{ ?key: v}` (emitter.cpp:410-439): FileRules and ViewingRules
        # custom keys, one per rule, since a rule's keys are sorted.
        "subst_flow_long_keys": case(rules, _LONG_KEYS),
        # Noncharacters: raw in a plain scalar (family), U+FFFD in double quotes (encoding,
        # quoted by its leading "- "; view descriptions in a flow map) and in literal blocks.
        "subst_noncharacters": case(noncharacters, (
            [f"a{c}" for c in _NONCHARACTERS] + [f"- {c}" for c in _NONCHARACTERS]
            + [f"a\n{c}" for c in _NONCHARACTERS] + [f"a\n{c}" for c in _NONCHARACTERS])),
        # Literal blocks that no reader reads back.
        "subst_literal_blocks": case(literals, (
            _UNREADABLE_LITERALS[:1] + _UNREADABLE_LITERALS + _UNREADABLE_LITERALS)),
    }
