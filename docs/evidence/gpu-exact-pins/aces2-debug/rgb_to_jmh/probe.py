# Diagnostic only (claude/aces2-gpu-debug). Error decomposition for upstream test
# FixedFunction/style_aces2_rgb_to_jmh_fwd on llvmpipe, run inside ocio-rs-rocky9:1:
#   python3 rgb_to_jmh/probe.py
# 1. The CPU replica (native/cpu_rgb_to_jmh --ap0) must reproduce the wheel's captured
#    cpu.bin J, M, h bits for every pixel; only then are its rgb_a / Aab used.
# 2. The captured shader is instrumented to replace, per pixel, rgb_a or Aab with the CPU
#    replica's value (texture injTex, selected by uniform injCtl.x), and to record the GPU's own
#    lms, rgb_a, Aab. The final error vs cpu.bin then shows how much each later stage adds.
import os, struct, subprocess
import numpy as np

cap = "work/style_aces2_rgb_to_jmh_fwd"
n = 27
img = np.fromfile(os.path.join(cap, "input.bin"), np.float32).reshape(-1, 4)[:n].copy()
cpu = np.fromfile(os.path.join(cap, "cpu.bin"), np.float32).reshape(-1, 4)[:n]
gpu = np.fromfile(os.path.join(cap, "port.bin"), np.float32).reshape(-1, 4)[:n]
img.tofile("/tmp/rj_in.bin")
subprocess.run(["/tmp/rj", "--ap0", "--batch", "/tmp/rj_in.bin", "/tmp/rj_out.bin"], check=True)
rep = np.fromfile("/tmp/rj_out.bin", np.float32).reshape(n, 4, 4)
rep_lms, rep_rgba, rep_aab, rep_jmh = rep[:, 0, :3], rep[:, 1, :3], rep[:, 2, :3], rep[:, 3, :3]
same = np.array_equal(rep_jmh.view(np.uint32), cpu[:, :3].view(np.uint32))
print("CPU replica J,M,h == wheel cpu.bin bits for all %d pixels: %s" % (n, same))
assert same

src = open(os.path.join(cap, "full.glsl")).read()
def sub(old, new):
    global src
    assert src.count(old) == 1, old
    src = src.replace(old, new)
sub("// Declaration of the OCIO shader function\n",
    "vec4 D[3];\nuniform vec4 injCtl;\nuniform sampler2D injTex;\n// Declaration of the OCIO shader function\n")
INJ = "texture2D(injTex, vec2(gl_TexCoord[0].s, 0.5)).rgb"
sub("        vec3 rgb_a = (sign(lms) * F_L_v) / ( 27.1299992 + F_L_v);\n",
    "        vec3 rgb_a = (sign(lms) * F_L_v) / ( 27.1299992 + F_L_v);\n"
    "        if (int(injCtl.x) == 1) rgb_a = %s; D[0] = vec4(lms, 0.0); D[1] = vec4(rgb_a, 0.0);\n" % INJ)
sub("        Aab = mat3(20.25881, 15480., 1720., 10.129405, -16887.2734, 1720., 0.506470263, 1407.27271, -3440.) * rgb_a.rgb;\n",
    "        Aab = mat3(20.25881, 15480., 1720., 10.129405, -16887.2734, 1720., 0.506470263, 1407.27271, -3440.) * rgb_a.rgb;\n"
    "        if (int(injCtl.x) == 2) Aab = %s; D[2] = vec4(Aab, 0.0);\n" % INJ)
sub("    gl_FragColor = OCIOMain(col);", "    vec4 o = OCIOMain(col);\n    int row = int(gl_FragCoord.y);\n"
    "    gl_FragColor = (row == 0) ? o : D[row - 1];")
open("/tmp/rj.glsl", "w").write(src)
np.tile(img, (4, 1)).tofile("/tmp/rj_img.bin")

def run(mode, inj):
    inj.astype(np.float32).tofile("/tmp/rj_inj.bin")
    env = dict(os.environ, PROBE_UNIFORMS="injCtl:%08x,0,0,0" % struct.unpack("<I", struct.pack("<f", float(mode)))[0])
    subprocess.run(["/tmp/b/probe", "/tmp/rj.glsl", "/tmp/rj_img.bin", str(n), "4", "/tmp/rj_o.bin",
                    "injTex", "2", str(n), "1", "3", "9728", "/tmp/rj_inj.bin"], check=True, env=env, stderr=subprocess.DEVNULL)
    return np.fromfile("/tmp/rj_o.bin", np.float32).reshape(4, n, 4)

base = run(0, rep_rgba)
print("instrumented output == captured GPU image bits:", np.array_equal(base[0].view(np.uint32), gpu.view(np.uint32)))
def ulps(a, b):
    return (a.view(np.int32).astype(np.int64) - b.view(np.int32).astype(np.int64))
print("GPU lms   - CPU lms   (ULPs) max |.|:", np.abs(ulps(base[1][:, :3], rep_lms)).max(axis=0))
print("GPU rgb_a - CPU rgb_a (ULPs) max |.|:", np.abs(ulps(base[2][:, :3], rep_rgba)).max(axis=0))
print("GPU Aab   - CPU Aab   relative max:", (np.abs(base[3][:, :3] - rep_aab) / np.abs(rep_aab)).max(axis=0))
for label, mode, inj in [("GPU as captured", 0, rep_rgba), ("CPU rgb_a injected", 1, rep_rgba), ("CPU Aab injected", 2, rep_aab)]:
    out = run(mode, inj)[0]
    e = np.abs(out[:, :3] - cpu[:, :3])
    k = np.unravel_index(np.argmax(e), e.shape)
    print("%-20s max abs error J %.6g M %.6g h %.6g | worst pixel %d channel %d (limit 0.0002)" % (
        label, e[:, 0].max(), e[:, 1].max(), e[:, 2].max(), k[0], k[1]))
w = 11
print("pixel 11 input", img[w, :3], "CPU JMh", cpu[w, :3], "GPU JMh", gpu[w, :3])
print("pixel 11 rgb_a ULPs GPU-CPU", ulps(base[2][w, :3], rep_rgba[w]), "Aab CPU", rep_aab[w], "GPU", base[3][w, :3])
