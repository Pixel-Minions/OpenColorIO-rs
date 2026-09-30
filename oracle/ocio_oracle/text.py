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
