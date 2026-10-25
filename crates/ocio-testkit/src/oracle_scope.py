# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""What each oracle command's answer can depend on, for the oracle's response cache.

Run by ``crates/ocio-testkit/src/oracle_scope.rs`` as ``python -I -X utf8 - <package dir>``,
with this source on stdin, from the oracle's directory and with the oracle's environment. It
reads every top-level module of the oracle's package (``ocio_oracle``) with ``ast``, then
imports every module of the package but ``__main__``: a module that fails to import fails
every process of the oracle, so the analysis fails, and every key is the whole oracle's. It
prints one JSON object:

    {"format": 3,
     "modules": {"<name>.py": {
        "edges": [the modules this one reaches statically, as "<name>.py"],
        "commands": [the @command functions it defines],
        "dispatchers": [those of its commands that may run another command by name],
        "fallback": [why the module's code can't be resolved statically],
        "import_fallback": [those of the fallback reasons in code that may run when the
                            module is imported (below)],
        "global": [why its top level may act on every command's process]}},
     "pins": {"<name>.py:<function>": the SHA-256 of the function's syntax tree, without its
              docstring, for commands.py's `command` and `_import_package_modules`, and for
              every dispatcher (for one that reaches another definition that may run a
              command by name: "dispatches through <name>.py:<definition>, ..."),
              "commands.py:COMMANDS": the SHA-256 of the statement that binds the registry,
              "__main__.py": the SHA-256 of the module's syntax tree, without its
              docstrings}}

It reports facts; ``oracle_scope.rs`` decides the keys. It is an allowlist: what it can't
prove harmless is a "fallback" reason (a command that can reach the module gets the whole
oracle's key) or a "global" reason (every key covers the module and what it reaches:
``commands.py`` imports every module of the package in every process).

Global: a module's top level may hold only
- constants (a docstring), ``pass``, and imports of a sibling or of a module of IMPORTABLE;
- ``def``, whose decorators are the innermost ``@command`` and those of PURE_DECORATORS (called
  with constants only: ``functools.lru_cache(f)`` decorates ``f`` and calls it), and whose
  defaults and annotations are inert (below);
- ``class`` without keywords or decorators, whose bases are builtin types, ``object``, a
  class of a module outside the package, or an earlier class of the module that defines (or
  assigns) none of CLASS_HOOKS; whose body holds constants, ``pass``, ``def`` (decorated as
  above, or with ``staticmethod``, ``classmethod``, ``property`` or a property's ``setter``),
  assignments of inert values to names, and classes of the same kind. When a base other than
  ``object`` and the builtin exceptions (PLAIN_BASES) is among its ancestors, whose metaclass
  may call what the class defines while it makes the class (``enum.Enum`` makes its members
  by calling the class), its body holds no ``def`` and no class, and assigns only DATA;
- assignments of inert values to names (``X = ...``, ``X: T = ...``, ``A, B = ...``, which
  unpacks DATA, an iterator made there, or a tuple or list display);
- ``if __name__ == "__main__":`` without ``else``.
An inert value runs no code of the package and calls nothing outside an allowlist. It is DATA
(using it runs no code of the package and changes nothing that other code shares), CODE (it
is, or holds, a function, a class or a lambda of the package) or HELD (it is, or holds, a
one-shot iterator that other code may share).
- DATA: constants; builtin types and exceptions, ``__file__`` and ``__name__``; a module
  outside the package and its attributes, under any name (OUTSIDE, a kind of DATA), but the
  process's streams; operators, comparisons, tests, subscripts, slices and f-strings of DATA;
  sets, dict keys, comprehensions (over names, iterating DATA) and ``*`` items of DATA; the
  non-mutating methods INERT_METHODS of DATA but OUTSIDE (an object of a module outside the
  package isn't builtin data: ``os.environ.copy()``, or ``E.copy()`` after ``E =
  os.environ``); the calls PURE_CALLS (builtins) and PURE_EXTERNAL_CALLS, with DATA arguments
  and no ``key=`` (a function the call calls: ``sorted(x, key=os.chdir)``, or a method that
  changes data, ``sorted([0], key=QUEUE.pop)``).
- FRESH: an iterator made where it is used, which shares nothing with other code until it is
  held: what ITERATOR_CALLS and ITERATOR_EXTERNAL_CALLS return (``zip``, ``enumerate``,
  ``reversed`` and ``itertools.count``) with DATA arguments, and a generator expression whose
  items are DATA. It is DATA only where it is consumed on the spot: the iterable of a consuming
  call (CONSUMING_CALLS: ``list``, ``tuple``, ``set``, ``frozenset``, ``dict``, ``sorted``,
  ``sum``'s first argument; CONSUMING_ONE: ``min`` or ``max``'s only one), an argument of
  ITERATOR_CALLS, ITERATOR_EXTERNAL_CALLS or CONSUMING_METHODS (``", ".join(...)``), a
  comprehension's iterable, a ``*`` item, and the value of an unpacking assignment
  (``dict(zip(KEYS, VALUES))``). Anywhere else it is HELD: an operand of ``and``, ``or`` or a
  conditional expression (``zip(a, b) or ()``), a set's item or a dict's key, a keyword's
  value (``dict(pairs=zip(a, b))``, ``max((), default=zip(a, b))``), ``sum``'s start, another
  method's argument (``{}.get("k", zip(a, b))``), an attribute's object, and what a name, a
  display or a dict's value holds.
- CODE: a function of the package, an earlier class of the module (as above), a lambda (its
  body isn't evaluated, its defaults are), an attribute of CODE, a display that holds CODE,
  and a generator expression whose items are CODE.
- HELD: an iterator held (by a name, a display, ...), the process's streams (STREAM_NAMES:
  ``sys`` itself under any name, as ``S = sys; S.stdin``; an attribute named ``sys``, as
  ``os.sys``; and an attribute named as a stream, as ``sys.stdin``, ``sys.__stdin__`` or
  ``sys.stdin.buffer``) and their attributes, an attribute of HELD, a display that holds HELD,
  and a generator expression whose items are HELD.
A name bound once to an inert value, here or in the sibling it is imported from, and
``<sibling>.<name>`` of one, is of its value's kind (HELD for FRESH). CODE and HELD may only
be held (by a name, a display's item, a dict's value, a default or an annotation): an
operator, a hash, an iteration, an unpacking, a format, a call it is passed to or a method of
it can run the package's code (``sorted(x, key=f)``, ``K[0]`` and ``__class_getitem__``,
``{E.A: 1}`` and ``__hash__``, ``dict(E.A for _ in R)`` and ``__iter__``), or consume an
iterator that other code shares (``dict(zip(KEYS, ids.COUNTER))``, ``list(pairs.PAIRS)``,
``list(zip(R, S.stdin))``). Anything else is a global reason.

Edges:
- imports of a sibling, at any depth: ``from . import x``, ``from .x import y``, ``import
  ocio_oracle.x``, ``from ocio_oracle import x``, ``from ocio_oracle.x import y``, and the bare
  ``import x`` and ``from x import y`` of a sibling's name (a script run from the package's
  directory, or a child whose path holds it, imports its siblings so);
- a string naming a sibling's file, ``"x.py"`` (a script the module starts), or a module,
  ``"ocio_oracle.x"`` or ``"ocio_oracle/x"`` (``ocio_oracle`` followed by another character
  than a separator, as in an import, names ``__init__.py``);
- the imports of Python code in a string (a child process's source).

Names are resolved where they are used: a function's parameters and the names it binds are
its own, and an import binds its name in the function (or the module) it is in.

Fallback reasons:
- an import of a module outside IMPORTABLE; any use of a module of IMPORTABLE outside USABLE
  (``importlib``, ``pkgutil``, ``builtins``, ``ctypes``, ``inspect``, ``gc``, ``runpy``,
  ``unittest``: they import, or reach, modules by a name they are given), but as a class's
  base; an import from above the package, of ``*``, of a subpackage or a missing sibling, or
  of a name its sibling doesn't define (a name the sibling imports is no definition: ``from
  .commands import importlib``); two imports binding one name differently in one scope;
- the names DENIED_NAMES (``__import__``, ``exec``, ``eval``, ``compile``, ``globals``,
  ``__builtins__``, ...), and ``vars`` with other than one argument;
- the package object (the name ``ocio_oracle``), and a sibling module used other than as
  ``<module>.<name>`` of a name the sibling defines (``getattr(commands, ...)``,
  ``vars(commands)``, ``commands.__file__``, ``commands.importlib``);
- ``sys``, ``os``, ``nt``, ``posix`` and ``subprocess`` used other than as ``<module>.<name>``;
  the attributes of ``sys`` outside SYS_ATTRIBUTES (``sys.modules``, ``sys.path``,
  ``sys._getframe``, ...; a child's source may call ``sys.path.insert``); ``sys.argv`` but in
  ``__main__.py`` (the oracle's arguments: the first is ``__main__.py``'s path, in the
  package's directory); an attribute, of anything, named as a function of ``os`` that starts
  a process (PROCESS_ATTRIBUTES), as one of those modules or as one outside USABLE
  (``OCIO.os``); a use of ``subprocess`` that isn't a call of ``run``, ``Popen``, ``call``,
  ``check_call`` or ``check_output``, without ``executable=``, with a list display of
  arguments that, when it starts with ``sys.executable``, holds options (``"-I"``, ``"-X"
  "utf8"``, ...; none joined with ``c`` or ``m``, as ``"-Ic"`` or ``"-cimport x"``, and no
  ``"-"``), then either ``"-c"`` and a string, or a name bound only once, at the top level,
  to a string, which is analysed as a child's source (it must parse), or a script: the
  package's directory joined with a sibling's ``"x.py"``, directly or through a name bound
  only once, at the top level, to it;
- ``sys.executable`` other than first in such a list; ``"-m"`` anywhere; an option holding
  ``c`` (``"-c"``, ``"-Ic"``, ``"-cimport x"``) in a list of arguments that doesn't start with
  ``sys.executable``;
- attributes whose names start and end with two underscores, outside DUNDER_ATTRIBUTES
  (``__globals__``, ``__subclasses__``, ``__self__``, ``__code__``, ...; ``__file__`` only of
  a module outside the package; never ``__dict__`` of one, its namespace: ``os.__dict__``),
  the attributes of frames and of the import system DENIED_ATTRIBUTES, and those names as
  constant strings in ``getattr`` or ``operator.attrgetter``;
- a constant string naming such an attribute, or one of MODULE_ATTRIBUTES or
  PROCESS_ATTRIBUTES (``"__globals__"``, ``"modules"``, ``"os"``), other than compared or in a
  table (a tuple, list or set display, or a dict display's key): bound to a name, returned, a
  default, an argument or an index;
- ``__file__`` other than as the package's directory joined with a sibling's ``"x.py"``, or
  as the oracle's directory assigned to a name whose every use is a value of a dict display,
  an item of a subprocess call's list of arguments, an argument of ``os.path.samefile``, or an
  argument of a function of the module whose parameter is used so in turn;
- a string that names a module or a file of the package in a way it can't resolve: a constant
  naming ``ocio_oracle.<x>`` or ``ocio_oracle/<x>`` that isn't a sibling, or built into a
  longer string (an operand, a method's object, a path join's argument); a string that ends
  with ``ocio_oracle``, the package's directory (the oracle runs from the oracle's
  directory: ``os.listdir("ocio_oracle")``); ``".py"`` built into a file name; an f-string
  part ending with ``ocio_oracle``, and an f-string or a bytes constant naming either; and a
  string that looks like code with an import but doesn't parse (a ``%`` or ``.format``
  template, or ``"-cfrom x import y"``). In a child's source, any string naming either;
- ``__cached__`` (the path of the module's bytecode, in the package's directory);
- the ``command`` decorator used other than as the innermost decorator of a top-level
  function (the registry keys commands by the decorated function's ``__name__``);
- a script, or a child's source, that may run a command by name (below);
- ``_import_package_modules`` named anywhere but in the call at ``commands.py``'s top level.
  That function, which imports every module of the package, is the one function exempt from
  these rules: the "global" reasons model what it does, and its pin holds it to the code the
  rules were written for.

Import fallback: the fallback reasons that can act when a module is imported, in every
process: those of a top-level statement that isn't inert (a global reason), outside the
bodies of the functions it defines; and those of the top-level statements that define a name
that code reaches, through the names it uses, at any depth and across modules. An inert
statement calls nothing of the package and nothing outside the allowlists, so what it binds
can only act when other code uses it. And a module whose code run at import reaches a
definition that may run a command by name, or the registry (``__main__.py``'s own
definitions and the registry they dispatch through aside: the module is pinned whole): a
command run when every process starts.

Dispatchers: a top-level definition may run a command by name when it uses ``COMMANDS``
(through any binding, or the name or string ``"COMMANDS"``; ``commands.py``'s ``command``,
which registers into it, aside), or uses a definition that may, through its own module's
names, the names it imports, or ``<module>.<name>`` of a module it imports; to a fixpoint
across modules. A module-level assignment counts as a definition of the names it binds. A
command that may is a dispatcher; every command of a module whose top level may is one. A
dispatcher's pin covers its own syntax tree; when another definition it reaches (the
registry aside) may run a command by name, its pin says so instead, and ``oracle_scope.rs``
doesn't follow it.

What the analysis doesn't follow, by design: values. A module name or a path assembled at run
time from fragments none of which names the package or a ``.py`` file, or attributes walked by
names a request gives (``getattr`` of a library object), aren't followed; nor is the state of
an object of a module outside the package that its operators, conversions or iteration may
change, but the process's streams (STREAM_NAMES). The request itself is part of every key.
"""

import ast
import builtins
import copy
import hashlib
import json
import os
import re
import sys
import traceback

PACKAGE = "ocio_oracle"

# The modules outside the package that a module may import and use: none imports a module by
# a name it is given, reaches another module's namespace, or starts a process other than
# through the calls checked below.
USABLE = frozenset({
    "PyOpenColorIO", "array", "base64", "binascii", "bisect", "codecs", "collections",
    "contextlib", "copy", "decimal", "difflib", "enum", "errno", "fnmatch", "fractions",
    "functools", "hashlib", "heapq", "io", "itertools", "json", "math", "nt", "numpy",
    "operator", "os", "platform", "posix", "re", "shutil", "stat", "string", "struct",
    "subprocess", "sys", "tempfile", "textwrap", "time", "traceback", "types", "unicodedata",
    "zipfile",
})
# The modules a module may import and still only define names; those outside USABLE are
# fallback reasons wherever their names are used.
IMPORTABLE = USABLE | {"builtins", "ctypes", "gc", "importlib", "inspect", "pkgutil", "runpy",
                       "unittest"}
# Names an attribute may not have: modules that start processes or reach other modules (the
# attribute `ctypes` of a numpy array is no module; DENIED_ATTRIBUTES holds what ctypes'
# module would give).
MODULE_ATTRIBUTES = frozenset({"sys", "os", "nt", "posix", "subprocess"}) | (
    IMPORTABLE - USABLE - {"ctypes"})
# External modules whose names may only be used as `<module>.<name>`, with the checks below.
STRICT = frozenset({"sys", "os", "nt", "posix", "subprocess"})
SYS_ATTRIBUTES = frozenset({
    "byteorder", "exc_info", "executable", "exit", "flags", "float_info",
    "getdefaultencoding", "getfilesystemencoding", "getrecursionlimit", "getsizeof",
    "getwindowsversion", "hexversion", "implementation", "maxsize", "maxunicode", "platform",
    "setrecursionlimit", "stderr", "stdin", "stdout", "version", "version_info",
})
# The functions of os (nt, posix) that start a process; denied as attributes of anything.
PROCESS_ATTRIBUTES = frozenset({
    "system", "popen", "startfile", "fork", "forkpty", "posix_spawn", "posix_spawnp",
    "add_dll_directory", "execl", "execle", "execlp", "execlpe", "execv", "execve", "execvp",
    "execvpe", "spawnl", "spawnle", "spawnlp", "spawnlpe", "spawnv", "spawnve", "spawnvp",
    "spawnvpe",
})
SUBPROCESS_CALLS = frozenset({"run", "Popen", "call", "check_call", "check_output"})
# Options of the interpreter that take a value.
VALUE_OPTIONS = frozenset({"-X", "-W"})

DENIED_NAMES = frozenset({
    "__import__", "exec", "eval", "compile", "__builtins__", "breakpoint", "globals",
    "locals", "help", "__loader__", "__spec__", "__path__", "__package__", "__build_class__",
    "__cached__",
})
DUNDER_ATTRIBUTES = frozenset({
    "__name__", "__qualname__", "__module__", "__doc__", "__members__", "__version__",
    "__init__", "__mro__", "__bases__", "__func__", "__dict__", "__setitem__",
    "__getitem__", "__delitem__", "__enter__", "__exit__", "__len__", "__iter__",
    "__next__", "__contains__", "__eq__", "__ne__", "__lt__", "__le__", "__gt__", "__ge__",
    "__hash__", "__str__", "__repr__", "__bool__", "__float__", "__int__", "__index__",
})
DENIED_ATTRIBUTES = frozenset({
    "f_globals", "f_locals", "f_back", "f_builtins", "f_code", "tb_frame", "tb_next",
    "gi_frame", "gi_code", "cr_frame", "cr_code", "ag_frame", "ag_code", "walk_stack",
    "walk_tb", "_getframe", "_current_frames", "modules", "meta_path", "path_hooks",
    "path_importer_cache", "import_module", "pythonapi", "PyDLL", "pydll", "CDLL", "cdll",
    "WinDLL", "windll", "OleDLL", "oledll", "LibraryLoader", "dlopen",
})
GETATTR_LIKE = frozenset({"getattr", "setattr", "delattr", "hasattr"})

# Inert values (see the module's docstring).
SAFE_BUILTINS = frozenset({
    "object", "type", "bool", "int", "float", "complex", "str", "bytes", "bytearray", "tuple",
    "list", "dict", "set", "frozenset", "range", "slice", "memoryview", "BaseException",
    "Exception", "ArithmeticError", "AssertionError", "AttributeError", "BufferError",
    "EOFError", "FloatingPointError", "ImportError", "IndexError", "KeyError",
    "LookupError", "MemoryError", "ModuleNotFoundError", "NameError", "NotImplementedError",
    "OSError", "OverflowError", "RecursionError", "ReferenceError", "RuntimeError",
    "StopIteration", "SyntaxError", "SystemError", "TypeError", "UnicodeDecodeError",
    "UnicodeEncodeError", "UnicodeError", "ValueError", "ZeroDivisionError",
    "FileNotFoundError", "FileExistsError", "PermissionError", "TimeoutError",
    "NotADirectoryError", "IsADirectoryError", "Warning", "UserWarning",
    "DeprecationWarning", "RuntimeWarning", "__file__", "__name__",
})
PURE_CALLS = frozenset({
    "type", "frozenset", "set", "tuple", "list", "dict", "range", "len", "sorted", "str",
    "int", "float", "bytes", "bool", "repr", "chr", "ord", "abs", "min", "max", "sum",
    "isinstance", "issubclass", "vars",
})
PURE_EXTERNAL_CALLS = frozenset({
    "re.compile", "struct.Struct", "numpy.dtype", "os.path.join", "os.path.dirname",
    "os.path.abspath", "os.path.basename",
})
# The calls that return an iterator, which any use but holding it consumes: the builtins and the
# functions of modules outside the package.
ITERATOR_CALLS = frozenset({"zip", "enumerate", "reversed"})
ITERATOR_EXTERNAL_CALLS = frozenset({"itertools.count"})
# The builtins that consume the iterable they are given as their first argument (`min` and
# `max` only when it is their only one: they compare several), and the methods of builtin data
# that consume every argument they are given (`", ".join(...)`, `set.union(...)`).
CONSUMING_CALLS = frozenset({"list", "tuple", "set", "frozenset", "dict", "sorted", "sum"})
CONSUMING_ONE = frozenset({"min", "max"})
CONSUMING_METHODS = frozenset({"join", "union", "intersection", "difference"})
# The objects of modules outside the package that are iterators, the process's streams, under
# any name: the module `sys` itself (`S = sys; S.stdin`), an attribute named `sys` (`os.sys`),
# and an attribute named as a stream (`sys.stdin`, `sys.__stdin__`, `sys.stdin.buffer`).
STREAM_NAMES = frozenset({"stdin", "stdout", "stderr", "__stdin__", "__stdout__", "__stderr__"})
INERT_METHODS = frozenset({
    "items", "keys", "values", "get", "copy", "union", "intersection", "difference",
    "format", "join", "split", "strip", "lower", "upper", "startswith", "endswith", "count",
    "index",
})
PURE_DECORATORS = frozenset({"contextlib.contextmanager", "functools.cache",
                             "functools.lru_cache"})
CLASS_DECORATORS = frozenset({"staticmethod", "classmethod", "property"})
CLASS_HOOKS = frozenset({"__init_subclass__", "__class_getitem__", "__mro_entries__",
                         "__set_name__", "__prepare__"})
# The bases whose metaclass is `type` and that look up none of the class's own methods when a
# class is made: `object` and the builtin exceptions. Any other base (a builtin type, or a class
# of a module outside the package: `enum.Enum`, whose metaclass makes members by calling the
# class) may run the class's methods when the class is made.
PLAIN_BASES = frozenset({"object"} | {
    name for name in SAFE_BUILTINS
    if isinstance(getattr(builtins, name, None), type)
    and issubclass(getattr(builtins, name), BaseException)})
# Names of attributes the rules deny, as strings: a string naming one is a fallback reason
# unless it is compared, or held in a tuple, list or set display or as a dict display's key.
DENIED_STRINGS = DENIED_ATTRIBUTES | MODULE_ATTRIBUTES | PROCESS_ATTRIBUTES

# The functions of the package exempt from the rules, or that they rely on; both pinned.
EXEMPT = ("commands", "_import_package_modules")
REGISTRY = ("commands", "command")
COMMANDS = ("commands", "COMMANDS")
# The framed dispatcher, which runs the command a request names: pinned whole.
MAIN = "__main__"

RUNS_PACKAGE = re.compile(r"-m\s*" + PACKAGE + r"(?![A-Za-z0-9_])")
MENTION = re.compile(r"(?<![A-Za-z0-9_])" + PACKAGE + r"(?![A-Za-z0-9_])([./\\]([A-Za-z_]\w*)?)?")
PY_FILE = re.compile(r"(\w*)\.py(?![A-Za-z0-9_])")
LOOKS_LIKE_IMPORT = re.compile(
    r"(?:^|[\n;])[ \t]*(?:import[ \t]+[A-Za-z_]|from[ \t]+[A-Za-z_.][\w.]*[ \t]+import\b)")
# `from x import y` anywhere, as in a "-c" option joined with its code ("-cfrom x import y").
FROM_IMPORT = re.compile(r"from[ \t]+[A-Za-z_.][\w.]*[ \t]+import\b")

FUNCTIONS = (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda)
COMPREHENSIONS = (ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp)


def dotted(node):
    """`a.b.c` of a chain of Name and Attribute nodes, or None."""
    parts = []
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if not isinstance(node, ast.Name):
        return None
    parts.append(node.id)
    return ".".join(reversed(parts))


def chain_root(node):
    while isinstance(node, ast.Attribute):
        node = node.value
    return node if isinstance(node, ast.Name) else None


def is_docstring(node):
    return isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) \
        and isinstance(node.value.value, str)


def docstrings(tree):
    """The ids of the docstring nodes of the module, its classes and its functions."""
    out = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.body and is_docstring(node.body[0]):
                out.add(id(node.body[0].value))
    return out


def parents_of(tree):
    parents = {}
    for node in ast.walk(tree):
        for sub in ast.iter_child_nodes(node):
            parents[id(sub)] = node
    return parents


def bound_names(tree, skip=frozenset()):
    """How many times each name is bound anywhere in `tree` (outside the nodes in `skip`)."""
    counts = {}
    for node in ast.walk(tree):
        if id(node) not in skip:
            for name in binds(node):
                counts[name] = counts.get(name, 0) + 1
    return counts


def binds(node):
    """The names node `node` binds where it is."""
    if isinstance(node, ast.Name) and not isinstance(node.ctx, ast.Load):
        return [node.id]
    if isinstance(node, ast.arg):
        return [node.arg]
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
        return [node.name]
    if isinstance(node, ast.alias):
        return [node.asname or node.name.split(".")[0]]
    if isinstance(node, ast.ExceptHandler) and node.name:
        return [node.name]
    if isinstance(node, (ast.Global, ast.Nonlocal)):
        return list(node.names)
    if isinstance(node, (ast.MatchAs, ast.MatchStar)) and node.name:
        return [node.name]
    if isinstance(node, ast.MatchMapping) and node.rest:
        return [node.rest]
    return []


def pin(node):
    """The SHA-256 of a function's syntax tree, without its docstring."""
    node = copy.deepcopy(node)
    if node.body and is_docstring(node.body[0]):
        node.body = node.body[1:] or [ast.Pass()]
    return hashlib.sha256(ast.dump(node).encode("utf-8")).hexdigest()


def module_pin(tree):
    """The SHA-256 of a module's syntax tree, without the docstrings of the module, its
    classes and its functions."""
    tree = copy.deepcopy(tree)
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) \
                and node.body and is_docstring(node.body[0]):
            node.body = node.body[1:] or [ast.Pass()]
    return hashlib.sha256(ast.dump(tree).encode("utf-8")).hexdigest()


class Module:
    """A top-level module of the package, and what the analysis found in it."""

    def __init__(self, name, siblings, tree):
        self.name = name
        self.siblings = siblings
        self.tree = tree
        self.scope = None
        self.edges = set()
        self.commands = []
        # (anchor node in the module's tree or None, reason).
        self.fallback = []
        self.global_ = []
        self.impure_statements = set()
        # Top-level definitions: name -> the statements that bind it at the top level.
        self.defs = {}
        # Top-level name -> what it refers to ((module, name) targets); and the targets of
        # the parts of the top level that run at import.
        self.refs = {}
        self.import_refs = set()
        self.top_refs = set()
        self.child_scopes = []
        self.scripts = {}
        self.exempt = set()
        self.analyzed = set()
        if tree is None:
            return
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                self.defs.setdefault(node.name, []).append(node)
            elif not isinstance(node, (ast.Import, ast.ImportFrom)):
                for sub in ast.walk(node):
                    if isinstance(sub, ast.Name) and isinstance(sub.ctx, ast.Store):
                        self.defs.setdefault(sub.id, []).append(node)
        if name == EXEMPT[0]:
            for node in tree.body:
                if isinstance(node, ast.FunctionDef) and node.name == EXEMPT[1]:
                    self.exempt.update(id(n) for n in ast.walk(node))

    def fail(self, node, why, anchor=None):
        anchor = anchor if anchor is not None else node
        line = getattr(node, "lineno", 0)
        self.fallback.append((anchor, f"{self.name}.py:{line}: {why}"))

    def impure(self, node, why):
        self.impure_statements.add(id(node))
        self.global_.append(f"{self.name}.py:{getattr(node, 'lineno', 0)}: {why}")


class Scope:
    """A module's code, or a child's source (`child`, anchored at the string's node in the
    module), and what its names bind where they are used: ("module", sibling),
    ("name", sibling, name), ("external", dotted path), ("forbidden", dotted path),
    ("package",), ("local",), or ("",) for a name bound otherwise or not at all."""

    def __init__(self, module, tree, child, anchor=None):
        self.module = module
        self.tree = tree
        self.child = child
        self.anchor = anchor
        self.parents = parents_of(tree)
        self.skip = module.exempt if not child else set()
        self.bound = bound_names(tree, self.skip)
        # (id of the function, comprehension or module, name) -> [binding].
        self.imports = {}
        self.locals_cache = {}

    def fail(self, node, why):
        self.module.fail(node, why, self.anchor)

    def bind(self, node, local, binding):
        key = (id(self.scope_of(node)), local)
        self.imports.setdefault(key, [])
        if binding not in self.imports[key]:
            self.imports[key].append(binding)
        if len(self.imports[key]) > 1:
            self.fail(node, f"two imports bind {local} differently")

    def scope_of(self, node):
        """The function, lambda or comprehension whose names `node` uses, or the tree."""
        child = node
        current = self.parents.get(id(node))
        while current is not None:
            if isinstance(current, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if any(child is s for s in current.body):
                    return current
            elif isinstance(current, ast.Lambda):
                if child is current.body:
                    return current
            elif isinstance(current, ast.comprehension):
                comprehension = self.parents[id(current)]
                # The first iterable is evaluated in the enclosing scope.
                if child is current.iter and comprehension.generators[0] is current:
                    child = comprehension
                    current = self.parents.get(id(child))
                    continue
                return comprehension
            elif isinstance(current, COMPREHENSIONS):
                return current
            child = current
            current = self.parents.get(id(current))
        return self.tree

    def locals_of(self, scope):
        """The names local to function, lambda or comprehension `scope`."""
        key = id(scope)
        if key in self.locals_cache:
            return self.locals_cache[key]
        names = set()
        declared = set()
        if isinstance(scope, FUNCTIONS):
            args = scope.args
            for arg in args.posonlyargs + args.args + args.kwonlyargs:
                names.add(arg.arg)
            for arg in (args.vararg, args.kwarg):
                if arg is not None:
                    names.add(arg.arg)
        for node in ast.walk(scope):
            if node is scope or isinstance(node, ast.arg):
                continue
            if isinstance(node, (ast.Global, ast.Nonlocal)):
                if self.scope_of(node) is scope:
                    declared.update(node.names)
                continue
            for name in binds(node):
                if self.binding_scope(node) is scope:
                    names.add(name)
        names -= declared
        self.locals_cache[key] = names
        return names

    def binding_scope(self, node):
        """The scope a binding node binds in: a def's or class's name binds in the scope
        around it; a walrus's target in the nearest function."""
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            return self.scope_of(node)
        scope = self.scope_of(node)
        if isinstance(self.parents.get(id(node)), ast.NamedExpr):
            while isinstance(scope, COMPREHENSIONS):
                scope = self.scope_of(scope)
        return scope

    def binding_at(self, name, node):
        """What `name` binds where `node` uses it (the module's top level for None)."""
        scope = self.scope_of(node) if node is not None else self.tree
        while scope is not self.tree:
            if name in self.locals_of(scope):
                found = self.imports.get((id(scope), name))
                return found[0] if found else ("local",)
            scope = self.scope_of(scope)
        found = self.imports.get((id(self.tree), name))
        return found[0] if found else ("",)


def collect_imports(scope):
    """The edges and bindings of every import of `scope`, and their fallback reasons."""
    for node in ast.walk(scope.tree):
        if id(node) in scope.skip or not isinstance(node, (ast.Import, ast.ImportFrom)):
            continue
        if isinstance(node, ast.ImportFrom):
            import_from(scope, node)
        else:
            for alias in node.names:
                import_name(scope, node, alias)


def sibling_from(scope, node, sibling, names):
    """`from <sibling> import names`."""
    module = scope.module
    if sibling not in module.siblings:
        scope.fail(node, f"imports from {sibling!r}, which isn't a module of the package")
        return
    module.edges.add(sibling)
    for alias in names:
        if alias.name == "*":
            scope.fail(node, "imports *")
        else:
            scope.bind(node, alias.asname or alias.name, ("name", sibling, alias.name))


def import_from(scope, node):
    module = scope.module
    name = node.module
    if node.level > 1:
        scope.fail(node, "imports from above the package")
    elif node.level == 1 and scope.child:
        scope.fail(node, "a child's source imports relatively")
    elif node.level == 1 and name is not None and "." in name:
        scope.fail(node, f"imports from a subpackage {name!r}")
    elif node.level == 1 and name is not None:
        sibling_from(scope, node, name, node.names)
    elif node.level == 1 or name == PACKAGE:
        for alias in node.names:
            local = alias.asname or alias.name
            if alias.name == "*":
                scope.fail(node, "imports *")
            elif alias.name in module.siblings:
                module.edges.add(alias.name)
                scope.bind(node, local, ("module", alias.name))
            else:
                module.edges.add("__init__")
                scope.bind(node, local, ("name", "__init__", alias.name))
    elif name.startswith(PACKAGE + "."):
        rest = name[len(PACKAGE) + 1:]
        if "." in rest:
            scope.fail(node, f"imports from a subpackage {name!r}")
        else:
            sibling_from(scope, node, rest, node.names)
    elif name.split(".")[0] in module.siblings:
        if "." in name:
            scope.fail(node, f"imports from a subpackage {name!r}")
        else:
            sibling_from(scope, node, name, node.names)
    else:
        for alias in node.names:
            if alias.name == "*":
                scope.fail(node, "imports *")
            else:
                bind_external(scope, node, alias.asname or alias.name, f"{name}.{alias.name}")


def import_name(scope, node, alias):
    module = scope.module
    top = alias.name.split(".")[0]
    if top == PACKAGE:
        rest = alias.name[len(PACKAGE) + 1:]
        if not rest:
            module.edges.add("__init__")
        elif "." in rest:
            scope.fail(node, f"imports a subpackage {alias.name!r}")
            return
        elif rest not in module.siblings:
            scope.fail(node, f"imports {rest!r}, which isn't a module of the package")
            return
        else:
            module.edges.add(rest)
        if alias.asname and rest:
            scope.bind(node, alias.asname, ("module", rest))
        else:
            scope.bind(node, alias.asname or PACKAGE, ("package",))
    elif top in module.siblings:
        module.edges.add(top)
        if "." in alias.name:
            scope.fail(node, f"imports a subpackage {alias.name!r}")
        else:
            scope.bind(node, alias.asname or top, ("module", top))
    elif alias.asname:
        bind_external(scope, node, alias.asname, alias.name)
    else:
        bind_external(scope, node, top, top)


def bind_external(scope, node, local, path):
    """An import of a module outside the package: one outside IMPORTABLE is a fallback
    reason; one outside USABLE is a fallback reason wherever its name is used."""
    top = path.split(".")[0]
    if top not in IMPORTABLE:
        scope.fail(node, f"imports {path}, which isn't in IMPORTABLE")
    scope.bind(node, local, ("external" if top in USABLE else "forbidden", path))


def resolve_name(modules, sibling, name, depth=0):
    """The definition `name` of `sibling` names, ("def", module, name), following the names
    the sibling imports from its siblings at its top level; ("module", m) for a module; or
    None."""
    module = modules.get(sibling)
    if module is None or module.scope is None or depth > len(modules):
        return None
    if name in module.defs:
        return ("def", sibling, name)
    binding = module.scope.binding_at(name, None)
    if binding[0] == "name":
        return resolve_name(modules, binding[1], binding[2], depth + 1)
    if binding[0] == "module":
        return ("module", binding[1])
    return None


def external_path(scope, node):
    """The dotted path of a Name or Attribute chain rooted at a module outside the package
    (USABLE or not), or None."""
    root = chain_root(node)
    if root is None:
        return None
    binding = scope.binding_at(root.id, root)
    if binding[0] not in ("external", "forbidden"):
        return None
    rest = dotted(node)[len(root.id):]
    return binding[1] + rest


def is_base(scope, node):
    """Whether `node` is (the root of) a class's base."""
    while True:
        parent = scope.parents.get(id(node))
        if isinstance(parent, ast.Attribute) and parent.value is node:
            node = parent
            continue
        return isinstance(parent, ast.ClassDef) and any(b is node for b in parent.bases)


def check_external(scope, node, path):
    """The fallback reasons of a use of the external `path` (a Name or an Attribute)."""
    parts = path.split(".")
    top = parts[0]
    parent = scope.parents.get(id(node))
    if top not in STRICT:
        return
    if len(parts) == 1:
        if not (isinstance(parent, ast.Attribute) and parent.value is node):
            scope.fail(node, f"uses {top} other than as {top}.<name>")
        return
    attr = parts[1]
    if top == "sys":
        if attr == "path":
            call = scope.parents.get(id(parent))
            if not (scope.child and len(parts) == 2 and isinstance(parent, ast.Attribute)
                    and parent.attr == "insert" and isinstance(call, ast.Call)
                    and call.func is parent):
                scope.fail(node, "uses sys.path")
        elif attr == "executable":
            if not argv_head(scope, node):
                scope.fail(node, "uses sys.executable other than first in the arguments of "
                                 "a subprocess call")
        elif attr == "argv":
            # The oracle's own arguments: the first is `__main__.py`'s path, in the package's
            # directory. Only `__main__.py`, pinned whole, reads them.
            if scope.child or scope.module.name != MAIN:
                scope.fail(node, "uses sys.argv")
        elif attr not in SYS_ATTRIBUTES:
            scope.fail(node, f"uses sys.{attr}")
    elif top in ("os", "nt", "posix"):
        if attr in PROCESS_ATTRIBUTES:
            scope.fail(node, f"uses {top}.{attr}")
    elif top == "subprocess":
        if attr in SUBPROCESS_CALLS and len(parts) == 2:
            if not (isinstance(parent, ast.Call) and parent.func is node):
                scope.fail(node, f"uses subprocess.{attr} other than in a call")
            else:
                check_argv(scope, parent)
        elif attr[:1].islower():
            scope.fail(node, f"uses subprocess.{attr}")


def argv_head(scope, node):
    """Whether `node` is the first item of the list of arguments of a subprocess call."""
    parent = scope.parents.get(id(node))
    if not isinstance(parent, ast.List) or not parent.elts or parent.elts[0] is not node:
        return False
    return subprocess_call(scope, parent) is not None


def subprocess_call(scope, argv):
    """The subprocess call whose list of arguments is `argv`, or None."""
    call = scope.parents.get(id(argv))
    if isinstance(call, ast.keyword):
        if call.arg != "args":
            return None
        call = scope.parents.get(id(call))
    elif not (isinstance(call, ast.Call) and call.args and call.args[0] is argv):
        return None
    if not isinstance(call, ast.Call):
        return None
    path = external_path(scope, call.func)
    if path is None or path.split(".")[0] != "subprocess" \
            or path.split(".")[-1] not in SUBPROCESS_CALLS:
        return None
    return call


def check_argv(scope, call):
    """The fallback reasons of a subprocess call `call` (see the module's docstring)."""
    module = scope.module
    argv = call.args[0] if call.args else None
    for keyword in call.keywords:
        if keyword.arg == "args":
            argv = keyword.value
        elif keyword.arg == "shell" and not (isinstance(keyword.value, ast.Constant)
                                             and keyword.value.value is False):
            scope.fail(call, "a subprocess call through the shell")
        elif keyword.arg is None:
            scope.fail(call, "a subprocess call with ** arguments")
        elif keyword.arg == "executable":
            scope.fail(call, "a subprocess call with executable=")
    if not isinstance(argv, ast.List) or not argv.elts:
        scope.fail(call, "a subprocess call whose arguments aren't a list display")
        return
    if any(isinstance(e, ast.Starred) for e in argv.elts):
        scope.fail(call, "a subprocess call with * arguments")
        return
    elts = argv.elts
    strings = [e.value for e in elts if isinstance(e, ast.Constant)]
    if external_path(scope, elts[0]) != "sys.executable":
        # "-c", alone or joined with other options or with the code ("-Ic", "-cimport x").
        if any(isinstance(s, str) and s[:1] == "-" and s[1:2] != "-" and "c" in s
               for s in strings):
            scope.fail(call, "a subprocess call runs Python code other than with "
                             "sys.executable")
        return
    i = 1
    while i < len(elts):
        item = elts[i]
        value = item.value if isinstance(item, ast.Constant) else None
        if isinstance(value, str) and value in VALUE_OPTIONS:
            if not (i + 1 < len(elts) and isinstance(elts[i + 1], ast.Constant)):
                scope.fail(call, f"the option {value} without a constant value")
                return
            i += 2
            continue
        if isinstance(value, str) and joined_code_option(value):
            scope.fail(call, f"the option {value!r} runs code the analysis can't read")
            return
        if value == "-c":
            source = child_source(scope, elts[i + 1] if i + 1 < len(elts) else None)
            if source is None:
                scope.fail(call, "a child's source that isn't a constant string, or a name "
                                 "bound only once, at the top level, to one")
            else:
                analyze_child(module, source[0], source[1], scope)
            return
        if isinstance(value, str) and value.startswith("-") and value != "-m":
            i += 1
            continue
        script = script_path(scope, item)
        if script is None:
            scope.fail(call, "runs a script other than a sibling's file in the package's "
                             "directory")
        else:
            module.scripts.setdefault(script, call)
        return
    scope.fail(call, "a subprocess call of sys.executable without code")


def joined_code_option(value):
    """Whether the interpreter's option `value` (one argument) reads its program from stdin
    ("-"), or joins -c or -m with other options or with the code ("-Ic", "-cimport x"), as
    Python reads it: the letters after "-" up to an -X or -W, whose value is the rest."""
    if value == "-":
        return True
    if value[:1] != "-" or value[1:2] == "-" or value in ("-c", "-m"):
        return False
    for letter in value[1:]:
        if letter in "XW":
            return False
        if letter in "cm":
            return True
    return False


def top_level_value(scope, name):
    """The value of the only binding of `name`, a top-level `name = value`, or None."""
    if scope.child or scope.bound.get(name, 0) != 1:
        return None
    for node in scope.tree.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 \
                and isinstance(node.targets[0], ast.Name) and node.targets[0].id == name:
            return node.value
    return None


def child_source(scope, node):
    """The code and node of a child's source given as `node`, or None."""
    if isinstance(node, ast.Name):
        node = top_level_value(scope, node.id)
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value, node
    return None


def script_path(scope, node):
    """The sibling `node` names as a script: the package's directory joined with a sibling's
    file name, directly or through a name bound only once, at the top level, to it."""
    if scope.child:
        return None
    if isinstance(node, ast.Name):
        node = top_level_value(scope, node.id)
    return package_join(scope, node) if node is not None else None


def file_shape(scope, node):
    """"package" for `os.path.dirname(os.path.abspath(__file__))`, "oracle" for the
    `os.path.dirname` of that, else None."""
    def argument_of(node, name):
        if isinstance(node, ast.Call) and external_path(scope, node.func) == name \
                and len(node.args) == 1 and not node.keywords:
            return node.args[0]
        return None

    inner = argument_of(node, "os.path.dirname")
    if inner is None:
        return None
    absolute = argument_of(inner, "os.path.abspath")
    if absolute is not None:
        return "package" if isinstance(absolute, ast.Name) and absolute.id == "__file__" \
            and scope.binding_at("__file__", absolute) == ("",) else None
    return "oracle" if file_shape(scope, inner) == "package" else None


def package_join(scope, node):
    """The sibling of `os.path.join(<the package's directory>, "<sibling>.py")`, or None."""
    if not (isinstance(node, ast.Call) and external_path(scope, node.func) == "os.path.join"
            and len(node.args) == 2 and not node.keywords
            and file_shape(scope, node.args[0]) == "package"
            and isinstance(node.args[1], ast.Constant) and isinstance(node.args[1].value, str)
            and node.args[1].value.endswith(".py")
            and node.args[1].value[:-3] in scope.module.siblings):
        return None
    return node.args[1].value[:-3]


def oracle_dir_sinks(scope, name, within, seen):
    """Whether every use of `name` in `within` (a function, or the module) is a sink: a value
    of a dict display, an item of a list of a subprocess call's arguments, an argument of
    `os.path.samefile`, or an argument of a top-level function whose parameter is used so."""
    stores = 0
    for node in ast.walk(within):
        if not isinstance(node, ast.Name) or node.id != name:
            continue
        if not isinstance(node.ctx, ast.Load):
            stores += 1
            continue
        parent = scope.parents.get(id(node))
        if isinstance(parent, ast.Dict) and any(v is node for v in parent.values):
            continue
        if isinstance(parent, ast.List) and parent.elts and parent.elts[0] is not node \
                and subprocess_call(scope, parent) is not None:
            continue
        if isinstance(parent, ast.Call) and any(a is node for a in parent.args):
            if external_path(scope, parent.func) == "os.path.samefile":
                continue
            index = next(i for i, a in enumerate(parent.args) if a is node)
            if isinstance(parent.func, ast.Name) \
                    and not any(isinstance(a, ast.Starred) for a in parent.args[:index]) \
                    and parameter_sinks(scope, parent.func.id, index, seen):
                continue
        return False
    return stores <= 1


def parameter_sinks(scope, function, index, seen):
    if (function, index) in seen:
        return True
    seen.add((function, index))
    defs = scope.module.defs.get(function, [])
    if len(defs) != 1 or scope.bound.get(function, 0) != 1 \
            or not isinstance(defs[0], ast.FunctionDef):
        return False
    params = defs[0].args.posonlyargs + defs[0].args.args
    if index >= len(params):
        return False
    return oracle_dir_sinks(scope, params[index].arg, defs[0], seen)


def accepted_file_uses(scope):
    """The nodes of the accepted uses of `__file__` (see the module's docstring)."""
    accepted = set()
    for node in ast.walk(scope.tree):
        if id(node) in scope.skip:
            continue
        if package_join(scope, node) is not None:
            accepted.update(id(n) for n in ast.walk(node.args[0]))
        elif file_shape(scope, node) == "oracle":
            parent = scope.parents.get(id(node))
            if isinstance(parent, ast.Assign) and len(parent.targets) == 1 \
                    and isinstance(parent.targets[0], ast.Name):
                within = scope.scope_of(parent)
                if oracle_dir_sinks(scope, parent.targets[0].id, within, set()):
                    accepted.update(id(n) for n in ast.walk(node))
    return accepted


def attribute_name_ok(scope, node, name, external):
    """The fallback reasons of the attribute `name` (of a module outside the package:
    `external`)."""
    if name.startswith("__") and name.endswith("__"):
        # A module's `__dict__` is its namespace: `os.__dict__["system"]`.
        if not (name in DUNDER_ATTRIBUTES and not (name == "__dict__" and external)
                or name == "__file__" and external):
            scope.fail(node, f"uses the attribute {name}")
    elif name in DENIED_ATTRIBUTES or name in MODULE_ATTRIBUTES \
            or name in PROCESS_ATTRIBUTES:
        scope.fail(node, f"uses the attribute {name}")


def is_command(scope, node):
    """Whether decorator `node` is the oracle's `command`."""
    if isinstance(node, ast.Name):
        binding = scope.binding_at(node.id, node)
        if scope.module.name == REGISTRY[0] and node.id == REGISTRY[1] and not scope.child \
                and binding == ("",):
            return True
        return binding == ("name",) + REGISTRY
    if isinstance(node, ast.Attribute) and node.attr == REGISTRY[1] \
            and isinstance(node.value, ast.Name):
        return scope.binding_at(node.value.id, node.value) == ("module", REGISTRY[0])
    return False


def analyze_code(modules, scope):
    """The fallback reasons of the names, attributes, calls, strings and `__file__` uses of
    `scope`'s code, and its children's sources."""
    docs = docstrings(scope.tree)
    accepted_file = accepted_file_uses(scope) if not scope.child else set()
    for node in ast.walk(scope.tree):
        if id(node) in scope.skip:
            continue
        parent = scope.parents.get(id(node))
        if isinstance(node, ast.Name):
            check_name(modules, scope, node, parent, accepted_file)
        elif isinstance(node, ast.Attribute):
            # `<module>.<name>`: the attribute's name; and those of the rest of the chain.
            external = external_path(scope, node.value) is not None
            if not (isinstance(node.value, ast.Name)
                    and scope.binding_at(node.value.id, node.value)[0] == "module"):
                attribute_name_ok(scope, node, node.attr, external)
            if isinstance(node.value, ast.Name) and external:
                check_external(scope, node, external_path(scope, node))
        elif isinstance(node, ast.Call):
            check_call(scope, node)
        elif isinstance(node, ast.Constant) and id(node) not in docs:
            check_string(scope, node, parent)
        elif isinstance(node, ast.JoinedStr):
            check_fstring(scope, node)


def check_name(modules, scope, node, parent, accepted_file):
    module = scope.module
    name = node.id
    binding = scope.binding_at(name, node)
    if binding[0] == "local":
        return
    if name in DENIED_NAMES:
        scope.fail(node, f"uses {name}")
    elif name == "vars" and not (isinstance(parent, ast.Call) and parent.func is node
                                 and len(parent.args) == 1 and not parent.keywords):
        scope.fail(node, "uses vars other than with one argument")
    elif name == "__file__":
        if scope.child or id(node) not in accepted_file:
            scope.fail(node, "uses __file__ other than as the package's directory joined "
                             "with a sibling's file name, or as the oracle's directory "
                             "passed on")
    elif name == PACKAGE or binding[0] == "package":
        scope.fail(node, f"uses the package {PACKAGE} other than in an import")
    elif binding[0] == "module":
        sibling = modules.get(binding[1])
        if not (isinstance(parent, ast.Attribute) and parent.value is node):
            scope.fail(node, f"uses the module {binding[1]} other than as "
                             f"{binding[1]}.<name>")
        elif sibling is None or parent.attr not in sibling.defs \
                or (binding[1], parent.attr) == EXEMPT:
            scope.fail(node, f"uses {binding[1]}.{parent.attr}, which {binding[1]}.py "
                             "doesn't define")
    elif binding[0] == "external":
        check_external(scope, node, binding[1])
    elif binding[0] == "forbidden":
        if not is_base(scope, node):
            scope.fail(node, f"uses {binding[1]}, which isn't in USABLE")
    elif binding[0] == "name":
        target = resolve_name(modules, binding[1], binding[2])
        if target is None or target[0] == "module":
            scope.fail(node, f"imports {binding[2]} from {binding[1]}.py, which doesn't "
                             "define it")
        elif target[1:] == EXEMPT:
            scope.fail(node, f"uses {EXEMPT[1]}")
    elif module.name == EXEMPT[0] and name == EXEMPT[1] and not scope.child:
        statement = scope.parents.get(id(parent))
        if not (isinstance(parent, ast.Call) and parent.func is node and not parent.args
                and isinstance(statement, ast.Expr)
                and any(statement is s for s in scope.tree.body)):
            scope.fail(node, f"uses {EXEMPT[1]} other than in the call at the top level")


def check_call(scope, node):
    func = node.func
    if isinstance(func, ast.Name) and func.id in GETATTR_LIKE \
            and scope.binding_at(func.id, func) == ("",) and len(node.args) > 1 \
            and isinstance(node.args[1], ast.Constant) and isinstance(node.args[1].value, str):
        attribute_name_ok(scope, node, node.args[1].value,
                          external_path(scope, node.args[0]) is not None)
    path = external_path(scope, func)
    if path in ("operator.attrgetter", "operator.methodcaller"):
        for arg in node.args[:1] if path == "operator.methodcaller" else node.args:
            if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                for part in arg.value.split("."):
                    attribute_name_ok(scope, node, part, False)


def mentions(text):
    """Whether `text` names the package with a separator, or a `.py` file."""
    return any(m.group(1) for m in MENTION.finditer(text)) or PY_FILE.search(text) is not None


def check_string(scope, node, parent):
    """The edges and fallback reasons of a constant (see the module's docstring)."""
    module = scope.module
    value = node.value
    if isinstance(value, bytes):
        text = value.decode("latin-1")
        if mentions(text) or MENTION.search(text) or LOOKS_LIKE_IMPORT.search(text):
            scope.fail(node, f"a bytes constant names {PACKAGE}, a .py file, or an import")
        return
    if not isinstance(value, str) or isinstance(parent, ast.JoinedStr):
        return
    denied = value in DENIED_STRINGS or len(value) > 4 and value.startswith("__") \
        and value.endswith("__") and value not in DUNDER_ATTRIBUTES
    # Compared, or in a table: a tuple, list or set display, or a dict display's key.
    if denied and not (isinstance(parent, (ast.Compare, ast.Tuple, ast.List, ast.Set))
                       or isinstance(parent, ast.Dict) and any(k is node for k in parent.keys)):
        scope.fail(node, f"the string {value!r} other than compared or in a table")
    if value == "-m" or RUNS_PACKAGE.search(value):
        scope.fail(node, "runs a module by name (-m)")
    if scope.child and (MENTION.search(value) or PY_FILE.search(value)):
        scope.fail(node, f"a child's source names {PACKAGE} or a .py file in a string")
        return
    try:
        code = ast.parse(value)
    except (SyntaxError, ValueError):
        code = None
    if code is not None and any(isinstance(n, (ast.Import, ast.ImportFrom))
                                for n in ast.walk(code)):
        analyze_child(module, value, node, scope)
    elif code is None and (LOOKS_LIKE_IMPORT.search(value) or FROM_IMPORT.search(value)):
        scope.fail(node, "a string looks like code with an import but doesn't parse")
    building = isinstance(parent, (ast.BinOp, ast.Attribute)) or (
        isinstance(parent, ast.Call) and any(a is node for a in parent.args)
        and external_path(scope, parent.func) == "os.path.join"
        and package_join(scope, parent) is None)
    for match in MENTION.finditer(value):
        separator, name = match.group(1), match.group(2)
        if separator is None:
            # At the end of the string, the package's name is its directory's (the oracle
            # runs from the oracle's directory): `os.listdir("ocio_oracle")`.
            if match.end() == len(value):
                scope.fail(node, f"the string names {PACKAGE}'s directory")
            elif building:
                scope.fail(node, f"builds a name from {PACKAGE}")
            module.edges.add("__init__")
        elif name in module.siblings and not building:
            module.edges.add(name)
        else:
            scope.fail(node, f"the string names {match.group(0)!r}, which isn't resolved")
    for match in PY_FILE.finditer(value):
        stem = match.group(1)
        if stem in module.siblings:
            module.edges.add(stem)
        elif not stem or building:
            scope.fail(node, "builds the name of a .py file")


def check_fstring(scope, node):
    for part in node.values:
        if isinstance(part, ast.Constant) and isinstance(part.value, str):
            text = part.value
            if LOOKS_LIKE_IMPORT.search(text) or FROM_IMPORT.search(text) or mentions(text) \
                    or RUNS_PACKAGE.search(text) or text.endswith(PACKAGE):
                scope.fail(node, f"an f-string names {PACKAGE}, a .py file, or an import")


def analyze_child(module, source, node, scope):
    """A child's source: its imports are edges, and it follows the rules of the module's code,
    in a scope of its own, anchored at the string's node in the module."""
    if id(node) in module.analyzed:
        return
    module.analyzed.add(id(node))
    anchor = scope.anchor if scope.anchor is not None else node
    try:
        code = ast.parse(source)
    except (SyntaxError, ValueError):
        module.fail(node, "a child's source doesn't parse", anchor)
        return
    child = Scope(module, code, child=True, anchor=anchor)
    collect_imports(child)
    module.child_scopes.append(child)


def targets_of(modules, scope, nodes, skip=frozenset()):
    """The top-level (module, name) definitions `nodes` of `scope` refer to; COMMANDS for any
    name, attribute or string "COMMANDS"."""
    module = scope.module
    out = set()
    for top in nodes:
        for node in ast.walk(top):
            if id(node) in skip:
                continue
            if isinstance(node, ast.Constant) and node.value == COMMANDS[1]:
                out.add(COMMANDS)
            elif isinstance(node, ast.Name) and isinstance(node.ctx, ast.Load):
                if node.id == COMMANDS[1]:
                    out.add(COMMANDS)
                binding = scope.binding_at(node.id, node)
                if binding[0] == "name":
                    target = resolve_name(modules, binding[1], binding[2])
                    out.add(target[1:] if target and target[0] == "def"
                            else (binding[1], binding[2]))
                elif binding == ("",) and not scope.child and node.id in module.defs:
                    out.add((module.name, node.id))
            elif isinstance(node, ast.Attribute):
                if node.attr == COMMANDS[1]:
                    out.add(COMMANDS)
                if isinstance(node.value, ast.Name):
                    binding = scope.binding_at(node.value.id, node.value)
                    if binding[0] == "module":
                        target = resolve_name(modules, binding[1], node.attr)
                        out.add(target[1:] if target and target[0] == "def"
                                else (binding[1], node.attr))
    return out


# --- Top-level purity ---------------------------------------------------------------------


# The kinds of an inert value. DATA runs no code of the package when it is used: builtin data,
# a module outside the package and its attributes, what PURE_CALLS, PURE_EXTERNAL_CALLS and
# INERT_METHODS return. OUTSIDE is DATA that is a module outside the package or one of its
# attributes, under any name: INERT_METHODS are builtin data's, not its. CODE is, or may hold,
# the package's code: a function, a class, a lambda, and what holds one. HELD is, or may hold,
# a one-shot iterator that other code may share. CODE and HELD may only be held (by a name, a
# display or a dict's value). FRESH is an iterator made of DATA where it is used, which shares
# nothing with other code until it is held: it is DATA where it is consumed on the spot (a
# consuming call's iterable, an iterator call's argument, a comprehension's iterable, a `*`
# item, an unpacking assignment's value), and HELD anywhere else.
DATA = "data"
OUTSIDE = "outside"
FRESH = "fresh"
CODE = "code"
HELD = "held"
PLAIN = (DATA, OUTSIDE)


def outside(path):
    """The kind of the module outside the package, or the attribute of one, `path`: HELD for
    the process's streams (STREAM_NAMES)."""
    parts = path.split(".")
    if parts == ["sys"] or "sys" in parts[1:] or STREAM_NAMES.intersection(parts):
        return HELD
    return OUTSIDE


def class_hooks(node):
    """Whether class `node` defines, or assigns, one of CLASS_HOOKS."""
    names = set()
    for item in node.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(item.name)
        elif isinstance(item, (ast.Assign, ast.AnnAssign)):
            targets = item.targets if isinstance(item, ast.Assign) else [item.target]
            names.update(n.id for t in targets for n in ast.walk(t) if isinstance(n, ast.Name))
    return bool(names & CLASS_HOOKS)


class Purity:
    """Whether a module's top level only defines names (see the module's docstring)."""

    def __init__(self, modules, module, resolving=None):
        self.modules = modules
        self.module = module
        self.scope = module.scope
        # The module's classes so far: name -> (whether it defines one of CLASS_HOOKS, whether
        # a base outside PLAIN_BASES is among its ancestors).
        self.classes = {}
        self.resolving = set() if resolving is None else resolving

    def unbound(self, name):
        return self.scope.bound.get(name, 0) == 0

    def name_kind(self, node, local):
        name = node.id
        if name in local or name in SAFE_BUILTINS and self.unbound(name):
            return DATA
        binding = self.scope.binding_at(name, None)
        if binding[0] in ("external", "forbidden"):
            return outside(binding[1])
        if binding[0] == "name":
            return self.definition(binding[1], binding[2])
        if binding[0] != "":
            return None
        return self.definition(self.module.name, name)

    def definition(self, sibling, name):
        """The kind of the inert value that `name` of module `sibling` is bound to, or None."""
        key = (sibling, name)
        module = self.modules.get(sibling)
        if key in self.resolving or module is None or module.scope is None:
            return None
        self.resolving.add(key)
        try:
            defs = module.defs.get(name)
            if defs is not None:
                if len(defs) != 1 or module.scope.bound.get(name, 0) != 1:
                    return None
                node = defs[0]
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    return CODE
                if isinstance(node, ast.ClassDef):
                    return CODE if sibling == self.module.name and name in self.classes \
                        else None
                if isinstance(node, ast.Assign) and len(node.targets) == 1 \
                        and isinstance(node.targets[0], ast.Name):
                    purity = self if module is self.module \
                        else Purity(self.modules, module, self.resolving)
                    kind = purity.kind(node.value, set())
                    return HELD if kind == FRESH else kind
                return None
            binding = module.scope.binding_at(name, None)
            if binding[0] in ("external", "forbidden"):
                return outside(binding[1])
            if binding[0] == "name":
                return self.definition(binding[1], binding[2])
            return None
        finally:
            self.resolving.discard(key)

    def inert(self, node, local):
        """Whether evaluating `node` (with the comprehension variables `local`) runs no code
        of the package and calls nothing outside the allowlists."""
        return self.kind(node, local) is not None

    def plain(self, node, local, consume=False):
        """Whether `node` is inert and DATA: code outside the package may use its value. An
        iterator made where it is used (FRESH) is DATA only where it is consumed on the spot
        (`consume`); anywhere else (an operand, a test, a set's item, a dict's key, a keyword's
        value, `sum`'s start, a method's argument) it is held."""
        kind = self.kind(node, local)
        return kind in PLAIN or consume and kind == FRESH

    def all_plain(self, nodes, local, consume=False):
        """DATA when the values of `nodes` are, used to make a value; else None."""
        return DATA if all(self.plain(n, local, consume) for n in nodes) else None

    def consumed(self, node):
        """The indexes of the positional arguments that call `node` consumes on the spot: the
        iterable of CONSUMING_CALLS and CONSUMING_ONE, the arguments of ITERATOR_CALLS and
        ITERATOR_EXTERNAL_CALLS (the iterator they make consumes them), and those of
        CONSUMING_METHODS."""
        func = node.func
        every = range(len(node.args))
        if isinstance(func, ast.Name) and self.unbound(func.id):
            if func.id in ITERATOR_CALLS:
                return set(every)
            if func.id in CONSUMING_CALLS or func.id in CONSUMING_ONE and len(node.args) == 1:
                return {0}
            return set()
        if external_path(self.scope, func) in ITERATOR_EXTERNAL_CALLS:
            return set(every)
        if isinstance(func, ast.Attribute) and func.attr in CONSUMING_METHODS:
            return set(every)
        return set()

    def held(self, nodes, local):
        """The kind of what holds the values of `nodes`, or None."""
        kinds = [self.kind(n, local) for n in nodes]
        if None in kinds:
            return None
        return CODE if CODE in kinds else HELD if HELD in kinds or FRESH in kinds else DATA

    def kind(self, node, local):
        """The kind of the value of `node` when evaluating it runs no code of the package and
        calls nothing outside the allowlists (with the comprehension variables `local`, which
        are DATA); else None. A value that is used (an operand, a subscript's value or index,
        an iterable, a set's item or a dict's key, a formatted value, a test, a call's argument
        or a method's object) must be DATA: the package's classes and functions run no code,
        and iterators and methods change nothing, only while they are held."""
        if node is None or isinstance(node, ast.Constant):
            return DATA
        if isinstance(node, (ast.Tuple, ast.List)):
            return self.held(node.elts, local)
        if isinstance(node, ast.Starred):
            # A `*` item is iterated on the spot.
            return self.all_plain([node.value], local, consume=True)
        if isinstance(node, ast.Set):
            return self.all_plain(node.elts, local)
        if isinstance(node, ast.Dict):
            # A key is hashed and compared, and `**x` is iterated; a value is held.
            if not all(self.plain(v if k is None else k, local)
                       for k, v in zip(node.keys, node.values)):
                return None
            return self.held([v for k, v in zip(node.keys, node.values) if k is not None],
                             local)
        if isinstance(node, ast.Name):
            return self.name_kind(node, local) if isinstance(node.ctx, ast.Load) else None
        if isinstance(node, ast.Attribute):
            # Reading an attribute of a class of the package runs no code of the package: none
            # has a metaclass, and none of their instances is made at import but the members
            # of an enum, whose class has no methods (pure_class).
            path = external_path(self.scope, node)
            if path is not None:
                return outside(path)
            if isinstance(node.value, ast.Name):
                binding = self.scope.binding_at(node.value.id, None)
                if binding[0] == "module":
                    return self.definition(binding[1], node.attr)
            # An attribute of OUTSIDE is OUTSIDE (`S.ascii_letters` after `S = string`), and one
            # of HELD is HELD (`S.stdin` after `S = sys`). One of DATA may be a method that
            # changes the data when it is called (`QUEUE.pop`): no call of the allowlists calls
            # an argument but with `key=`, and a decorator takes constants only. One of FRESH
            # holds the iterator (`zip(a, b).__next__`).
            kind = self.kind(node.value, local)
            return HELD if kind == FRESH else kind
        if isinstance(node, ast.BinOp):
            return self.all_plain([node.left, node.right], local)
        if isinstance(node, ast.UnaryOp):
            return self.all_plain([node.operand], local)
        if isinstance(node, ast.BoolOp):
            return self.all_plain(node.values, local)
        if isinstance(node, ast.Compare):
            return self.all_plain([node.left] + node.comparators, local)
        if isinstance(node, ast.IfExp):
            return self.held([node.body, node.orelse], local) \
                if self.plain(node.test, local) else None
        if isinstance(node, ast.Subscript):
            return self.all_plain([node.value, node.slice], local)
        if isinstance(node, ast.Slice):
            return self.all_plain([node.lower, node.upper, node.step], local)
        if isinstance(node, ast.JoinedStr):
            return self.all_plain(node.values, local)
        if isinstance(node, ast.FormattedValue):
            return self.all_plain([node.value, node.format_spec], local)
        if isinstance(node, ast.Lambda):
            return CODE if self.arguments(node.args, local) else None
        if isinstance(node, COMPREHENSIONS):
            inner = set(local)
            for generator in node.generators:
                if generator.is_async or not self.plain(generator.iter, inner, consume=True):
                    return None
                targets = generator.target.elts if isinstance(
                    generator.target, ast.Tuple) else [generator.target]
                if not all(isinstance(t, ast.Name) for t in targets):
                    return None
                inner |= {t.id for t in targets}
                if not all(self.plain(c, inner) for c in generator.ifs):
                    return None
            if isinstance(node, ast.DictComp):
                return self.held([node.value], inner) if self.plain(node.key, inner) else None
            if isinstance(node, ast.SetComp):
                return self.all_plain([node.elt], inner)
            elt = self.held([node.elt], inner)
            # A generator is a one-shot iterator: of DATA where it is made (FRESH) when its
            # items are DATA; else of its items' kind, which may only be held (`dict(E.A for _
            # in R)` iterates an enum's member, `dict(ids.PAIRS for _ in R)` a held iterator).
            if isinstance(node, ast.GeneratorExp):
                return FRESH if elt == DATA else elt
            return elt
        if isinstance(node, ast.Call):
            # The arguments go to code outside the package (`sorted(x, key=f)` calls `f`), which
            # consumes an iterator it is given (`dict(zip(KEYS, ids.COUNTER))`). An iterator
            # made where it is used may be consumed there, not held (`dict(pairs=zip(...))`,
            # `{}.get("k", zip(...))`, `sum((), zip(...))`, `max((), default=zip(...))`).
            consumed = self.consumed(node)
            if not all(self.plain(a, local, i in consumed) for i, a in enumerate(node.args)) \
                    or not all(k.arg not in (None, "key") and self.plain(k.value, local)
                               for k in node.keywords):
                return None
            func = node.func
            if isinstance(func, ast.Name) and self.unbound(func.id):
                if func.id in PURE_CALLS:
                    return DATA if func.id not in ("type", "vars") or len(node.args) == 1 \
                        else None
                if func.id in ITERATOR_CALLS:
                    return FRESH
            path = external_path(self.scope, func)
            if path in PURE_EXTERNAL_CALLS:
                return DATA
            if path in ITERATOR_EXTERNAL_CALLS:
                return FRESH
            # Builtin data's methods: not those of an object of a module outside the package.
            if isinstance(func, ast.Attribute) and func.attr in INERT_METHODS \
                    and self.kind(func.value, local) == DATA:
                return DATA
            return None
        return None

    def arguments(self, args, local):
        """Whether a function's defaults and annotations, evaluated where it is defined and
        then only held, are inert."""
        values = list(args.defaults) + [d for d in args.kw_defaults if d is not None]
        values += [a.annotation for a in args.posonlyargs + args.args + args.kwonlyargs]
        values += [args.vararg and args.vararg.annotation,
                   args.kwarg and args.kwarg.annotation]
        return all(self.inert(v, local) for v in values)

    def pure_decorator(self, node, in_class, properties):
        # `command` only registers what it is given (out of place, it is a fallback reason).
        if is_command(self.scope, node):
            return True
        target = node.func if isinstance(node, ast.Call) else node
        # Constants only: `functools.lru_cache(f)` decorates `f`, then calls it with the function
        # it decorates (`f = QUEUE.append`).
        if isinstance(node, ast.Call) and not (
                all(isinstance(a, ast.Constant) for a in node.args)
                and all(k.arg is not None and isinstance(k.value, ast.Constant)
                        for k in node.keywords)):
            return False
        path = external_path(self.scope, target)
        if path in PURE_DECORATORS:
            return True
        if in_class and not isinstance(node, ast.Call):
            if isinstance(node, ast.Name) and node.id in CLASS_DECORATORS \
                    and self.unbound(node.id):
                return True
            if isinstance(node, ast.Attribute) and node.attr in ("setter", "getter", "deleter") \
                    and isinstance(node.value, ast.Name) and node.value.id in properties:
                return True
        return False

    def pure_function(self, node, in_class=False, properties=()):
        if not all(self.pure_decorator(d, in_class, properties) for d in node.decorator_list):
            return False
        return self.arguments(node.args, set()) and self.inert(node.returns, set())

    def pure_class(self, node):
        """Whether making class `node` runs no code of the package: if so, whether a base
        outside PLAIN_BASES is among its ancestors; if not, None."""
        if node.keywords or node.decorator_list:
            return None
        external = False
        for base in node.bases:
            if isinstance(base, ast.Name) and base.id in SAFE_BUILTINS \
                    and self.unbound(base.id):
                external = external or base.id not in PLAIN_BASES
            elif external_path(self.scope, base) is not None:
                external = True
            elif isinstance(base, ast.Name) and base.id in self.classes \
                    and self.scope.bound.get(base.id, 0) == 1 \
                    and not self.classes[base.id][0]:
                external = external or self.classes[base.id][1]
            else:
                return None
        properties = set()
        for item in node.body:
            if isinstance(item, ast.Pass) or isinstance(item, ast.Expr) \
                    and isinstance(item.value, ast.Constant):
                continue
            if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                # Another base's metaclass may call the class's methods while it makes the
                # class: `enum.Enum`'s makes each member so.
                if external or not self.pure_function(item, True, properties):
                    return None
                if any(isinstance(d, ast.Name) and d.id == "property"
                       for d in item.decorator_list):
                    properties.add(item.name)
            elif isinstance(item, (ast.Assign, ast.AnnAssign)):
                # Such a metaclass may also call what the class holds.
                if not self.pure_assign(item, external):
                    return None
            elif isinstance(item, ast.ClassDef):
                if external or self.pure_class(item) is None:
                    return None
            else:
                return None
        return external

    def pure_assign(self, node, plain=False):
        targets = node.targets if isinstance(node, ast.Assign) else [node.target]
        unpacked = False
        for target in targets:
            unpacks = isinstance(target, (ast.Tuple, ast.List))
            names = target.elts if unpacks else [target]
            if not all(isinstance(n, ast.Name) for n in names):
                return False
            # Unpacking iterates the value: DATA (an iterator made there is consumed there), or
            # a display, whose items are only held.
            if unpacks and not isinstance(node.value, (ast.Tuple, ast.List)):
                if not self.plain(node.value, set(), consume=True):
                    return False
                unpacked = True
        check = self.plain if plain else self.inert
        # An iterator made where it is unpacked, and held by no target, is consumed there.
        value = unpacked and len(targets) == 1 or check(node.value, set())
        return value and check(getattr(node, "annotation", None), set())

    def pure_import(self, node):
        if isinstance(node, ast.ImportFrom):
            if node.level or node.module == PACKAGE \
                    or node.module.startswith(PACKAGE + ".") \
                    or node.module.split(".")[0] in self.module.siblings:
                return True
            return node.module.split(".")[0] in IMPORTABLE
        return all(a.name.split(".")[0] in IMPORTABLE | {PACKAGE} | self.module.siblings
                   for a in node.names)

    def check(self):
        module = self.module
        for node in module.tree.body:
            if isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) \
                    or isinstance(node, ast.Pass):
                continue
            if isinstance(node, (ast.Import, ast.ImportFrom)):
                if not self.pure_import(node):
                    module.impure(node, "imports a module outside IMPORTABLE")
            elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if not self.pure_function(node):
                    module.impure(node, f"the definition of {node.name} runs code")
            elif isinstance(node, ast.ClassDef):
                external = self.pure_class(node)
                if external is None:
                    module.impure(node, f"the class {node.name} runs code")
                else:
                    self.classes[node.name] = (class_hooks(node), external)
            elif isinstance(node, (ast.Assign, ast.AnnAssign)):
                if not self.pure_assign(node):
                    module.impure(node, "a top-level assignment runs code")
            elif not is_main_guard(node):
                module.impure(node, f"a top-level {type(node).__name__} statement")


def is_main_guard(node):
    """`if __name__ == "__main__":`, with no else."""
    if not isinstance(node, ast.If) or node.orelse or not isinstance(node.test, ast.Compare):
        return False
    test = node.test
    return isinstance(test.left, ast.Name) and test.left.id == "__name__" \
        and len(test.ops) == 1 and isinstance(test.ops[0], ast.Eq) \
        and isinstance(test.comparators[0], ast.Constant) \
        and test.comparators[0].value == "__main__"


def import_time_parts(statement):
    """The parts of a top-level statement evaluated when the module is imported: a function's
    decorators, defaults and annotations; a class's bases, decorators and body but its
    methods' bodies; nothing of a main guard; the rest of other statements."""
    if isinstance(statement, (ast.FunctionDef, ast.AsyncFunctionDef)):
        args = statement.args
        return statement.decorator_list + [d for d in args.defaults + args.kw_defaults if d] \
            + [a.annotation for a in args.posonlyargs + args.args + args.kwonlyargs
               + [args.vararg, args.kwarg] if a is not None and a.annotation is not None] \
            + ([statement.returns] if statement.returns else [])
    if isinstance(statement, ast.ClassDef):
        parts = statement.bases + [k.value for k in statement.keywords] \
            + statement.decorator_list
        for item in statement.body:
            parts += import_time_parts(item) if isinstance(
                item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) else [item]
        return parts
    if is_main_guard(statement):
        return []
    return [statement]


# --- The package ----------------------------------------------------------------------------


def read(package_dir, name, siblings):
    path = os.path.join(package_dir, name + ".py")
    try:
        with open(path, "rb") as f:
            tree = ast.parse(f.read(), filename=path)
    except (SyntaxError, ValueError, UnicodeDecodeError) as exc:
        module = Module(name, siblings, None)
        module.fail(None, f"doesn't parse: {exc}")
        module.impure(None, "doesn't parse")
        return module
    module = Module(name, siblings, tree)
    module.scope = Scope(module, tree, child=False)
    collect_imports(module.scope)
    return module


def analyze(modules, module):
    if module.tree is None:
        return
    scope = module.scope
    analyze_code(modules, scope)
    # Children's sources found above (and in their own strings) follow the same rules.
    done = 0
    while done < len(module.child_scopes):
        analyze_code(modules, module.child_scopes[done])
        done += 1
    Purity(modules, module).check()

    accepted = set()
    for node in module.tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.decorator_list \
                and is_command(scope, node.decorator_list[-1]):
            module.commands.append(node.name)
            accepted.update(id(n) for n in ast.walk(node.decorator_list[-1]))
    for node in ast.walk(module.tree):
        if id(node) not in accepted and isinstance(node, (ast.Name, ast.Attribute)) \
                and is_command(scope, node):
            scope.fail(node, "uses the command decorator other than as the innermost "
                             "decorator of a top-level function")

    for node in module.tree.body:
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            continue
        refs = targets_of(modules, scope, [node], accepted)
        if id(node) in module.impure_statements:
            module.import_refs |= targets_of(modules, scope, import_time_parts(node), accepted)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            module.refs[node.name] = refs
            continue
        module.top_refs |= refs
        # A name a top-level statement binds refers to what the statement does.
        for target in ast.walk(node):
            if isinstance(target, ast.Name) and isinstance(target.ctx, ast.Store):
                module.refs.setdefault(target.id, set()).update(refs)


def dispatching(modules):
    """The (module, name) of the top-level definitions that may run a command by name."""
    found = {COMMANDS}
    while True:
        more = {
            (module.name, name)
            for module in modules.values() if module.tree is not None
            for name, refs in module.refs.items()
            if (module.name, name) not in found and (module.name, name) != REGISTRY
            and refs & found
        }
        if not more:
            return found
        found |= more


def reach(modules, start):
    """The (module, name) of the top-level definitions that the targets `start` reach, through
    the names they use, at any depth and across modules (`start` included)."""
    reached = set()
    stack = list(start)
    while stack:
        target = stack.pop()
        if target in reached or target[0] not in modules:
            continue
        reached.add(target)
        stack.extend(modules[target[0]].refs.get(target[1], ()))
    return reached


def import_time(modules):
    """The (module, name) of the top-level definitions that code run at import may reach."""
    return reach(modules, [t for m in modules.values() for t in m.import_refs])


def dispatching_at_import(modules, module, found):
    """The definitions that may run a command by name, or use the registry, that the code
    `module` runs when it is imported reaches; for `__main__`, pinned whole, those but its own
    and the registry it dispatches through."""
    reached = reach(modules, module.import_refs) & found
    if module.name == MAIN:
        reached = {t for t in reached if t[0] != MAIN and t != COMMANDS}
    return sorted(reached)


def dispatcher_pin(modules, module, node, found):
    """The pin of the dispatcher `node`: its own; or, when a definition it reaches but itself
    and the registry may run a command by name, what reaches them (it isn't followed then)."""
    others = reach(modules, module.refs.get(node.name, ())) - {(module.name, node.name), COMMANDS}
    through = sorted(f"{m}.py:{n}" for m, n in others & found)
    return f"dispatches through {', '.join(through)}" if through else pin(node)


def registry_pin(module):
    """The pin of the registry's binding, `COMMANDS = {}` in `commands.py`: the SHA-256 of the
    one top-level statement that binds it, or why there isn't one."""
    statements = module.defs.get(COMMANDS[1], [])
    if len(statements) != 1 or module.scope.bound.get(COMMANDS[1], 0) != 1 \
            or isinstance(statements[0], ast.ClassDef):
        return f"{COMMANDS[1]} isn't bound once, by one top-level statement"
    return hashlib.sha256(ast.dump(statements[0]).encode("utf-8")).hexdigest()


def owner(module, node):
    """The top-level statement of `module` that `node` is in, and whether `node` is in a part
    evaluated at import (import_time_parts); (None, True) for a reason of the whole module."""
    if node is None:
        return None, True
    parents = module.scope.parents
    path = [node]
    while id(path[-1]) in parents and parents[id(path[-1])] is not module.tree:
        path.append(parents[id(path[-1])])
    statement = path[-1]
    parts = import_time_parts(statement)
    ids = {id(n) for part in parts for n in ast.walk(part)}
    return statement, id(node) in ids


def import_package(package_dir, siblings):
    """Imports every module of the package but `__main__`, with stdout on stderr; raises what
    an import raises."""
    import importlib

    sys.dont_write_bytecode = True
    sys.path.insert(0, os.path.dirname(os.path.abspath(package_dir)))
    importlib.import_module(PACKAGE)
    for name in siblings:
        if name not in ("__init__", "__main__"):
            importlib.import_module(f"{PACKAGE}.{name}")


def main(package_dir):
    siblings = sorted(
        entry[:-3] for entry in os.listdir(package_dir)
        if entry.endswith(".py") and os.path.isfile(os.path.join(package_dir, entry)))
    modules = {name: read(package_dir, name, set(siblings)) for name in siblings}
    for module in modules.values():
        analyze(modules, module)
    found = dispatching(modules)

    def runs_commands(module):
        return module.tree is not None and (
            any((module.name, n) in found for n in module.refs) or module.top_refs & found)

    for module in modules.values():
        if module.tree is not None:
            for target in dispatching_at_import(modules, module, found):
                module.fail(None, f"may run a command by name when it is imported, through "
                                  f"{target[0]}.py:{target[1]}")
        for child in module.child_scopes:
            if targets_of(modules, child, [child.tree]) & found:
                module.fail(None, "a child's source may run a command by name", child.anchor)
        for script, node in sorted(module.scripts.items()):
            module.edges.add(script)
            if runs_commands(modules[script]):
                module.fail(node, f"runs {script}.py, which may run a command by name")
    reached = import_time(modules)
    out = {}
    pins = {}
    for name, module in modules.items():
        every = module.tree is not None and bool(module.top_refs & found)
        dispatchers = [c for c in module.commands if every or (name, c) in found]
        at_import = []
        for node, why in module.fallback:
            statement, head = owner(module, node) if module.tree is not None else (None, True)
            names = [] if statement is None else (
                [statement.name] if isinstance(
                    statement, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))
                else [n.id for n in ast.walk(statement)
                      if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store)])
            if statement is None or head and id(statement) in module.impure_statements \
                    or any((name, n) in reached for n in names):
                at_import.append(why)
        out[name + ".py"] = {
            "edges": sorted(e + ".py" for e in module.edges if e != name),
            "commands": module.commands,
            "dispatchers": dispatchers,
            "fallback": [why for _, why in module.fallback],
            "import_fallback": at_import,
            "global": module.global_,
        }
        if module.tree is None:
            continue
        for node in module.tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                if (name, node.name) in (EXEMPT, REGISTRY):
                    pins[f"{name}.py:{node.name}"] = pin(node)
                elif node.name in dispatchers:
                    pins[f"{name}.py:{node.name}"] = dispatcher_pin(modules, module, node, found)
        if name == MAIN:
            pins[f"{name}.py"] = module_pin(module.tree)
        if name == COMMANDS[0]:
            pins[f"{name}.py:{COMMANDS[1]}"] = registry_pin(module)
    report = json.dumps({"format": 3, "modules": out, "pins": pins}, sort_keys=True)

    # Every module's top level runs in every process: one that raises breaks them all.
    stdout = os.dup(1)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    try:
        import_package(package_dir, siblings)
    except BaseException:  # noqa: BLE001 - any failure to import fails the analysis
        traceback.print_exc()
        sys.stderr.write("the oracle's package doesn't import\n")
        sys.stderr.flush()
        os._exit(1)
    data = report.encode("utf-8")
    while data:
        data = data[os.write(stdout, data):]
    sys.stderr.flush()
    os._exit(0)


main(sys.argv[1])
