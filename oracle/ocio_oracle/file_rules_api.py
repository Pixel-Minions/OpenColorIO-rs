# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for file rules and the regular expressions behind them (card p3-oracle,
chunk O3.4).

- file_rules_match: rules (a glob pattern and an extension, a regular expression, or the
  color-space-name path search) inserted into a new FileRules, each insertion's exception;
  the rules as they read back; and for each path, the color space and the index of the rule
  that matches it (Config.getColorSpaceFromFilepath) and filepathOnlyMatchesDefaultRule.

The rules compile and match with each wheel's std::regex (ECMAScript, MSVC's STL or
libstdc++): FileRules.cpp turns a glob pattern and extension into a regular expression
(BuildRegularExpression), validates every expression when it is set, and matches a path with
regex_match. So this is the black box of the regex engine the port reproduces, per platform:
its errors' what() texts reach the messages, and both libraries recurse on long inputs, which
can crash the process. A case can run in a new Python process of its own ("isolated"), where a
crash is reported instead of ending the oracle.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import json
import os
import subprocess
import sys

import PyOpenColorIO as OCIO

from .checks import check_bool, check_keys
from .commands import captured_log, command
from .config_api import (REPORTED, RequestError, attempt, dump_object, make_config, raised,
                         text)

RULE_KEYS = {
    "glob": {"name", "colorspace", "pattern", "extension"},
    "regex": {"name", "colorspace", "regex"},
    "path_search": {"path_search"},
}


def _check_case(c, case):
    check_keys(f"cases[{c}]", case, {"rules", "default", "paths", "isolated"})
    for key in ("rules", "paths"):
        if not isinstance(case.get(key, []), list):
            raise RequestError(f"cases[{c}].{key} must be a list")
    for r, rule in enumerate(case.get("rules", [])):
        if not isinstance(rule, dict):
            raise RequestError(f"cases[{c}].rules[{r}] must be an object")
        kind = ("path_search" if "path_search" in rule else "regex" if "regex" in rule
                else "glob")
        if set(rule) != RULE_KEYS[kind]:
            raise RequestError(f"cases[{c}].rules[{r}]: a {kind} rule has exactly the keys "
                               f"{sorted(RULE_KEYS[kind])}")
        if kind == "path_search":
            if rule["path_search"] is not True:
                raise RequestError(f"cases[{c}].rules[{r}].path_search must be true")
        else:
            for key in RULE_KEYS[kind]:
                text(f"cases[{c}].rules[{r}].{key}", rule[key])
    if "default" in case:
        text(f"cases[{c}].default", case["default"])
    for p, path in enumerate(case.get("paths", [])):
        text(f"cases[{c}].paths[{p}]", path)
    if "isolated" in case:
        check_bool(f"cases[{c}].isolated", case["isolated"])


def _insert(rules, index, rule):
    if "path_search" in rule:
        rules.insertPathSearchRule(index)
    elif "regex" in rule:
        rules.insertRule(index, text("", rule["name"]), text("", rule["colorspace"]),
                         text("", rule["regex"]))
    else:
        rules.insertRule(index, text("", rule["name"]), text("", rule["colorspace"]),
                         text("", rule["pattern"]), text("", rule["extension"]))


def _match(config, path):
    """The color space and rule index getColorSpaceFromFilepath gives the path, and
    filepathOnlyMatchesDefaultRule's answer."""
    out = {}
    try:
        colorspace, index = config.getColorSpaceFromFilepath(path)
        out["colorspace"], out["rule"] = colorspace, index
    except REPORTED as exc:
        out["exception"] = raised(exc)
    out["only_default"] = attempt(config.filepathOnlyMatchesDefaultRule, path)
    return out


