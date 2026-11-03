# CPU stage values from the official opencolorio==2.5.2 wheel for upstream test
# FixedFunction/style_aces2_nan_bug (tests/gpu/FixedFunctionOp_test.cpp:756-777).
# Every number printed comes from the wheel; this script computes nothing itself.
import struct, sys
import numpy as np
import PyOpenColorIO as O

DISPLAY = "DISPLAY - CIE-XYZ-D65_to_ST2084-P3-D65"
VIEW = "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0"
LIM = [0.68, 0.32, 0.265, 0.69, 0.15, 0.06, 0.32168, 0.33767]
AP0 = [0.7347, 0.2653, 0.0, 1.0, 0.0001, -0.0770, 0.32168, 0.33767]
cfg = O.Config.CreateRaw()

def group(rt=False):
    di = O.BuiltinTransform(DISPLAY); di.setDirection(O.TRANSFORM_DIR_INVERSE)
    vi = O.BuiltinTransform(VIEW); vi.setDirection(O.TRANSFORM_DIR_INVERSE)
    g = O.GroupTransform(); g.appendTransform(di); g.appendTransform(vi)
    return g

def run(t, px, opt=O.OPTIMIZATION_DEFAULT):
    cpu = cfg.getProcessor(t).getOptimizedCPUProcessor(O.BIT_DEPTH_F32, O.BIT_DEPTH_F32, opt)
    a = np.array(px, dtype=np.float32).reshape(-1, 4).copy()
    cpu.applyRGBA(a)
    return a

def ff(style, params, inv=False):
    t = O.FixedFunctionTransform(style, params)
    if inv: t.setDirection(O.TRANSFORM_DIR_INVERSE)
    return t

def h(v): return "%08x" % struct.unpack("<I", struct.pack("<f", float(v)))[0]
def show(name, a):
    print("%-28s" % name, " ".join("%14.8g(%s)" % (x, h(x)) for x in a[:3]))

def stages(x):
    px = [x, x, x, 1.0]
    full = run(group(), px)[0]
    opt = cfg.getProcessor(group()).getOptimizedProcessor(O.OPTIMIZATION_DEFAULT).createGroupTransform()
    ts = list(opt)
    pre = O.GroupTransform()
    for t in ts[:3]: pre.appendTransform(t)
    rgb = run(pre, px, O.OPTIMIZATION_NONE)[0]
    cjmh = run(ff(O.FIXED_FUNCTION_ACES_RGB_TO_JMH_20, LIM), rgb, O.OPTIMIZATION_NONE)[0]
    tjmh = run(ff(O.FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20, [4000] + LIM, True), cjmh, O.OPTIMIZATION_NONE)[0]
    jmh = run(ff(O.FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20, [4000], True), tjmh, O.OPTIMIZATION_NONE)[0]
    ap0 = run(ff(O.FIXED_FUNCTION_ACES_RGB_TO_JMH_20, AP0, True), jmh, O.OPTIMIZATION_NONE)[0]
    post = O.GroupTransform()
    for t in ts[4:]: post.appendTransform(t)
    out = run(post, ap0, O.OPTIMIZATION_NONE)[0]
    ot = O.GroupTransform()
    for t in ts[:4]: ot.appendTransform(t)
    ot_out = run(ot, px, O.OPTIMIZATION_NONE)[0]
    return dict(full=full, rgb=rgb, cjmh=cjmh, tjmh=tjmh, jmh=jmh, ap0=ap0, out=out, ot=ot_out)

if __name__ == "__main__":
    x = np.float32(float(sys.argv[1]) if len(sys.argv) > 1 else 0.89942779)
    s = stages(x)
    print("input", h(x))
    for k in ["rgb", "cjmh", "tjmh", "jmh", "ap0", "ot", "out", "full"]:
        show(k, s[k])
