# Conditioning of the official wheel's CPU result around the nan_bug input:
# run the upstream test's group on f32 inputs k ULPs away from 0x3f6640e6.
import struct, sys
import numpy as np
sys.path.insert(0, __import__("os").path.dirname(__file__))
from cpu_stages import cfg, group, h, O

def f(bits): return struct.unpack("<f", struct.pack("<I", bits))[0]
cpu = cfg.getProcessor(group()).getDefaultCPUProcessor()
base = 0x3f6640e6
ks = list(range(-8, 9)) + [-1000, -300, -100, -30, 30, 100, 300, 1000]
ks = sorted(set(ks))
img = np.array([[f(base + k)] * 3 + [1.0] for k in ks], dtype=np.float32)
cpu.applyRGBA(img)
for k, row in zip(ks, img):
    print("%+5d %.8f  %s" % (k, f(base + k), "  ".join("%12.4f" % v for v in row[:3])))
# Upstream's FIXME neighbour.
a = np.array([[0.89944305, 0.89944305, 0.89944305, 1.0]], dtype=np.float32); cpu.applyRGBA(a)
print("0.89944305 ->", a[0][:3], "bits", h(np.float32(0.89944305)))
