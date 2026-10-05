# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for a config's API (card p3-oracle, chunk O3.1).

- config_calls: a config made from a source (raw, Config(), YAML text, a file, a built-in
  config or $OCIO), in an environment and a directory of files the request gives; then calls
  in order on the config and on the objects the calls return or create, each call's result or
  exception and log; and `dump`, everything an object holds through its getters.

The call engine here (sources, values, calls, dumps, the environment) is shared with the other
Phase 3 commands, which import it: context_calls (context_api.py) and config_processor
(config_processors.py).

Like every oracle command, it reports what the library does and never computes expected
values.

The environment. A config reads OCIO_ACTIVE_DISPLAYS, OCIO_ACTIVE_VIEWS and
OCIO_INACTIVE_COLORSPACES when it is created, and a context without an `environment:` section
loads every variable of the process into its string variables and its cache ID. So a request
runs in an environment that holds exactly the variables it gives, and nothing of the oracle's
own: the results then depend on the request alone (the oracle's cache keys on the request), and
the variables of the machine that runs the oracle, secrets included, never reach a result. The
command unsets every variable the process started with, sets the request's, runs, and restores
them. OCIO reads OCIO_LOGGING_LEVEL once per process, which a request can't change (see
logging_environment), so the command refuses it.

Strings. The binding passes a Python str to the library as UTF-8 and a Python bytes as is,
so a request gives a string as JSON text or as {"bytes": hex}. The binding decodes what the
library returns as UTF-8: a result string comes back as JSON text, which holds it exactly. A
result the binding can't decode raises UnicodeDecodeError, which the command reports with the
bytes it couldn't decode (the whole string), in hex. So does an exception whose message the
binding can't decode: the message's bytes then come back, and its type (Exception or
ExceptionMissingFile) doesn't.
"""

import contextlib
import copy
import os
import struct
import tempfile

import PyOpenColorIO as OCIO

from . import spec
from .checks import check_keys
from .commands import captured_log, command, exception_result

# What the library raises: OCIO's exceptions (in PyOpenColorIO, ExceptionMissingFile doesn't
# derive from OCIO.Exception); pybind11's translations of the C++ standard exceptions
# (RuntimeError for std::exception, such as std::regex_error, ValueError, IndexError,
# OverflowError); and UnicodeDecodeError, a result string the binding can't decode. A
# TypeError is pybind11 finding no overload for the arguments: a bug in the request, so it
# fails the command instead.
REPORTED = (OCIO.Exception, OCIO.ExceptionMissingFile, RuntimeError, ValueError, IndexError,
            OverflowError, KeyError, StopIteration)



class RequestError(Exception):
    """A request the command can't run exactly. Not in REPORTED, so it fails the command."""


# Variables a request can't set: OCIO reads them once per process.
FIXED_VARIABLES = {"OCIO_LOGGING_LEVEL"}

# Module-level functions a call may reach on "OCIO". The others change the process for the
# commands after this one (SetCurrentConfig, SetLoggingLevel, SetEnvVariable, ...), or aren't
# the config API.
MODULE_FUNCTIONS = {
    "GetVersion", "GetVersionHex", "GetEnvVariable", "IsEnvVariablePresent",
    "ResolveConfigPath", "ClearAllCaches",
    "CombineTransformDirections", "GetInverseTransformDirection", "BitDepthIsFloat",
    "BitDepthToInt",
    # The enums' string conversions (ParseUtils.cpp), each *ToString and *FromString.
    *(f"{name}{way}" for name in ("Allocation", "BitDepth", "Bool", "CDLStyle", "EnvironmentMode",
                                  "ExposureContrastStyle", "FixedFunctionStyle", "GpuLanguage",
                                  "GradingStyle", "Interpolation", "LoggingLevel", "NegativeStyle",
                                  "RangeStyle", "TransformDirection")
      for way in ("ToString", "FromString")),
}

# Dunder methods a call may name. Everything else starting with "_" is refused.
DUNDER_CALLS = {"__str__", "__repr__", "__len__", "__getitem__", "__setitem__",
                "__contains__", "__iter__"}


# ---------------------------------------------------------------------------------------------
# Values: request -> Python


