# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for how a config reads YAML values of each type (card p3-oracle, chunk O3.3).

- yaml_scalars: spellings of a YAML value, each written into a config at a field OCIO reads
  as one type (bool, the profile version's ints, double, a list of doubles or floats, string,
  string list), the config loaded with Config.CreateFromStream, and the value read back: its
  bits, or what loading raised.

It is the black box of yaml-cpp 0.8.0's conversions as each wheel builds them: yaml-cpp's
`as<T>` reads numbers through std::stringstream with each platform's C++ library (MSVC's STL,
libstdc++), and OCIO's profile version goes through std::stoi. Each result carries the YAML
text that was loaded, so a port test loads exactly the same text.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import struct

import PyOpenColorIO as OCIO

from .checks import check_keys
from .commands import command
from .config_api import (REPORTED, LogCapture, RequestError, f64_bits, raised, text,
                         value_out)


def _f32_bits(value):
    """The bits of a C float the binding widened to a Python float (exact: widening keeps
    every value, and narrowing it back gives the same float)."""
    return struct.unpack("<I", struct.pack("<f", value))[0]


def _color_space(config):
    return config.getColorSpace("raw")


def _log_base(config):
    return _color_space(config).getTransform(OCIO.COLORSPACE_DIR_TO_REFERENCE).getBase()


# Field name -> (where its key goes, the key, what OCIO loads it as, how the command reads it
# back, how the value is written out).
#   where: "top" (a key of the config, column 0), "color_space" (a key of the color space
#   "raw", column 4), "log" (a key of a LogTransform, the color space's to_reference, column 6),
#   "version" (the value of ocio_profile_version, line 1).
FIELDS = {
    "strictparsing": ("top", "strictparsing", "bool (load(bool), as<bool>)",
                      lambda c: c.isStrictParsingEnabled(), bool),
    "isdata": ("color_space", "isdata", "bool (load(bool), as<bool>)",
               lambda c: _color_space(c).isData(), bool),
    "ocio_profile_version": ("version", "ocio_profile_version",
                             "string, split on '.', each part std::stoi",
                             lambda c: [c.getMajorVersion(), c.getMinorVersion()], list),
    "luma": ("top", "luma", "vector<double> (as<std::vector<double>>), 3 of them",
             lambda c: c.getDefaultLumaCoefs(), lambda v: [{"f64": f64_bits(x)} for x in v]),
    "base": ("log", "base", "double (load(double), as<double>), a scalar",
             _log_base, lambda v: {"f64": f64_bits(v)}),
    "allocationvars": ("color_space", "allocationvars", "vector<float> (as<std::vector<float>>)",
                       lambda c: _color_space(c).getAllocationVars(),
                       lambda v: [{"f32": _f32_bits(x)} for x in v]),
    "family": ("color_space", "family", "string (load(std::string), as<std::string>)",
               lambda c: _color_space(c).getFamily(), value_out),
    "description": ("color_space", "description",
                    "string (loadDescription: as<std::string>, trailing newlines removed)",
                    lambda c: _color_space(c).getDescription(), value_out),
    "aliases": ("color_space", "aliases",
                "string list (as<std::vector<std::string>>), then ColorSpace::addAlias",
                lambda c: _color_space(c).getAliases(), value_out),
}


def _yaml(field, spelling):
    """The config text with `spelling` as the field's value, as bytes."""
    where, key = FIELDS[field][:2]
    value = spelling.encode("utf-8") if isinstance(spelling, str) else spelling
    version = value if where == "version" else b"2"
    lines = [b"ocio_profile_version: " + version]
    if where == "top":
        lines.append(key.encode() + b": " + value)
    lines += [b"roles:", b"  default: raw", b"colorspaces:", b"  - !<ColorSpace>",
              b"    name: raw"]
    if where == "color_space":
        lines.append(b"    " + key.encode() + b": " + value)
    if where == "log":
        lines += [b"    to_reference: !<LogTransform>", b"      " + key.encode() + b": " + value]
    return b"\n".join(lines) + b"\n"


def _read(field, yaml):
    """Loads the config and reads the field back: {"value"}, or {"exception"} or
    {"undecodable"} (as config_api.call_out reports them), and the log."""
    read, write = FIELDS[field][3:]
    with LogCapture() as log:
        try:
            out = {"value": write(read(OCIO.Config.CreateFromStream(yaml)))}
        except UnicodeDecodeError as exc:
            out = {"undecodable": bytes(exc.object).hex()}
        except REPORTED as exc:
            out = {"exception": raised(exc)}
        out["log"] = log.take()
    return out


@command
def yaml_scalars(args, blobs):
    """Spellings of a YAML value, each read as a typed config field.

    args:
      cases     [{"field": name, "spellings": [spelling, ...]}, ...]
                field: a key of FIELDS:
                  strictparsing         top-level bool -> isStrictParsingEnabled()
                  isdata                the color space's bool -> isData()
                  ocio_profile_version  the version's text, split on '.' and each part read by
                                        std::stoi -> [getMajorVersion(), getMinorVersion()]
                  luma                  top-level list of 3 doubles -> getDefaultLumaCoefs()
                  base                  a LogTransform's double -> getBase()
                  allocationvars        the color space's list of floats -> getAllocationVars()
                  family                the color space's string -> getFamily()
                  description           the color space's string, through loadDescription
                                        -> getDescription()
                  aliases               the color space's string list -> getAliases()
                spelling: the YAML text that follows "<key>: " (text, or {"bytes": hex}),
                such as "0x1A", ".inf", "'quoted'", "[1, 2, 3]". It may span lines: a key is
                at column 0 (top-level), 4 (the color space's) or 6 (the LogTransform's), so a
                continuation line needs more indentation than that.
      The config is
        ocio_profile_version: <the spelling, or 2>
        [<top-level key>: <spelling>]
        roles:
          default: raw
        colorspaces:
          - !<ColorSpace>
            name: raw
            [<color space key>: <spelling>]
            [to_reference: !<LogTransform>
              base: <spelling>]
      with a line feed after each line; each result gives the exact text.
    result:
      cases     per case, in order, per spelling: {"yaml": the config's text, {"bytes": hex},
                "value", "log"}, or {"yaml", "exception" or "undecodable", "log"} (as
                config_api reports them). The value: a bool; [major, minor]; {"f64": bits} or a
                list of them (doubles); a list of {"f32": bits} (floats); a string,
                {"bytes": hex}; a list of strings (each as config_api writes a list's elements).
                The log: [{"bytes": hex}], a message each (config_api.LogCapture)
    blobs: none

    An unknown key or field, or a spelling that isn't text or {"bytes": hex}, is refused.
    """
    check_keys("yaml_scalars", args, {"cases"})
    cases = args.get("cases", [])
    if not isinstance(cases, list):
        raise RequestError(f"cases must be a list, not {cases!r}")
    out = []
    for c, case in enumerate(cases):
        check_keys(f"cases[{c}]", case, {"field", "spellings"})
        field = case.get("field")
        if field not in FIELDS:
            raise RequestError(f"cases[{c}]: unknown field {field!r}; one of {sorted(FIELDS)}")
        spellings = case.get("spellings", [])
        if not isinstance(spellings, list):
            raise RequestError(f"cases[{c}].spellings must be a list")
        entries = []
        for s, spelling in enumerate(spellings):
            yaml = _yaml(field, text(f"cases[{c}].spellings[{s}]", spelling))
            entries.append({"yaml": {"bytes": yaml.hex()}, **_read(field, yaml)})
        out.append(entries)
    return {"cases": out}, []
