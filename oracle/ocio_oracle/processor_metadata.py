# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Oracle command for what a processor records about what it read (card p1-processor).

- processor_metadata: a ProcessorMetadata filled with addFile and addLook, read back; and the
  processors of groups that carry format metadata, with the metadata the processor and its
  createGroupTransform() hold.

Like every oracle command, it reports what the library does and never computes expected
values.
"""

import PyOpenColorIO as OCIO

from .commands import command

# A group's keys: those the request must give, and those it may.
GROUP_KEYS = {"direction", "name", "id", "attributes", "children"}


def _strings(what, value):
    if not isinstance(value, list) or not all(isinstance(s, str) for s in value):
        raise ValueError(f"{what} must be a list of strings")
    return value


def _group(what, spec):
    """A GroupTransform from {"direction": TRANSFORM_DIR_* name, "name": str or null, "id": str
    or null, "attributes": [[name, value]], "children": [group]}. The metadata calls run in
    this order: setName, setID, then node[name] = value for each attribute; the children are
    appended after."""
    if not isinstance(spec, dict) or set(spec) != GROUP_KEYS:
        raise ValueError(f"{what} must have exactly the keys {sorted(GROUP_KEYS)}")
    direction = spec["direction"]
    if direction not in ("TRANSFORM_DIR_FORWARD", "TRANSFORM_DIR_INVERSE"):
        raise ValueError(f"{what}.direction must name a direction")
    group = OCIO.GroupTransform()
    group.setDirection(getattr(OCIO, direction))
    metadata = group.getFormatMetadata()
    for key, setter in (("name", metadata.setName), ("id", metadata.setID)):
        if spec[key] is not None:
            if not isinstance(spec[key], str):
                raise ValueError(f"{what}.{key} must be a string or null")
            setter(spec[key])
    if not isinstance(spec["attributes"], list):
        raise ValueError(f"{what}.attributes must be a list")
    for a, pair in enumerate(spec["attributes"]):
        name, value = _strings(f"{what}.attributes[{a}]", pair)
        metadata[name] = value
    if not isinstance(spec["children"], list):
        raise ValueError(f"{what}.children must be a list")
    for c, child in enumerate(spec["children"]):
        group.appendTransform(_group(f"{what}.children[{c}]", child))
    return group


def _tree(node):
    """A FormatMetadata node: element name and value, attributes, children."""
    return {"element_name": node.getElementName(), "element_value": node.getElementValue(),
            "attributes": [list(pair) for pair in node.getAttributes()],
            "children": [_tree(child) for child in node.getChildElements()]}


@command
def processor_metadata(args, blobs):
    """A ProcessorMetadata, and the format metadata of groups' processors.

    args:
      files, looks  strings: ProcessorMetadata() then addFile for each file and addLook for
                    each look, in order (the binding passes the bytes before a first NUL)
      groups        [group]: each built as described in _group, then
                    Config.CreateRaw().getProcessor(group)
    result:
      metadata  {"files": list(getFiles()), "looks": list(getLooks())}
      groups    per group {"cache_id": getCacheID(),
                "metadata": the processor's getFormatMetadata(), "group_metadata":
                createGroupTransform().getFormatMetadata(), "group_size": its number of
                transforms}, each metadata as {"element_name", "element_value",
                "attributes": [[name, value]], "children": [node]}

    An unknown key, or a value of the wrong type, makes the command raise.
    """
    if not isinstance(args, dict) or set(args) != {"files", "looks", "groups"}:
        raise ValueError("processor_metadata takes exactly files, looks and groups")
    meta = OCIO.ProcessorMetadata()
    for f in _strings("files", args["files"]):
        meta.addFile(f)
    for look in _strings("looks", args["looks"]):
        meta.addLook(look)
    result = {"metadata": {"files": list(meta.getFiles()), "looks": list(meta.getLooks())},
              "groups": []}
    if not isinstance(args["groups"], list):
        raise ValueError("groups must be a list")
    config = OCIO.Config.CreateRaw()
    for g, spec in enumerate(args["groups"]):
        proc = config.getProcessor(_group(f"groups[{g}]", spec))
        group = proc.createGroupTransform()
        result["groups"].append({
            "cache_id": proc.getCacheID(),
            "metadata": _tree(proc.getFormatMetadata()),
            "group_metadata": _tree(group.getFormatMetadata()),
            "group_size": len(group),
        })
    return result, []