def text(what, value):
    """A string the request gives: JSON text (the binding passes it as UTF-8) or
    {"bytes": hex} (passed as is)."""
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and set(value) == {"bytes"} and isinstance(value["bytes"], str):
        return bytes.fromhex(value["bytes"])
    raise RequestError(f"{what} must be a string or {{\"bytes\": hex}}, not {value!r}")


def value_in(v, objects):
    """A call argument:
    - {"enum": "NAME"}: the PyOpenColorIO enum value of that name;
    - {"transform": <spec>}: a transform (spec.transform);
    - {"f64": bits}: the float with those bits;
    - {"bytes": hex}: those bytes;
    - {"ref": name}: an object stored by an earlier call's "as";
    - {"map": [[key, value], ...]}: a dict (for the Context constructor's stringVars);
    - lists and other JSON values as themselves."""
    if isinstance(v, dict):
        keys = set(v)
        if keys == {"enum"}:
            member = getattr(OCIO, v["enum"], None) if isinstance(v["enum"], str) else None
            if member is None or not hasattr(type(member), "__members__"):
                raise RequestError(f"unknown enum value {v['enum']!r}")
            return member
        if keys == {"transform"}:
            return spec.transform(v["transform"])
        if keys == {"f64"}:
            return spec.f64_from_bits(v["f64"])
        if keys == {"bytes"}:
            return text("a bytes value", v)
        if keys == {"ref"}:
            if v["ref"] not in objects:
                raise RequestError(f"no object is stored as {v['ref']!r}")
            return objects[v["ref"]]
        if keys == {"map"}:
            return {value_in(k, objects): value_in(x, objects) for k, x in v["map"]}
        raise RequestError(f"unknown value spec {v!r}")
    if isinstance(v, list):
        return [value_in(x, objects) for x in v]
    return v


# ---------------------------------------------------------------------------------------------
# Values: Python -> result


def f64_bits(value):
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def _own_repr(obj):
    """Whether the binding gives obj's class a repr() of its own (upstream's operator<<),
    rather than pybind11's default, which prints an address."""
    for cls in type(obj).__mro__:
        if cls.__name__ in ("pybind11_object", "object"):
            return False
        if "__repr__" in cls.__dict__:
            return True
    return False


def value_out(v):
    """A JSON form of what the library returned, exact:
    - None, bools, ints and strings as themselves; floats (C doubles, or C floats the binding
      widened) as {"f64": bits}; bytes as {"bytes": hex}; enums as {"enum": name};
    - lists, tuples and the binding's iterators as lists; dicts as {"dict": [[key, value]]};
    - a transform as {"class", "repr"}; any other object as {"class"}, with "repr" where the
      binding gives its class one (upstream's operator<<), or {"exception": ...} where the
      binding can't decode it."""
    if v is None or isinstance(v, (bool, int, str)):
        return v
    if isinstance(v, float):
        return {"f64": f64_bits(v)}
    if isinstance(v, bytes):
        return {"bytes": v.hex()}
    if hasattr(type(v), "__members__") and hasattr(v, "name"):
        return {"enum": v.name}
    if isinstance(v, dict):
        return {"dict": [[value_out(k), value_out(x)] for k, x in v.items()]}
    if isinstance(v, (list, tuple)) or hasattr(type(v), "__next__"):
        return [value_out(x) for x in v]
    out = {"class": type(v).__name__}
    if _own_repr(v):
        try:
            out["repr"] = repr(v)
        except UnicodeDecodeError as exc:
            out["repr"] = {"exception": raised(exc)}
    return out


def raised(exc):
    """What a reported exception says: {"type", "message"}, and for UnicodeDecodeError the
    bytes the binding couldn't decode."""
    out = exception_result(exc)
    if isinstance(exc, UnicodeDecodeError):
        out["bytes"] = bytes(exc.object).hex()
    return out


def attempt(fn, *args):
    """fn(*args) written out (value_out), or {"exception": ...} when the library raised."""
    try:
        return value_out(fn(*args))
    except REPORTED as exc:
        return {"exception": raised(exc)}


def attempt_dump(fn):
    """fn(), a dump already written out, or {"exception": ...} when the library raised."""
    try:
        return fn()
    except REPORTED as exc:
        return {"exception": raised(exc)}


# ---------------------------------------------------------------------------------------------
# The environment and the directory of a request


