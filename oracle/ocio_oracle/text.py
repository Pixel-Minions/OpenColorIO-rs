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


def _check_token(token):
    if not token or len(token) > 63 or any(c.isspace() for c in token):
        raise ValueError(f"token {token!r} must be 1-63 non-space characters")


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


def _clf_matrix(directory, tokens):
    path = _unique_path(directory, "clf")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write('<?xml version="1.0" encoding="UTF-8"?>\n')
        f.write('<ProcessList compCLFversion="3" id="probe">\n')
        f.write('    <Matrix inBitDepth="32f" outBitDepth="32f">\n')
        f.write('        <Array dim="3 3">\n')
        f.write(" ".join(tokens) + "\n")
        f.write("        </Array>\n    </Matrix>\n</ProcessList>\n")
    try:
        group = _load_file_transform(path)
    except OCIO.Exception as exc:
        return {"exception": exception_result(exc)}
    matrices = [t for t in group if isinstance(t, OCIO.MatrixTransform)]
    if len(matrices) != 1:
        return {"transforms": [type(t).__name__ for t in group]}
    m44 = matrices[0].getMatrix()
    m33 = [m44[i] for i in (0, 1, 2, 4, 5, 6, 8, 9, 10)]
    return {"bits": [struct.unpack("<Q", struct.pack("<d", v))[0] for v in m33]}


@command
def clf_matrix_values(args, blobs):
    """Reads tokens as the 3x3 Array of a CLF Matrix through a FileTransform.

    Each token reaches XMLReaderUtils ParseNumber<double> and NumberUtils::from_chars(double)
    (CTFReaderHelper.cpp:355-380, XMLReaderUtils.h:147-205).

    args: {"tokens": [str], "separately": bool}
      separately=false: one Matrix per 9 tokens (the count must be a multiple of 9);
      separately=true: one Matrix per token, followed by eight "0" entries.
    result: one entry per Matrix: {"bits": [u64 x 9]} or {"exception": {...}}
    """
    tokens = args["tokens"]
    for token in tokens:
        _check_token(token)
    with tempfile.TemporaryDirectory() as directory:
        if args.get("separately"):
            return [_clf_matrix(directory, [t] + ["0"] * 8) for t in tokens], []
        if len(tokens) % 9:
            raise ValueError("clf_matrix_values needs a multiple of 9 tokens")
        return [_clf_matrix(directory, tokens[i:i + 9]) for i in range(0, len(tokens), 9)], []
