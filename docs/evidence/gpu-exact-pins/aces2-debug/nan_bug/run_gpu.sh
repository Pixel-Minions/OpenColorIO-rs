#!/usr/bin/env bash
# Diagnostic only. Run the instrumented nan_bug shader on llvmpipe for given input bits.
# usage (inside ocio-rs-rocky9:1, cwd /dbg): nan_bug/run_gpu.sh INPUT_HEX [NAME=hex,... injections]
set -euo pipefail
W=work/style_aces2_nan_bug; B=/tmp/b; mkdir -p $B
[ -x $B/probe ] || g++ -O2 -std=c++17 -o $B/probe native/probe.cpp -ldl
in=$1; shift
export PROBE_UNIFORMS="$(python3 nan_bug/instrument.py $W/full.glsl $B/inst.glsl "$@")"
python3 - $in $B/in.bin <<'PY'
import struct,sys
v=struct.unpack('<f',struct.pack('<I',int(sys.argv[1],16)))[0]
open(sys.argv[2],'wb').write(struct.pack('<4f',v,v,v,1.0)*64)
PY
T="ocio_lut1d_0Sampler 2 4096 17 1 9729 $W/texture-0.bin ocio_reach_m_table_1Sampler 1 363 1 1 9728 $W/texture-1.bin ocio_gamut_cusp_table_1Sampler 1 363 1 3 9728 $W/texture-2.bin"
$B/probe $B/inst.glsl $B/in.bin 32 2 $B/out.bin $T 2>/dev/null
python3 - $B/out.bin <<'PY'
import struct,sys
d=open(sys.argv[1],'rb').read(); f=struct.unpack('<%df'%(len(d)//4),d); u=struct.unpack('<%dI'%(len(d)//4),d)
names={29:'F_L_v',0:'lut',1:'rgb_in',2:'lms',3:'rgb_a',4:'Aab',5:'JMh',6:'hrad cos sin reachMaxM',7:'cusp J M gTop thr',8:'tonemappedJMh',9:'JMh_inv',10:'Aab_out',11:'rgb_a_out',30:'lms_out',31:'AP0'}
for c in range(2):
    for k in range(3): names[12+6*c+k]=['c%d Jx focusJ slopeGain JIS'%c,'c%d slope JIC gamutBM gTop'%c,'c%d reachBM remapM remapJ h'%c][k]
    names[15+6*c]='c%d ratio prop thr M'%c; names[16+6*c]='c%d moff goff roff scale'%c; names[17+6*c]='c%d nd r/g nd-1'%c
for i in sorted(names):
    print('%-26s'%names[i],' '.join('%15.9g(%08x)'%(f[4*i+j],u[4*i+j]) for j in range(4)))
print('%-26s'%'FINAL',' '.join('%15.9g(%08x)'%(f[128+j],u[128+j]) for j in range(4)))
PY