def _startup_environment():
    """The variables the process started with, with their names' exact case and bytes:
    nt.environ on Windows, posix.environ (bytes) elsewhere. os.environ uppercases names on
    Windows, and neither changes after startup unless something sets a variable through
    os.environ, which no command does."""
    if os.name == "nt":
        import nt  # noqa: PLC0415 - exists on Windows only
        return dict(nt.environ)
    import posix  # noqa: PLC0415 - exists on POSIX only
    return dict(posix.environ)


def _env_name_value(name, value):
    name, value = text("an environment variable's name", name), text("its value", value)
    if os.name == "nt" and (isinstance(name, bytes) or isinstance(value, bytes)):
        raise RequestError("on Windows, environment variables are given as text")
    if (name.decode("utf-8", "replace") if isinstance(name, bytes) else name) in FIXED_VARIABLES:
        raise RequestError(f"{name!r} can't be set per request: OCIO reads it once per process")
    return name, value


@contextlib.contextmanager
def environment(env):
    """Runs the block in an environment holding exactly `env` ({name: value}), then restores
    the process's own. On Windows, setting a variable to "" removes it (_wputenv), as OCIO's
    own Setenv does."""
    if env is None:
        env = {}
    if not isinstance(env, dict):
        raise RequestError(f"env must be an object, not {env!r}")
    pairs = [_env_name_value(name, value) for name, value in env.items()]
    saved = _startup_environment()
    for name in saved:
        os.unsetenv(name)
    try:
        for name, value in pairs:
            os.putenv(name, value)
        yield
    finally:
        for name, _ in pairs:
            os.unsetenv(name)
        for name, value in saved.items():
            os.putenv(name, value)


def _check_relative(path):
    parts = path.replace("\\", "/").split("/")
    if not path or os.path.isabs(path) or ":" in path or any(p in ("", ".", "..") for p in parts):
        raise RequestError(f"file path {path!r} must be relative, without '.', '..' or empty parts")


@contextlib.contextmanager
def directory(files):
    """A new temporary directory holding `files` ({relative path: content}, content text
    written as UTF-8 or {"bytes": hex}), made the working directory for the block; yields its
    absolute path."""
    files = {} if files is None else files
    if not isinstance(files, dict):
        raise RequestError(f"files must be an object, not {files!r}")
    contents = []
    for path, content in files.items():
        _check_relative(path)
        content = text(f"files[{path!r}]", content)
        contents.append((path, content.encode("utf-8") if isinstance(content, str) else content))
    previous = os.getcwd()
    with tempfile.TemporaryDirectory() as root:
        root = os.path.realpath(root)
        for path, content in contents:
            full = os.path.join(root, *path.replace("\\", "/").split("/"))
            os.makedirs(os.path.dirname(full), exist_ok=True)
            with open(full, "wb") as f:
                f.write(content)
        os.chdir(root)
        try:
            yield root
        finally:
            os.chdir(previous)


# ---------------------------------------------------------------------------------------------
# Config sources


def make_config(source):
    """A config from a source:
      "raw"                Config.CreateRaw()
      "new"                Config() (Config::Create)
      "env"                Config.CreateFromEnv(), which reads $OCIO
      {"yaml": text}       Config.CreateFromStream(text)
      {"file": path}       Config.CreateFromFile(path): relative to the request's directory, or
                           absolute, or an ocio:// URI
      {"builtin": name}    Config.CreateFromBuiltinConfig(name)"""
    if source == "raw":
        return OCIO.Config.CreateRaw()
    if source == "new":
        return OCIO.Config()
    if source == "env":
        return OCIO.Config.CreateFromEnv()
    if isinstance(source, dict) and len(source) == 1:
        (kind, value), = source.items()
        if kind == "yaml":
            return OCIO.Config.CreateFromStream(text("yaml", value))
        if kind == "file":
            return OCIO.Config.CreateFromFile(text("file", value))
        if kind == "builtin":
            return OCIO.Config.CreateFromBuiltinConfig(text("builtin", value))
    raise RequestError(f"unknown config source {source!r}")


# ---------------------------------------------------------------------------------------------
# Dumps


def _keyed(obj, method, *args):
    return attempt(getattr(obj, method), *args)


def _getters(obj, skip=()):
    """Every method of obj named get*, is* or has* that takes no argument, called, by name;
    and the names of those that need arguments."""
    getters, uncalled = {}, []
    for name in sorted(dir(obj)):
        if not name.startswith(("get", "is", "has")) or name in skip:
            continue
        method = getattr(obj, name)
        if not callable(method):
            continue
        try:
            getters[name] = value_out(method())
        except TypeError:
            uncalled.append(name)
        except REPORTED as exc:
            getters[name] = {"exception": raised(exc)}
    return getters, uncalled


