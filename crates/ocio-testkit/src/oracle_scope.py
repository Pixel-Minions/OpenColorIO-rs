# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""What each oracle command's answer can depend on, for the oracle's response cache.

Run by ``crates/ocio-testkit/src/oracle_scope.rs`` as ``python -I -X utf8 - <package dir>``,
with this source on stdin, it reads every top-level module of the oracle's package
(``ocio_oracle``) with ``ast``, imports none of them, and prints one JSON object:

    {"format": 1, "modules": {"<name>.py": {
        "edges": [the modules this one reaches statically, as "<name>.py"],
        "commands": [the @command functions it defines],
        "dispatchers": [those of its commands that may run another command by name],
        "fallback": [why the module can't be resolved statically],
        "global": [why its top level may act on every command's process]}}}

It reports facts; ``oracle_scope.rs`` decides the keys. Whatever it can't resolve is a
"fallback" reason (a command that can reach the module gets the key of the whole oracle),
and whatever a module's top level does beyond defining names is a "global" reason (every key
then covers the module: ``commands.py`` imports every module of the package in every
process).

Edges:
- ``from . import x``, ``from .x import y``, ``import ocio_oracle.x``, ``from ocio_oracle
  import x`` and ``from ocio_oracle.x import y``, at any depth of the module;
- a string naming a sibling's file, ``"x.py"`` or ``".../x.py"`` (a script the module starts),
  or a module, ``"ocio_oracle.x"`` or ``"ocio_oracle/x"`` (``"ocio_oracle"`` alone names
  ``__init__.py``);
- the imports of Python code in a string (a child process's ``-c`` source): of
  ``ocio_oracle`` modules, and of any bare module name that is also a sibling's.

Fallback reasons:
- an import the package can't resolve (``from ..``, ``*``, a subpackage, a missing sibling);
- the import machinery or a way around the static imports: ``importlib``, ``pkgutil``,
  ``runpy``, ``builtins``, ``gc`` and ``inspect`` (any use of what they bind),
  ``__import__``, ``exec``, ``eval``, ``compile``, ``__builtins__``, ``sys.modules``,
  ``sys.path`` (outside a child's source), ``__subclasses__``, frames (``f_globals``, ...),
  the name ``ocio_oracle`` itself (whose attributes are every module, once ``commands.py``
  imported them), ``getattr`` of a sibling module with a computed name, and a string such as
  ``"__import__"`` used as an argument or an index;
- ``__file__`` other than as the package's directory joined with a sibling's ``"x.py"``,
  or as the oracle's directory;
- a string naming ``ocio_oracle.<something>`` that isn't a sibling, or an f-string naming a
  module of the package by a computed name;
- the ``command`` decorator used other than as the innermost decorator of a top-level
  function (the registry keys commands by the decorated function's ``__name__``);
- running the package as a program (``"-m ocio_oracle"``, or the string ``"ocio_oracle"``
  alone, an argument), whose ``__main__.py`` runs whatever command its request names; running
  a sibling as a script, or a child's source, that may run a command by name (below).

Dispatchers: a top-level definition may run a command by name when it uses ``COMMANDS``
(``commands.py``'s ``command``, which registers into it, aside), or uses a definition that
may, through its own module's names, the names it imports, or ``<module>.<name>`` of a
module it imports; to a fixpoint across modules. A module-level assignment counts as a
definition of the names it binds. A command that may is a dispatcher; every command of a
module whose top level may is one.

``commands.py``'s ``_import_package_modules``, which imports every module of the package, is
the one function exempt: the "global" reasons model what it does.
"""

import ast
import json
import os
import re
import sys

PACKAGE = "ocio_oracle"

DYNAMIC_NAMES = {
    "__import__", "exec", "eval", "compile", "__builtins__", "breakpoint", "__package__",
    "__loader__", "__spec__", "__path__",
}
DYNAMIC_ATTRIBUTES = {
    "__builtins__", "__loader__", "__spec__", "__path__", "__subclasses__", "_getframe",
    "f_globals", "f_locals", "f_back", "tb_frame", "gi_frame", "cr_frame", "ag_frame",
    "modules", "meta_path", "path_hooks", "path_importer_cache",
}
DYNAMIC_STRINGS = {"__import__", "__builtins__", "import_module", "__subclasses__"}
DYNAMIC_MODULES = {"importlib", "pkgutil", "runpy", "imp", "zipimport", "builtins", "gc",
                   "inspect"}

# The calls a module's top level may make and still only define names.
PURE_CALLS = {
    "frozenset", "set", "tuple", "list", "dict", "type", "len", "sorted", "range", "str",
    "int", "float", "bytes", "bytearray", "object", "enumerate", "zip", "isinstance",
    "issubclass", "hasattr", "callable", "min", "max", "sum", "reversed", "getattr", "vars",
    "repr", "abs", "chr", "ord",
    "re.compile", "itertools.count", "np.dtype", "os.path.join", "os.path.dirname",
    "os.path.abspath", "os.path.basename", "struct.Struct",
}
PURE_METHODS = {
    "items", "keys", "values", "union", "intersection", "difference", "format", "join",
    "encode", "upper", "lower", "split", "strip", "replace", "startswith", "endswith",
}
PURE_DECORATORS = {
    "contextlib.contextmanager", "functools.cache", "functools.lru_cache", "staticmethod",
    "classmethod", "property",
}

RUNS_PACKAGE = re.compile(r"-m\s*" + PACKAGE + r"(?![A-Za-z0-9_./\\])")
MENTION = re.compile(r"(?<![A-Za-z0-9_])" + PACKAGE + r"(?![A-Za-z0-9_])([./\\]([A-Za-z_]\w*)?)?")


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


def docstrings(tree):
    """The ids of the docstring nodes of the module, its classes and its functions."""
    out = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
            if node.body and is_docstring(node.body[0]):
                out.add(id(node.body[0].value))
    return out


def is_docstring(node):
    return isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) \
        and isinstance(node.value.value, str)


class Module:
    def __init__(self, name, siblings):
        self.name = name
        self.siblings = siblings
        self.edges = set()
        self.commands = []
        self.fallback = []
        self.global_ = []
        # Local name -> ("module", sibling), ("name", sibling, name) or ("dynamic", module).
        self.bindings = {}
        # Top-level name -> its references (refs_of); and those of the top level.
        self.refs = {}
        self.top_refs = set()
        # The references of children's sources, and the siblings run as scripts.
        self.child_refs = set()
        self.scripts = set()

    def fail(self, node, why):
        self.fallback.append(f"{self.name}.py:{getattr(node, 'lineno', 0)}: {why}")

    def impure(self, node, why):
        self.global_.append(f"{self.name}.py:{getattr(node, 'lineno', 0)}: {why}")

    def sibling(self, name, node):
        """An edge to sibling `name`, or a fallback reason when there is none."""
        if name in self.siblings:
            self.edges.add(name)
            return True
        self.fail(node, f"imports {name!r}, which isn't a module of the package")
        return False

    def bind_from(self, node, module):
        """`from .<module> import ...`, or `from . import ...` for module None."""
        for alias in node.names:
            local = alias.asname or alias.name
            if alias.name == "*":
                self.fail(node, "imports *")
            elif module is None:
                if alias.name in self.siblings:
                    self.edges.add(alias.name)
                    self.bindings[local] = ("module", alias.name)
                else:
                    self.edges.add("__init__")
                    self.bindings[local] = ("name", "__init__", alias.name)
            elif self.sibling(module, node):
                self.bindings[local] = ("name", module, alias.name)

    def dynamic(self, node, name, names, child):
        """An import of the import machinery: in a child's source, a fallback reason; in the
        module, bindings whose every use (outside the exempt function) is one."""
        if child:
            self.fail(node, f"a child's source imports {name}")
        for local in names:
            self.bindings[local] = ("dynamic", name)

    def imports(self, node, child=False):
        """The edges and bindings of an import statement; `child` for a child's source, whose
        bare imports of a sibling's name count as edges too."""
        if isinstance(node, ast.ImportFrom):
            module = node.module
            if node.level > 1:
                self.fail(node, "imports from above the package")
            elif node.level == 1:
                if module is not None and "." in module:
                    self.fail(node, f"imports from a subpackage {module!r}")
                else:
                    self.bind_from(node, module)
            elif module == PACKAGE:
                self.bind_from(node, None)
            elif module and module.startswith(PACKAGE + "."):
                rest = module[len(PACKAGE) + 1:]
                if "." in rest:
                    self.fail(node, f"imports from a subpackage {module!r}")
                else:
                    self.bind_from(node, rest)
            elif module and module.split(".")[0] in DYNAMIC_MODULES:
                self.dynamic(node, module, [a.asname or a.name for a in node.names], child)
            elif child and module in self.siblings:
                self.edges.add(module)
            return
        for alias in node.names:
            top = alias.name.split(".")[0]
            if top == PACKAGE:
                rest = alias.name[len(PACKAGE) + 1:]
                if not rest:
                    self.edges.add("__init__")
                elif "." in rest:
                    self.fail(node, f"imports a subpackage {alias.name!r}")
                elif self.sibling(rest, node) and alias.asname:
                    self.bindings[alias.asname] = ("module", rest)
            elif top in DYNAMIC_MODULES:
                self.dynamic(node, alias.name, [alias.asname or top], child)
            elif child and top in self.siblings:
                self.edges.add(top)


def file_shape(node):
    """"package" for `os.path.dirname(os.path.abspath(__file__))`, "oracle" for the
    `os.path.dirname` of that, else None."""
    def argument_of(node, name):
        if isinstance(node, ast.Call) and dotted(node.func) == name and len(node.args) == 1 \
                and not node.keywords:
            return node.args[0]
        return None

    inner = argument_of(node, "os.path.dirname")
    if inner is None:
        return None
    absolute = argument_of(inner, "os.path.abspath")
    if absolute is not None:
        return "package" if isinstance(absolute, ast.Name) and absolute.id == "__file__" else None
    return "oracle" if file_shape(inner) == "package" else None


def accepted_file_uses(module, tree, parents):
    """The nodes of the accepted uses of `__file__`: the oracle's directory, or the package's
    joined with a sibling's file name."""
    accepted = set()
    scripts = {s + ".py" for s in module.siblings}
    for node in ast.walk(tree):
        shape = file_shape(node)
        parent = parents.get(id(node))
        if shape == "oracle" or (
                shape == "package" and isinstance(parent, ast.Call)
                and dotted(parent.func) == "os.path.join" and not parent.keywords
                and len(parent.args) == 2 and parent.args[0] is node
                and isinstance(parent.args[1], ast.Constant) and parent.args[1].value in scripts):
            accepted.update(id(n) for n in ast.walk(node))
    return accepted


def analyze_code(module, tree, child, exempt=()):
    """The dynamic constructs, `__file__` uses, strings and (in a child's source) imports of
    `tree`. The top-level functions named in `exempt` are skipped."""
    parents = {}
    skipped = set()
    for node in ast.walk(tree):
        for sub in ast.iter_child_nodes(node):
            parents[id(sub)] = node
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name in exempt:
            skipped.update(id(n) for n in ast.walk(node))
    docs = docstrings(tree)
    accepted_file = accepted_file_uses(module, tree, parents)
    for node in ast.walk(tree):
        if id(node) in skipped:
            continue
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            if child:
                module.imports(node, child=True)
        elif isinstance(node, ast.Name):
            binding = module.bindings.get(node.id, ("",))
            if binding[0] == "dynamic":
                module.fail(node, f"uses {binding[1]}")
            elif node.id in DYNAMIC_NAMES:
                module.fail(node, f"uses {node.id}")
            elif node.id == "__file__" and id(node) not in accepted_file:
                module.fail(node, "uses __file__ other than as the package's directory joined "
                                  "with a sibling's file name, or as the oracle's directory")
            elif node.id == PACKAGE:
                module.fail(node, f"uses the package {PACKAGE} other than in an import")
        elif isinstance(node, ast.Attribute):
            if node.attr in DYNAMIC_ATTRIBUTES:
                module.fail(node, f"uses .{node.attr}")
            elif not child and node.attr == "path" and dotted(node.value) == "sys":
                module.fail(node, "uses sys.path")
        elif isinstance(node, ast.Call) and isinstance(node.func, ast.Name) \
                and node.func.id in ("getattr", "setattr", "delattr", "hasattr") and node.args:
            target = node.args[0]
            name = node.args[1] if len(node.args) > 1 else None
            if isinstance(target, ast.Name) \
                    and module.bindings.get(target.id, ("",))[0] in ("module", "dynamic") \
                    and not (isinstance(name, ast.Constant) and isinstance(name.value, str)):
                module.fail(node, f"{node.func.id} of the module {target.id} with a computed "
                                  "name")
        elif isinstance(node, ast.Constant) and id(node) not in docs:
            string(module, node, parents.get(id(node)))
        elif isinstance(node, ast.JoinedStr):
            parts = node.values
            for i, part in enumerate(parts):
                if not (isinstance(part, ast.Constant) and isinstance(part.value, str)):
                    continue
                for match in MENTION.finditer(part.value):
                    computed = match.end() == len(part.value) and i + 1 < len(parts)
                    if match.group(1) is not None and match.group(2) is None or computed:
                        module.fail(node, f"an f-string names a module of {PACKAGE} by a "
                                          "computed name")


def string(module, node, parent):
    """The edges of a string constant: a sibling's file or module name, or the imports of
    code; and the fallback reasons of its mentions of the package."""
    value = node.value
    if isinstance(value, bytes):
        if PACKAGE.encode() in value or any(s.encode() in value for s in DYNAMIC_STRINGS):
            module.fail(node, f"a bytes constant names {PACKAGE} or the import machinery")
        return
    if not isinstance(value, str):
        return
    if value in DYNAMIC_STRINGS and isinstance(parent, (ast.Call, ast.Subscript, ast.keyword)):
        module.fail(node, f"the string {value!r} as an argument or an index")
    if value.strip() == PACKAGE or RUNS_PACKAGE.search(value):
        module.fail(node, f"runs the package {PACKAGE} as a program")
    base = value.replace("\\", "/").rsplit("/", 1)[-1]
    if base.endswith(".py") and base[:-3] in module.siblings:
        module.edges.add(base[:-3])
        module.scripts.add(base[:-3])
    for match in MENTION.finditer(value):
        separator, name = match.group(1), match.group(2)
        if separator is None:
            module.edges.add("__init__")
        elif name in module.siblings:
            module.edges.add(name)
            module.scripts.add(name)
        else:
            module.fail(node, f"the string names {match.group(0)!r}, which isn't a module of "
                              "the package")
    try:
        code = ast.parse(value)
    except (SyntaxError, ValueError):
        return
    if any(isinstance(n, (ast.Import, ast.ImportFrom)) for n in ast.walk(code)):
        analyze_code(module, code, child=True)
        module.child_refs |= refs_of([code], set())


def is_command(module, node):
    """Whether decorator `node` is the oracle's `command`."""
    if isinstance(node, ast.Name):
        if module.name == "commands" and node.id == "command":
            return True
        return module.bindings.get(node.id) == ("name", "commands", "command")
    if isinstance(node, ast.Attribute) and node.attr == "command" \
            and isinstance(node.value, ast.Name):
        return module.bindings.get(node.value.id) == ("module", "commands")
    return False


def refs_of(nodes, skip):
    """What `nodes` refer to, except in `skip`: ("name", n) for a name loaded or deleted,
    ("attr", x, n) for `x.n`, and ("attr", None, n) for an attribute of anything else."""
    out = set()
    for top in nodes:
        for node in ast.walk(top):
            if id(node) in skip:
                continue
            if isinstance(node, ast.Name) and not isinstance(node.ctx, ast.Store):
                out.add(("name", node.id))
            elif isinstance(node, ast.Attribute):
                base = node.value.id if isinstance(node.value, ast.Name) else None
                out.add(("attr", base, node.attr))
    return out


def pure_expression(node):
    """Whether evaluating `node` at import time only computes a value. A lambda's body isn't
    evaluated then; its defaults are."""
    if node is None:
        return True
    lambda_bodies = set()
    for sub in ast.walk(node):
        if isinstance(sub, ast.Lambda):
            lambda_bodies.update(id(n) for n in ast.walk(sub.body))
    for sub in ast.walk(node):
        if id(sub) in lambda_bodies:
            continue
        if isinstance(sub, (ast.NamedExpr, ast.Await, ast.Yield, ast.YieldFrom)):
            return False
        if isinstance(sub, ast.Call):
            if dotted(sub.func) in PURE_CALLS:
                continue
            if isinstance(sub.func, ast.Attribute) and sub.func.attr in PURE_METHODS:
                continue
            return False
    return True


def pure_decorator(module, node):
    """Whether decorator `node` only defines: `command` registers the function, wherever it
    is (out of place, it is a fallback reason instead)."""
    if is_command(module, node):
        return True
    target = node.func if isinstance(node, ast.Call) else node
    if dotted(target) not in PURE_DECORATORS:
        return False
    return all(pure_expression(a) for a in getattr(node, "args", [])) and \
        all(pure_expression(k.value) for k in getattr(node, "keywords", []))


def pure_function(module, node):
    if not all(pure_decorator(module, d) for d in node.decorator_list):
        return False
    args = node.args
    values = list(args.defaults) + [d for d in args.kw_defaults if d is not None]
    values += [a.annotation for a in args.posonlyargs + args.args + args.kwonlyargs]
    values += [args.vararg and args.vararg.annotation, args.kwarg and args.kwarg.annotation,
               node.returns]
    return all(pure_expression(v) for v in values)


def pure_assign(node):
    targets = node.targets if isinstance(node, ast.Assign) else [node.target]
    for target in targets:
        names = target.elts if isinstance(target, (ast.Tuple, ast.List)) else [target]
        if not all(isinstance(n, ast.Name) for n in names):
            return False
    return pure_expression(node.value) and pure_expression(getattr(node, "annotation", None))


def is_main_guard(node):
    """`if __name__ == "__main__":`, with no else."""
    if not isinstance(node, ast.If) or node.orelse or not isinstance(node.test, ast.Compare):
        return False
    test = node.test
    return isinstance(test.left, ast.Name) and test.left.id == "__name__" \
        and len(test.ops) == 1 and isinstance(test.ops[0], ast.Eq) \
        and isinstance(test.comparators[0], ast.Constant) \
        and test.comparators[0].value == "__main__"


def check_top_level(module, tree):
    """The "global" reasons: top-level statements that do more than define names."""
    for node in tree.body:
        if is_docstring(node) or isinstance(node, (ast.Import, ast.ImportFrom, ast.Pass)):
            continue
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            if not pure_function(module, node):
                module.impure(node, f"the definition of {node.name} runs code")
        elif isinstance(node, ast.ClassDef):
            pure = all(pure_decorator(module, d) for d in node.decorator_list) and \
                all(pure_expression(b) for b in node.bases) and \
                all(pure_expression(k.value) for k in node.keywords)
            for item in node.body:
                if is_docstring(item) or isinstance(item, ast.Pass):
                    continue
                if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    pure = pure and pure_function(module, item)
                elif isinstance(item, (ast.Assign, ast.AnnAssign)):
                    pure = pure and pure_assign(item)
                else:
                    pure = False
            if not pure:
                module.impure(node, f"the class {node.name} runs code")
        elif isinstance(node, (ast.Assign, ast.AnnAssign)):
            if not pure_assign(node):
                module.impure(node, "a top-level assignment runs code")
        elif not is_main_guard(node):
            module.impure(node, f"a top-level {type(node).__name__} statement")


def analyze(package_dir, name, siblings):
    module = Module(name, siblings)
    path = os.path.join(package_dir, name + ".py")
    try:
        with open(path, "rb") as f:
            tree = ast.parse(f.read(), filename=path)
    except (SyntaxError, ValueError, UnicodeDecodeError) as exc:
        module.fail(None, f"doesn't parse: {exc}")
        module.impure(None, "doesn't parse")
        return module
    # Bindings first: the checks below look the imported names up.
    for node in ast.walk(tree):
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            module.imports(node)
    exempt = ("_import_package_modules",) if name == "commands" else ()
    analyze_code(module, tree, child=False, exempt=exempt)
    check_top_level(module, tree)

    accepted = set()
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.decorator_list \
                and is_command(module, node.decorator_list[-1]):
            module.commands.append(node.name)
            accepted.update(id(n) for n in ast.walk(node.decorator_list[-1]))
    for node in ast.walk(tree):
        if id(node) not in accepted and isinstance(node, (ast.Name, ast.Attribute)) \
                and is_command(module, node):
            module.fail(node, "uses the command decorator other than as the innermost "
                              "decorator of a top-level function")

    for node in tree.body:
        if isinstance(node, (ast.Import, ast.ImportFrom)):
            continue
        refs = refs_of([node], accepted)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            module.refs[node.name] = refs
            continue
        module.top_refs |= refs
        # A name a top-level statement binds refers to what the statement does.
        for target in ast.walk(node):
            if isinstance(target, ast.Name) and isinstance(target.ctx, ast.Store):
                module.refs.setdefault(target.id, set()).update(refs)
    return module


def resolve(modules, module, ref):
    """The top-level (module, name) that reference `ref` of `module` names, or None."""
    if ref[0] == "name":
        name = ref[1]
        if name in module.refs:
            return (module.name, name)
        binding = module.bindings.get(name, ("",))
        if binding[0] == "name" and binding[1] in modules:
            return (binding[1], binding[2])
        return None
    _, base, name = ref
    binding = module.bindings.get(base, ("",)) if base is not None else ("",)
    if binding[0] == "module" and binding[1] in modules:
        return (binding[1], name)
    return None


def dispatches(modules, module, refs, found):
    """Whether `refs` of `module` use COMMANDS or a definition in `found`."""
    return any(ref[-1] == "COMMANDS" or resolve(modules, module, ref) in found for ref in refs)


def dispatching(modules):
    """The (module, name) of the top-level definitions that may run a command by name."""
    found = set()
    while True:
        more = {
            (module.name, name)
            for module in modules.values()
            for name, refs in module.refs.items()
            if (module.name, name) not in found
            and not (module.name == "commands" and name == "command")
            and dispatches(modules, module, refs, found)
        }
        if not more:
            return found
        found |= more


def main(package_dir):
    siblings = sorted(
        entry[:-3] for entry in os.listdir(package_dir)
        if entry.endswith(".py") and os.path.isfile(os.path.join(package_dir, entry)))
    modules = {name: analyze(package_dir, name, set(siblings)) for name in siblings}
    found = dispatching(modules)

    def runs_commands(module):
        return any((module.name, n) in found for n in module.refs) \
            or dispatches(modules, module, module.top_refs, found)

    for module in modules.values():
        if dispatches(modules, module, module.child_refs, found):
            module.fail(None, "a child's source may run a command by name")
        for script in sorted(module.scripts):
            if runs_commands(modules[script]):
                module.fail(None, f"runs {script}.py, which may run a command by name")
    out = {}
    for name, module in modules.items():
        every = dispatches(modules, module, module.top_refs, found)
        out[name + ".py"] = {
            "edges": sorted(e + ".py" for e in module.edges if e != name),
            "commands": module.commands,
            "dispatchers": [c for c in module.commands if every or (name, c) in found],
            "fallback": module.fallback,
            "global": module.global_,
        }
    sys.stdout.write(json.dumps({"format": 1, "modules": out}, sort_keys=True))


main(sys.argv[1])