def run_case(source, case):
    """One case, in this process."""
    out = {}
    with captured_log() as log:
        try:
            config = make_config(source)
        except REPORTED as exc:
            return {"config": {"exception": raised(exc)}, "log": list(log)}
        rules = OCIO.FileRules()
        inserted = []
        for rule in case.get("rules", []):
            try:
                _insert(rules, rules.getNumEntries() - 1, rule)
                inserted.append(None)
            except REPORTED as exc:
                inserted.append({"exception": raised(exc)})
        out["inserted"] = inserted
        if "default" in case:
            out["default"] = attempt(rules.setDefaultRuleColorSpace, text("", case["default"]))
        out["file_rules"] = dump_object(rules)
        config.setFileRules(rules)
        out["paths"] = [_match(config, text("", path)) for path in case.get("paths", [])]
    out["log"] = list(log)
    return out


# Runs an isolated case in a new process: reads {"oracle_path", "config", "case"} from stdin,
# writes the case's result to stdout.
_CHILD = r"""
import json, sys
request = json.loads(sys.stdin.read())
sys.path.insert(0, request["oracle_path"])
from ocio_oracle import file_rules_api
sys.stdout.write(json.dumps(file_rules_api.run_case(request["config"], request["case"])))
"""


def _run_isolated(source, case):
    oracle_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    request = {"oracle_path": oracle_path, "config": source, "case": case}
    child = subprocess.run([sys.executable, "-I", "-c", _CHILD],
                           input=json.dumps(request).encode("utf-8"), capture_output=True,
                           timeout=600)
    if child.returncode == 0:
        try:
            return json.loads(child.stdout.decode("utf-8"))
        except ValueError:
            pass
    return {"crashed": {"returncode": child.returncode,
                        "stderr": child.stderr.decode("utf-8", "replace")}}


@command
def file_rules_match(args, blobs):
    """File rules built in a new FileRules, and the paths they match.

    args:
      config    the config the rules are set on (a config_calls source; default "raw"): the
                path-search rule looks for its color space names, and getColorSpaceFromFilepath
                and filepathOnlyMatchesDefaultRule take the rules from it
      cases     [{"rules": [rule, ...], "default": color space, "paths": [path, ...],
                  "isolated": bool}, ...], each on a new config and a new FileRules (which holds
                the default rule only). Each rule is inserted before the default rule, in order:
                  {"name", "colorspace", "pattern", "extension"}
                                     insertRule(i, name, colorspace, pattern, extension)
                  {"name", "colorspace", "regex"}
                                     insertRule(i, name, colorspace, regex)
                  {"path_search": true}
                                     insertPathSearchRule(i)
                then setDefaultRuleColorSpace(default) when "default" is given, then
                config.setFileRules(rules). Strings are text or {"bytes": hex} (see
                config_api). "isolated": true runs the case in a new Python process
                (`python -I`), so that a crash in the regex engine is reported
    result:
      cases     per case: {"inserted": per rule null or {"exception"}, "default": null or
                {"exception"} (when given), "file_rules": the FileRules as config_api's
                dump_object writes it (repr(), and per rule its name, pattern, extension, regex,
                color space and custom keys), "paths": per path {"colorspace", "rule" (its
                index), "only_default"} or {"exception", "only_default"}, "log"}; or {"config":
                {"exception"}, "log"} when making the config raised; or, for an isolated case
                whose process died, {"crashed": {"returncode", "stderr"}}
    blobs: none

    An unknown key, a rule with keys of more than one kind, or a string that isn't text or
    {"bytes": hex}, is refused.
    """
    check_keys("file_rules_match", args, {"config", "cases"})
    source = args.get("config", "raw")
    cases = args.get("cases", [])
    if not isinstance(cases, list):
        raise RequestError(f"cases must be a list, not {cases!r}")
    for c, case in enumerate(cases):
        _check_case(c, case)
    out = []
    for case in cases:
        if case.get("isolated"):
            out.append(_run_isolated(source, {k: v for k, v in case.items() if k != "isolated"}))
        else:
            out.append(run_case(source, case))
    return {"cases": out}, []