def _enum_members(enum):
    return list(enum.__members__.values())


# Per class: (the getters taking arguments that the dump calls, a function of the object giving
# each call's arguments: its own names, aliases, categories, attribute keys and rule indices,
# and every direction), and the getters it leaves out because others give the same (a color
# space set's color spaces, as names).
def _interchange(o):
    return [("getInterchangeAttribute", (k,)) for k in o.getInterchangeAttributes()]


def _tokens(o, method, getter):
    return [(method, (t,)) for t in getattr(o, getter)()]


def _directions(enum):
    return [("getTransform", (d,)) for d in _enum_members(enum)]


def _rules(o, per_rule):
    entries = range(o.getNumEntries())
    return ([(m, (i,)) for i in entries for m in per_rule + ("getNumCustomKeys",)]
            + [(m, (i, k)) for i in entries for k in range(o.getNumCustomKeys(i))
               for m in ("getCustomKeyName", "getCustomKeyValue")]
            + [("getIndexForRule", (o.getName(i),)) for i in entries])


TRANSFORM_DIRECTIONS = (OCIO.TRANSFORM_DIR_FORWARD, OCIO.TRANSFORM_DIR_INVERSE)
RULE_KEYS = ("getNumCustomKeys", "getCustomKeyName", "getCustomKeyValue", "getIndexForRule")
FILE_RULE_GETTERS = ("getName", "getPattern", "getExtension", "getRegex", "getColorSpace")
VIEWING_RULE_GETTERS = ("getName", "getColorSpaces", "getEncodings")

KEYED = {
    "ColorSpace": (
        ("getTransform", "hasAlias", "hasCategory", "getInterchangeAttribute"),
        lambda o: (_directions(OCIO.ColorSpaceDirection) + _tokens(o, "hasAlias", "getAliases")
                   + _tokens(o, "hasCategory", "getCategories") + _interchange(o))),
    "Look": (("getInterchangeAttribute",), _interchange),
    "ViewTransform": (
        ("getTransform", "hasCategory", "getInterchangeAttribute"),
        lambda o: (_directions(OCIO.ViewTransformDirection)
                   + _tokens(o, "hasCategory", "getCategories") + _interchange(o))),
    "NamedTransform": (
        ("getTransform", "GetTransform", "hasAlias", "hasCategory"),
        lambda o: ([("getTransform", (d,)) for d in TRANSFORM_DIRECTIONS]
                   + [("GetTransform", (o, d)) for d in TRANSFORM_DIRECTIONS]
                   + _tokens(o, "hasAlias", "getAliases")
                   + _tokens(o, "hasCategory", "getCategories"))),
    "ColorSpaceSet": (("hasColorSpace",), lambda o: _tokens(o, "hasColorSpace",
                                                            "getColorSpaceNames")),
    "FileRules": (FILE_RULE_GETTERS + RULE_KEYS, lambda o: _rules(o, FILE_RULE_GETTERS)),
    "ViewingRules": (VIEWING_RULE_GETTERS + RULE_KEYS, lambda o: _rules(o, VIEWING_RULE_GETTERS)),
}
SKIPPED = {"ColorSpaceSet": ("getColorSpaces", "getColorSpace")}


def dump_object(obj):
    """What an object other than a config holds: {"class", "repr" (where the binding gives one),
    "getters": every get*/is*/has* method without arguments, by name, "keyed": [[method,
    [arguments], value]] for the getters with arguments KEYED lists ("self" for the object
    itself), or {"exception": ...} where listing them raised, "uncalled": the get*/is*/has*
    methods that KEYED doesn't list, and those SKIPPED leaves out}. Values as value_out writes them,
    or {"exception": ...}. A transform is {"class", "repr"}."""
    if isinstance(obj, OCIO.Transform) or isinstance(obj, (list, tuple)) or obj is None:
        return value_out(obj)
    cls = type(obj).__name__
    out = value_out(obj)
    skipped = SKIPPED.get(cls, ())
    getters, uncalled = _getters(obj, skip=skipped)
    out["getters"] = getters
    methods, planner = KEYED.get(cls, ((), lambda o: []))
    keyed = []
    try:
        plan = planner(obj)
    except REPORTED as exc:
        plan, keyed = [], {"exception": raised(exc)}
    for method, args in plan:
        shown = ["self" if a is obj else value_out(a) for a in args]
        keyed.append([method, shown, _keyed(obj, method, *args)])
    out["keyed"] = keyed
    out["uncalled"] = sorted(set(uncalled) - set(methods) | set(skipped))
    return out


