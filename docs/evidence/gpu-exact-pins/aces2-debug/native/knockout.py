# Diagnostic only (claude/aces2-gpu-debug). Builtin knock-out on llvmpipe: re-render each
# captured original shader with one class of GLSL builtins replaced by a double-precision
# emulation rounded once to float (the same result a correctly rounded libm gives), and report
# the upstream-formula error vs the captured wheel CPU image. Variants only change #version
# 120 -> 400 (needed for fp64); the "v400" control shows that alone changes nothing.
#   wheel native/knockout.py
import json, os, re, subprocess
import numpy as np

LIB = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "acc_lib.glsl")).read()

CASES = [("style_aces2_nan_bug", 0.01), ("style_aces2_4000nit_p3_rndtrip", 0.018),
         ("style_aces2_rec709_rndtrip", 0.004), ("style_aces2_rgb_to_jmh_fwd", 0.0002)]
VARIANTS = [("original", []), ("v400", []), ("pow", ["pow"]), ("atan", ["atan"]), ("sin/cos", ["sin", "cos"]),
            ("log/log2", ["log", "log2"]), ("all but pow", ["atan", "sin", "cos", "log", "log2"]),
            ("all", ["pow", "atan", "sin", "cos", "log", "log2"])]

def variant(src, names, original):
    if original:
        return src
    body = src.split("\n", 1)[1]
    assert src.startswith("#version 120\n")
    for nme in names:
        body = re.sub(r"(?<![\w.])%s\(" % nme, "acc_%s(" % nme, body)
    return "#version 400 compatibility\n" + (LIB if names else "") + body

for case, lim in CASES:
    d = os.path.join("work", case); o = json.load(open(os.path.join(d, "observations.json")))
    tex = []
    for i, x in enumerate(o["gpu_metadata"]["shader"]["textures"]):
        tex += [x["sampler_name"], "1" if x["dimensions"] == "TEXTURE_1D" else "2", str(x["width"]), str(x["height"]),
                "1" if x["channel"] == "TEXTURE_RED_CHANNEL" else "3", "9728" if x["interpolation"] == "INTERP_NEAREST" else "9729",
                os.path.join(d, "texture-%d.bin" % i)]
    src = open(os.path.join(d, "full.glsl")).read()
    n = o["original_size"] // 4
    c = np.fromfile(os.path.join(d, "cpu.bin"), np.float32).reshape(-1, 4)[:n, :3]
    p = np.fromfile(os.path.join(d, "port.bin"), np.float32).reshape(-1, 4)[:n, :3]
    print("==", case, "limit", lim)
    for label, names in VARIANTS:
        open("/tmp/ko.glsl", "w").write(variant(src, names, label == "original"))
        subprocess.run(["/tmp/b/probe", "/tmp/ko.glsl", os.path.join(d, "input.bin"), "256", "256", "/tmp/ko.bin"] + tex,
                       check=True, stderr=subprocess.DEVNULL)
        g = np.fromfile("/tmp/ko.bin", np.float32).reshape(-1, 4)[:n, :3]
        e = np.abs(c - g)
        if o["relative"]: e = e / np.maximum(np.abs(c), o["minimum"])
        k = np.unravel_index(np.argmax(e), e.shape)
        exact = np.array_equal(g.view(np.uint32), c.view(np.uint32))
        print("  %-12s maxerr %.9g at px %d ch %d %s | image==CI GPU: %s | image==CPU bits: %s" % (
            label, e.max(), k[0], k[1], "PASS" if e.max() <= lim else "FAIL",
            np.array_equal(g.view(np.uint32), p.view(np.uint32)), exact))
