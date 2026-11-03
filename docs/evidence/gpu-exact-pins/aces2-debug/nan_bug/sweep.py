# Diagnostic only. Conditioning sweep around the nan_bug input (0x3f6640e6), run inside
# ocio-rs-rocky9:1. CPU values come from the official wheel (Linux); GPU values from the
# captured original shader on llvmpipe (native/probe). Nothing here is an expected value.
#   wheel nan_bug/sweep.py HALF_WIDTH_ULPS
import os, struct, subprocess, sys
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
from cpu_stages import cfg, group, ff, O, LIM

half = int(sys.argv[1]) if len(sys.argv) > 1 else 2048
n = 2 * half
side = int(np.ceil(np.sqrt(n)))
base = 0x3f6640e6
bits = np.arange(base - half, base - half + side * side, dtype=np.uint32)
x = bits.view(np.float32)
img = np.stack([x, x, x, np.ones_like(x)], axis=1).astype(np.float32)
W = "work/style_aces2_nan_bug"
img.tofile("/tmp/sweep_in.bin")
T = ("ocio_lut1d_0Sampler 2 4096 17 1 9729 %s/texture-0.bin ocio_reach_m_table_1Sampler 1 363 1 1 9728 %s/texture-1.bin "
     "ocio_gamut_cusp_table_1Sampler 1 363 1 3 9728 %s/texture-2.bin" % (W, W, W)).split()
subprocess.run(["/tmp/b/probe", W + "/full.glsl", "/tmp/sweep_in.bin", str(side), str(side), "/tmp/sweep_gpu.bin"] + T,
               check=True, stderr=subprocess.DEVNULL)
gpu = np.fromfile("/tmp/sweep_gpu.bin", dtype=np.float32).reshape(-1, 4)
cpu = img.copy()
cfg.getProcessor(group()).getDefaultCPUProcessor().applyRGBA(cpu)

# CPU gamut-compress expansion (wheel components): does the inverse land on the pole side?
opt = list(cfg.getProcessor(group()).getOptimizedProcessor(O.OPTIMIZATION_DEFAULT).createGroupTransform())
pre = O.GroupTransform()
for t in opt[:3]: pre.appendTransform(t)
pre.appendTransform(ff(O.FIXED_FUNCTION_ACES_RGB_TO_JMH_20, LIM))
cj = img.copy(); cfg.getProcessor(pre).getOptimizedCPUProcessor(O.BIT_DEPTH_F32, O.BIT_DEPTH_F32, O.OPTIMIZATION_NONE).applyRGBA(cj)
tj = cj.copy(); cfg.getProcessor(ff(O.FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20, [4000] + LIM, True)).getOptimizedCPUProcessor(
    O.BIT_DEPTH_F32, O.BIT_DEPTH_F32, O.OPTIMIZATION_NONE).applyRGBA(tj)
cpu_pole = tj[:, 1] > 2 * cj[:, 1]

def rel(c, g):
    return np.max(np.abs(c[:, :3] - g[:, :3]) / np.maximum(np.abs(c[:, :3]), 1.0), axis=1)
r = rel(cpu, gpu)
sel = slice(0, n)
print("inputs: %d consecutive f32 values, 0x%08x..0x%08x (%.8f..%.8f)" % (n, bits[0], bits[n - 1], x[0], x[n - 1]))
print("CPU (wheel) inverse lands on pole side (gamut-compress M expansion > 2x): %d / %d" % (cpu_pole[sel].sum(), n))
print("CPU regime flips between adjacent f32 inputs: %d" % np.count_nonzero(np.diff(cpu_pole[sel].astype(int))))
print("GPU vs CPU relative error (upstream formula, min 1.0) <= 0.01: %d / %d" % (np.count_nonzero(r[sel] <= 0.01), n))
print("  ... among CPU pole-side inputs: %d / %d pass" % (np.count_nonzero((r[sel] <= 0.01) & cpu_pole[sel]), cpu_pole[sel].sum()))
print("  ... among CPU regular inputs:   %d / %d pass" % (np.count_nonzero((r[sel] <= 0.01) & ~cpu_pole[sel]), (~cpu_pole[sel]).sum()))
i0 = half
print("actual input 0x%08x: cpu %s gpu %s rel %.8g pole %s" % (bits[i0], cpu[i0, :3], gpu[i0, :3], r[i0], cpu_pole[i0]))
# Show the local pattern of CPU regimes for +-40 ULPs.
print("CPU pole-side map, -40..+40 ULPs ('X' pole, '.' regular, '|' actual input):")
print("".join(("|" if k == 0 else "") + ("X" if cpu_pole[i0 + k] else ".") for k in range(-40, 41)))
print("GPU pass map (rel <= 0.01), same range ('+' pass, '-' fail):")
print("".join(("|" if k == 0 else "") + ("+" if r[i0 + k] <= 0.01 else "-") for k in range(-40, 41)))
fl = np.nonzero(np.diff(cpu_pole[sel].astype(int)))[0]
if len(fl):
    print("CPU flip band: %+d..%+d ULPs from the input (%.8f..%.8f); below it regular=%s, above it pole=%s" % (
        fl[0] + 1 - half, fl[-1] - half, x[fl[0] + 1], x[fl[-1]], not cpu_pole[0], bool(cpu_pole[n - 1])))
    band = slice(fl[0] + 1, fl[-1] + 1)
    print("inside the band: CPU pole-side %d / %d, GPU-vs-CPU pass %d / %d" % (
        cpu_pole[band].sum(), fl[-1] - fl[0], np.count_nonzero(r[band] <= 0.01), fl[-1] - fl[0]))