def _names(iterator):
    return [name for name in iterator]


def dump_config(config):
    """Everything a config holds, through every getter, in this order (each value as value_out
    writes it, or {"exception": ...} where the getter raised):
      getters       every get*/is*/has* method without arguments, by name (objects as
                    value_out writes them; the objects are dumped below)
      color_space_names   for each SearchReferenceSpaceType and ColorSpaceVisibility,
                    "TYPE/VISIBILITY": getColorSpaceNames(type, visibility)
      color_spaces  per color space of getColorSpaceNames(ALL, ALL), in order: {"name", "dump"
                    (getColorSpace(name), dumped), "isColorSpaceUsed", "isInactiveColorSpace",
                    "isColorSpaceLinear": per ReferenceSpaceType}
      categories    per category of those color spaces (in order of appearance, as
                    getCategories gives them): [category, getColorSpaces(category) dumped]
      roles         per role of getRoleNames(): {"role", "getRoleColorSpace", "hasRole"}
      lookups       per name of the color spaces, their aliases, the roles, the named transforms
                    and their aliases, the looks and the view transforms (each once, in that
                    order): {"name", "getColorSpace" (the found space's name, or null),
                    "getCanonicalName", "getNamedTransform" (the found one's name, or null)}
      environment   per name of getEnvironmentVarNames(): [name, getEnvironmentVarDefault]
      displays      per display of getDisplaysAll(): {"display", "isDisplayTemporary",
                    "getDefaultView", "getViews": getViews(display), "VIEW_SHARED" and
                    "VIEW_DISPLAY_DEFINED": getViews(type, display), "views": per view of
                    those (each once) the view getters below, "by_color_space": per color space
                    [name, getViews(display, name), getDefaultView(display, name)]}
      shared_views  per view of getSharedViews(): the view getters with display ""
      virtual_display  {"VIEW_SHARED" and "VIEW_DISPLAY_DEFINED": getVirtualDisplayViews(type),
                    "views": per view of those, {"view", "hasVirtualView", "isVirtualViewShared",
                    and getVirtualDisplayView{TransformName,ColorSpaceName,Looks,Rule,
                    Description}}}
      looks         per name of getLookNames(): getLook(name) dumped
      view_transforms  per name of getViewTransformNames(): getViewTransform(name) dumped;
                    then "default_scene_to_display": getDefaultSceneToDisplayViewTransform()
                    dumped
      named_transform_names  per NamedTransformVisibility: getNamedTransformNames(visibility)
      named_transforms  per name of getNamedTransformNames(ALL): getNamedTransform(name) dumped
      current_context, file_rules, viewing_rules   getCurrentContext(), getFileRules(),
                    getViewingRules(), dumped
      serialize     serialize()
      validate      null, or validate()'s exception
      not_dumped    the get*/is*/has* methods the dump never calls
    The view getters: {"view", "hasView", "isViewShared", and getDisplayView{TransformName,
    ColorSpaceName,Looks,Rule,Description}}."""
    out = {"class": "Config"}
    getters, _ = _getters(config, skip=("getColorSpaces", "getLooks", "getViewTransforms",
                                               "getNamedTransforms"))
    out["getters"] = getters
    called = set(getters)

    def note(*names):
        called.update(names)

    names = {}
    for kind in _enum_members(OCIO.SearchReferenceSpaceType):
        for vis in _enum_members(OCIO.ColorSpaceVisibility):
            names[f"{kind.name}/{vis.name}"] = attempt(
                lambda k=kind, v=vis: _names(config.getColorSpaceNames(k, v)))
    out["color_space_names"] = names
    note("getColorSpaceNames")
    all_names = _names(config.getColorSpaceNames(OCIO.SEARCH_REFERENCE_SPACE_ALL,
                                                 OCIO.COLORSPACE_ALL))

    spaces, categories, aliases = [], [], []
    for name in all_names:
        cs = config.getColorSpace(name)
        entry = {"name": name, "dump": dump_object(cs) if cs is not None else None,
                 "isColorSpaceUsed": attempt(config.isColorSpaceUsed, name),
                 "isInactiveColorSpace": attempt(config.isInactiveColorSpace, name),
                 "isColorSpaceLinear": {t.name: attempt(config.isColorSpaceLinear, name, t)
                                        for t in _enum_members(OCIO.ReferenceSpaceType)}}
        spaces.append(entry)
        if cs is not None:
            categories.extend(c for c in cs.getCategories() if c not in categories)
            aliases.extend(a for a in cs.getAliases())
    out["color_spaces"] = spaces
    note("getColorSpace", "isColorSpaceUsed", "isInactiveColorSpace", "isColorSpaceLinear")

    out["categories"] = [[c, attempt_dump(lambda c=c: dump_object(config.getColorSpaces(c)))]
                         for c in categories]
    note("getColorSpaces")

    roles = _names(config.getRoleNames())
    out["roles"] = [{"role": r, "getRoleColorSpace": attempt(config.getRoleColorSpace, r),
                     "hasRole": attempt(config.hasRole, r)} for r in roles]
    note("getRoleColorSpace", "hasRole")

    nt_names = _names(config.getNamedTransformNames(OCIO.NAMEDTRANSFORM_ALL))
    nt_aliases = []
    for name in nt_names:
        nt = config.getNamedTransform(name)
        if nt is not None:
            nt_aliases.extend(nt.getAliases())
    looks = _names(config.getLookNames())
    vts = _names(config.getViewTransformNames())
    keys = []
    for key in all_names + aliases + roles + nt_names + nt_aliases + looks + vts:
        if key not in keys:
            keys.append(key)

    def found_name(getter, key):
        found = getter(key)
        return None if found is None else found.getName()

    out["lookups"] = [{"name": k,
                       "getColorSpace": attempt(found_name, config.getColorSpace, k),
                       "getCanonicalName": attempt(config.getCanonicalName, k),
                       "getNamedTransform": attempt(found_name, config.getNamedTransform, k)}
                      for k in keys]
    note("getCanonicalName", "getNamedTransform")

    out["environment"] = [[n, attempt(config.getEnvironmentVarDefault, n)]
                          for n in _names(config.getEnvironmentVarNames())]
    note("getEnvironmentVarDefault")

    def view_getters(display, view):
        entry = {"view": view, "hasView": attempt(config.hasView, display, view),
                 "isViewShared": attempt(config.isViewShared, display, view)}
        for what in ("TransformName", "ColorSpaceName", "Looks", "Rule", "Description"):
            entry[f"getDisplayView{what}"] = attempt(getattr(config, f"getDisplayView{what}"),
                                                     display, view)
        return entry

    displays = []
    for display in _names(config.getDisplaysAll()):
        entry = {"display": display,
                 "isDisplayTemporary": attempt(config.isDisplayTemporary, display),
                 "getDefaultView": attempt(config.getDefaultView, display),
                 "getViews": attempt(lambda d=display: _names(config.getViews(d)))}
        views = []
        for vt in _enum_members(OCIO.ViewType):
            entry[vt.name] = attempt(lambda t=vt, d=display: _names(config.getViews(t, d)))
            if isinstance(entry[vt.name], list):
                views.extend(v for v in entry[vt.name] if v not in views)
        if isinstance(entry["getViews"], list):
            views.extend(v for v in entry["getViews"] if v not in views)
        entry["views"] = [view_getters(display, v) for v in views]
        entry["by_color_space"] = [
            [cs, attempt(lambda d=display, c=cs: _names(config.getViews(d, c))),
             attempt(config.getDefaultView, display, cs)] for cs in all_names]
        displays.append(entry)
    out["displays"] = displays
    out["shared_views"] = [view_getters("", v) for v in _names(config.getSharedViews())]
    note("isDisplayTemporary", "getDefaultView", "getViews", "hasView", "isViewShared",
         *[f"getDisplayView{w}" for w in ("TransformName", "ColorSpaceName", "Looks", "Rule",
                                          "Description")])

    virtual = {}
    vviews = []
    for vt in _enum_members(OCIO.ViewType):
        virtual[vt.name] = attempt(lambda t=vt: _names(config.getVirtualDisplayViews(t)))
        if isinstance(virtual[vt.name], list):
            vviews.extend(v for v in virtual[vt.name] if v not in vviews)
    virtual["views"] = []
    for view in vviews:
        entry = {"view": view, "hasVirtualView": attempt(config.hasVirtualView, view),
                 "isVirtualViewShared": attempt(config.isVirtualViewShared, view)}
        for what in ("TransformName", "ColorSpaceName", "Looks", "Rule", "Description"):
            entry[f"getVirtualDisplayView{what}"] = attempt(
                getattr(config, f"getVirtualDisplayView{what}"), view)
        virtual["views"].append(entry)
    out["virtual_display"] = virtual
    note("getVirtualDisplayViews", "hasVirtualView", "isVirtualViewShared",
         *[f"getVirtualDisplayView{w}" for w in ("TransformName", "ColorSpaceName", "Looks",
                                                 "Rule", "Description")])

    out["looks"] = [attempt_dump(lambda n=n: dump_object(config.getLook(n))) for n in looks]
    out["view_transforms"] = [attempt_dump(lambda n=n: dump_object(config.getViewTransform(n)))
                              for n in vts]
    out["default_scene_to_display"] = attempt_dump(
        lambda: dump_object(config.getDefaultSceneToDisplayViewTransform()))
    note("getLook", "getViewTransform", "getLooks", "getViewTransforms")

    out["named_transform_names"] = {
        v.name: attempt(lambda v=v: _names(config.getNamedTransformNames(v)))
        for v in _enum_members(OCIO.NamedTransformVisibility)}
    out["named_transforms"] = [attempt_dump(lambda n=n: dump_object(config.getNamedTransform(n)))
                               for n in nt_names]
    note("getNamedTransformNames", "getNamedTransforms")

    out["current_context"] = attempt_dump(lambda: dump_object(config.getCurrentContext()))
    out["file_rules"] = attempt_dump(lambda: dump_object(config.getFileRules()))
    out["viewing_rules"] = attempt_dump(lambda: dump_object(config.getViewingRules()))
    out["serialize"] = attempt(config.serialize)
    try:
        config.validate()
        out["validate"] = None
    except REPORTED as exc:
        out["validate"] = {"exception": raised(exc)}
    out["not_dumped"] = [n for n in sorted(dir(config))
                         if n.startswith(("get", "is", "has")) and n not in called]
    return out


