# Diagnostic only. Validates the fp64 emulations in acc_lib.glsl on llvmpipe against the
# platform (glibc double-precision math via Python's math module, rounded once to float), and
# measures llvmpipe's own builtins the same way.
#   wheel native/check_acc.py
import math, os, subprocess
import numpy as np

LIB = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "acc_lib.glsl")).read()
rng = np.random.default_rng(1234)
N = 4096
x = np.exp(rng.uniform(math.log(1e-4), math.log(1e3), N)).astype(np.float32)
ys = np.array([0.42, 2.38095236, 1.13705599, 0.879464149, -0.879464149, 1 / 2.4, 2.4], np.float32)
y = np.where(rng.uniform(size=N) < 0.5, ys[rng.integers(0, len(ys), N)], rng.uniform(-3, 3, N)).astype(np.float32)
ay = rng.normal(size=N).astype(np.float32); ax = rng.normal(size=N).astype(np.float32)
ang = rng.uniform(0, 2 * math.pi, N).astype(np.float32)
img = np.stack([x, y, ay, ax], 1).astype(np.float32)
img2 = np.stack([ang, ang, ang, ang], 1).astype(np.float32)
img.tofile("/tmp/ca_in1.bin"); img2.tofile("/tmp/ca_in2.bin")

def run(expr, inp):
    src = "#version 400 compatibility\n" + LIB + """
uniform sampler2D img;
void main() { vec4 v = texture2D(img, gl_TexCoord[0].st); gl_FragColor = %s; }
""" % expr
    open("/tmp/ca.glsl", "w").write(src)
    subprocess.run(["/tmp/b/probe", "/tmp/ca.glsl", inp, str(N), "1", "/tmp/ca_out.bin"], check=True, stderr=subprocess.DEVNULL)
    return np.fromfile("/tmp/ca_out.bin", np.float32).reshape(N, 4)

def ulp(a, b):
    return np.abs(a.view(np.int32).astype(np.int64) - b.view(np.int32).astype(np.int64))

f32 = np.float32
ref_pow = np.array([f32(math.pow(float(a), float(b))) for a, b in zip(x, y)])
ref_atan = np.array([f32(math.atan2(float(a), float(b))) for a, b in zip(ay, ax)])
ref_log = np.array([f32(math.log(float(a))) for a in x])
ref_sin = np.array([f32(math.sin(float(a))) for a in ang]); ref_cos = np.array([f32(math.cos(float(a))) for a in ang])
o = run("vec4(acc_pow(v.x, v.y), acc_atan(v.z, v.w), acc_log(v.x), pow(v.x, v.y))", "/tmp/ca_in1.bin")
o2 = run("vec4(atan(v.z, v.w), log(v.x), 0.0, 0.0)", "/tmp/ca_in1.bin")
o3 = run("vec4(acc_sin(v.x), acc_cos(v.x), sin(v.x), cos(v.x))", "/tmp/ca_in2.bin")
for name, got, ref in [("acc_pow", o[:, 0], ref_pow), ("llvmpipe pow", o[:, 3], ref_pow),
                       ("acc_atan", o[:, 1], ref_atan), ("llvmpipe atan", o2[:, 0], ref_atan),
                       ("acc_log", o[:, 2], ref_log), ("llvmpipe log", o2[:, 1], ref_log),
                       ("acc_sin", o3[:, 0], ref_sin), ("llvmpipe sin", o3[:, 2], ref_sin),
                       ("acc_cos", o3[:, 1], ref_cos), ("llvmpipe cos", o3[:, 3], ref_cos)]:
    u = ulp(got, ref)
    print("%-14s ULP vs glibc-double-rounded: max %d, mean %.3f, exact %.1f%%" % (name, u.max(), u.mean(), 100.0 * np.mean(u == 0)))
# pow at the exponent the ACES2 post-adaptation compression uses, for values ~ the CAM16 lms range
xs = np.exp(rng.uniform(math.log(0.01), math.log(100.0), N)).astype(np.float32)
img3 = np.stack([xs, np.full(N, 0.42, np.float32), xs, xs], 1).astype(np.float32); img3.tofile("/tmp/ca_in3.bin")
o4 = run("vec4(pow(v.x, v.y), acc_pow(v.x, v.y), 0.0, 0.0)", "/tmp/ca_in3.bin")
ref = np.array([f32(math.pow(float(a), float(f32(0.42)))) for a in xs])
for name, got in [("llvmpipe pow(x,0.42f)", o4[:, 0]), ("acc_pow(x,0.42f)", o4[:, 1])]:
    u = ulp(got, ref)
    print("%-22s x in [0.01,100]: ULP max %d, mean %.3f, exact %.1f%%" % (name, u.max(), u.mean(), 100.0 * np.mean(u == 0)))