def dump(obj):
    return dump_config(obj) if isinstance(obj, OCIO.Config) else dump_object(obj)


# ---------------------------------------------------------------------------------------------
# Calls


def _target(on, objects):
    """The object a call acts on: a stored object, a PyOpenColorIO class (for its static
    methods), or "OCIO" (the module, for MODULE_FUNCTIONS)."""
    if on in objects:
        return objects[on]
    if on == "OCIO":
        return OCIO
    cls = getattr(OCIO, on, None) if isinstance(on, str) else None
    if isinstance(cls, type):
        return cls
    raise RequestError(f"no object is stored as {on!r}, and it isn't a PyOpenColorIO class")


def _method(target, name):
    if not isinstance(name, str) or (name.startswith("_") and name not in DUNDER_CALLS):
        raise RequestError(f"call {name!r}: not a method a call may name")
    if target is OCIO and name not in MODULE_FUNCTIONS:
        raise RequestError(f"call {name!r} on OCIO: only {sorted(MODULE_FUNCTIONS)}")
    method = getattr(target, name, None)
    if method is None or not callable(method):
        raise RequestError(f"{type(target).__name__} has no method {name!r}")
    return method


CALL_KEYS = {
    "call": {"call", "on", "args", "kwargs", "as"},
    "new": {"new", "args", "kwargs", "as"},
    "copy": {"copy", "as"},
    "dump": {"dump"},
}


def run_calls(calls, objects, subject):
    """Runs the calls in order. Each is one of:
      {"call": method, "on": name (default: the subject), "args": [values],
       "kwargs": {name: value}, "as": name}
                  on.method(*args, **kwargs); "on" names a stored object, a PyOpenColorIO
                  class (its static methods) or "OCIO" (MODULE_FUNCTIONS); "as" stores the
                  result for later calls
      {"new": class, "args", "kwargs", "as"}   a PyOpenColorIO class's constructor
      {"copy": name, "as": name}               copy.deepcopy (the binding's __deepcopy__:
                  createEditableCopy)
      {"dump": name}                           dump (dump_config or dump_object)
    Values as value_in reads them. Returns per call {"result": value_out of the result} or
    {"exception": {"type", "message"}} (where the library raised: nothing is stored then), and
    "log": what OCIO logged during the call."""
    if not isinstance(calls, list):
        raise RequestError(f"calls must be a list, not {calls!r}")
    results = []
    with captured_log() as log:
        for i, call in enumerate(calls):
            kind = next((k for k in CALL_KEYS if isinstance(call, dict) and k in call), None)
            if kind is None:
                raise RequestError(f"calls[{i}]: {call!r} is none of {sorted(CALL_KEYS)}")
            check_keys(f"calls[{i}]", call, CALL_KEYS[kind])
            store = call.get("as")
            if store is not None and (not isinstance(store, str) or store in ("OCIO",)):
                raise RequestError(f"calls[{i}]: can't store as {store!r}")
            args = value_in(call.get("args", []), objects)
            kwargs = call.get("kwargs", {})
            if not isinstance(kwargs, dict):
                raise RequestError(f"calls[{i}].kwargs must be an object")
            kwargs = {k: value_in(v, objects) for k, v in kwargs.items()}
            if not isinstance(args, list):
                raise RequestError(f"calls[{i}].args must be a list")
            if kind == "call":
                fn = _method(_target(call.get("on", subject), objects), call["call"])
            elif kind == "new":
                cls = getattr(OCIO, call["new"], None) if isinstance(call["new"], str) else None
                if not isinstance(cls, type):
                    raise RequestError(f"calls[{i}]: {call['new']!r} isn't a PyOpenColorIO class")
                fn = cls
            elif kind == "copy":
                source = _target(call["copy"], objects)
                fn = lambda: copy.deepcopy(source)  # noqa: E731
            else:
                source = _target(call["dump"], objects)
                fn = lambda: dump(source)  # noqa: E731
            start = len(log)
            try:
                value = fn(*args, **kwargs)
            except REPORTED as exc:
                results.append({"exception": raised(exc), "log": log[start:]})
                continue
            if kind == "dump":
                results.append({"result": value, "log": log[start:]})
                continue
            if store is not None:
                objects[store] = value
            results.append({"result": value_out(value), "log": log[start:]})
    return results


def run_request(args, subject, make_subject, extra_keys=()):
    """The frame every Phase 3 call command shares: the environment and the directory, the
    subject (stored as `subject`), then the calls. Returns the result object."""
    check_keys(f"{subject}_calls", args, {"env", "files", "calls", subject, *extra_keys})
    result = {}
    with directory(args.get("files")) as root, environment(args.get("env")):
        result["dir"] = root
        objects = {}
        with captured_log() as log:
            try:
                objects[subject] = make_subject(args.get(subject))
                result[subject] = None
            except REPORTED as exc:
                result[subject] = {"exception": raised(exc)}
        result[f"{subject}_log"] = list(log)
        result["calls"] = run_calls(args.get("calls", []), objects, subject) \
            if subject in objects else []
    return result


@command
def config_calls(args, blobs):
    """A config from a source, then calls on it and on the objects they give.

    args:
      config    the source (see make_config): "raw", "new", "env", {"yaml": text},
                {"file": path} or {"builtin": name}
      env       {name: value}: the environment of the whole request, which holds exactly these
                variables (none by default); names and values are text, or on Linux
                {"bytes": hex}. OCIO_LOGGING_LEVEL is refused
      files     {relative path: content}: written to a new temporary directory, which is the
                working directory of the whole request (so {"file": "config.ocio"} finds
                them); content is text (written as UTF-8) or {"bytes": hex}
      calls     the calls (see run_calls), on "config" unless they say otherwise
    result:
      dir         the temporary directory's absolute path, as the paths OCIO reports start
      config      null, or {"exception"} when making the config raised (no call runs then)
      config_log  what OCIO logged while making the config
      calls       per call {"result"} or {"exception"}, and "log" (see run_calls)
    blobs: none

    An unknown key, a call naming a method that doesn't exist or one it may not reach, a
    value spec it doesn't know, or arguments no overload takes (pybind11's TypeError) make the
    command raise, so the call fails.
    """
    return run_request(args, "config", make_config), []

