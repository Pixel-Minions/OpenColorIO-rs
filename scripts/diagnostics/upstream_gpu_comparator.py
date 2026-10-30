# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
# Diagnostic only. NEVER MERGE. Compile the original upstream comparison functions.
import hashlib,json,pathlib,re,struct,subprocess,sys
root=pathlib.Path.cwd()
out=root/'kept/native-comparator'
out.mkdir(parents=True,exist_ok=True)
source=root/'upstream/OpenColorIO/tests/gpu/GPUUnitTest.cpp'
text=source.read_text()
shader=text[text.index('namespace Shader'):text.index('\nOCIOGPUTest::OCIOGPUTest')]
component=text[text.index('    void DiffComponent('):text.index('    constexpr size_t invalidIndex')]
head='#include <algorithm>\n#include <cmath>\n#include <cstdint>\n#include <cstring>\n#include <fstream>\n#include <iomanip>\n#include <iostream>\n#include <limits>\n#include <stdexcept>\n#include <vector>\n'
main=r'''
std::vector<float> load(const char * path) {
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in) throw std::runtime_error("cannot open captured image");
    const auto bytes = in.tellg();
    if (bytes < 0 || bytes % 4 != 0) throw std::runtime_error("unaligned captured image");
    std::vector<float> values(static_cast<size_t>(bytes) / 4);
    in.seekg(0);
    if (!in.read(reinterpret_cast<char *>(values.data()), bytes)) throw std::runtime_error("short captured image");
    return values;
}
float from_bits(const char * arg) {
    const uint32_t bits = static_cast<uint32_t>(std::stoul(arg));
    float value; std::memcpy(&value, &bits, sizeof(value)); return value;
}
void index_value(size_t value) {
    if (value == std::numeric_limits<size_t>::max()) std::cout << "null";
    else std::cout << value;
}
int main(int argc, char ** argv) {
    try {
        static_assert(sizeof(float) == 4, "captured binary32 required");
        if (argc != 7) throw std::runtime_error("expected CPU GPU count relative min_bits threshold_bits");
        const auto cpu = load(argv[1]), gpu = load(argv[2]);
        const size_t count = std::stoull(argv[3]);
        if (cpu.size() != count || gpu.size() < count || count % 4 != 0) throw std::runtime_error("capture comparison span mismatch");
        const bool relative = std::stoi(argv[4]) != 0;
        const float minimum = from_bits(argv[5]), epsilon = from_bits(argv[6]);
        const size_t invalid = std::numeric_limits<size_t>::max();
        size_t idxDiff=invalid, idxInf=invalid, idxNan=invalid;
        float diff=0.0f;
        // GPUUnitTest.cpp:445-449 @v2.5.2: same pixel/channel traversal and copied DiffComponent.
        for (size_t pixel=0; pixel<count/4; ++pixel)
            for (size_t channel=0; channel<4; ++channel)
                DiffComponent(cpu, gpu, 4*pixel+channel, relative, minimum, diff, idxDiff, idxInf, idxNan);
        uint32_t bits; std::memcpy(&bits, &diff, sizeof(bits));
        std::cout << std::setprecision(std::numeric_limits<float>::max_digits10)
            << "{\"max_diff\":" << diff << ",\"max_diff_bits\":" << bits << ",\"index_diff\":";
        index_value(idxDiff); std::cout << ",\"index_inf\":"; index_value(idxInf);
        std::cout << ",\"index_nan\":"; index_value(idxNan);
        // GPUUnitTest.cpp:458: original failure condition, without its optional diagnostic print flag.
        std::cout << ",\"fails_original_threshold\":"
            << ((diff>epsilon || idxInf!=invalid || idxNan!=invalid) ? "true" : "false") << "}\n";
        return 0;
    } catch(const std::exception & error) { std::cerr << error.what() << '\n'; return 2; }
}
'''
cpp=out/'upstream-comparator.cpp'
cpp.write_text(head+'\n// Verbatim Shader functions: GPUUnitTest.cpp:27-124 @v2.5.2.\n'+shader+'\n// Verbatim DiffComponent: GPUUnitTest.cpp:355-382 @v2.5.2.\n'+component+main)
fixed=(root/'upstream/OpenColorIO/tests/gpu/FixedFunctionOp_test.cpp').read_text()
header=(root/'upstream/OpenColorIO/tests/gpu/GPUUnitTest.h').read_text()
default=re.search(r'm_expectedMinimalValue\{\s*([^}]+)\}',header).group(1).strip()
cases=['style_aces_gamutcomp13_inv','style_aces2_4000nit_p3_rndtrip','style_aces2_nan_bug','style_aces2_rec709_rndtrip','style_aces2_rgb_to_jmh_fwd']
def bits(value): return struct.unpack('<I',struct.pack('<f',float(value)))[0]
prepared=[]
for profile in ('debug','release'):
    folder=pathlib.Path(sys.argv[1])/profile
    for case in cases:
        matches=list(folder.rglob(case+'/observations.json'))
        assert len(matches)==1,(case,profile,matches)
        path=matches[0].parent
        meta=json.loads(matches[0].read_text())
        block=fixed.split(f'OCIO_ADD_GPU_TEST(FixedFunction, {case})',1)[1].split('OCIO_ADD_GPU_TEST(',1)[0]
        minimum=re.search(r'setExpectedMinimalValue\(([^)]+)\)',block)
        minimum=float((minimum.group(1) if minimum else default).removesuffix('f'))
        epsilon=float(re.search(r'setErrorThreshold\(([^)]+)\)',block).group(1).removesuffix('f'))
        assert bits(epsilon)==bits(meta['threshold'])
        assert meta['relative']==('setRelativeComparison(true)' in block)
        cpu=(path/'cpu.bin').read_bytes(); gpu=(path/'wheel.bin').read_bytes()
        assert len(cpu)==meta['entries']*4 and len(gpu)>=len(cpu)
        assert gpu==(path/'port.bin').read_bytes(),'original GPU identity lost'
        old=re.search(r'max_diff: ([^,]+), index_diff: Some\((\d+)\)',meta['comparison'])
        assert old
        prepared.append(dict(Profile=profile,Case=case,CPU=str(path/'cpu.bin'),GPU=str(path/'wheel.bin'),Count=meta['entries'],Relative=meta['relative'],MinimumBits=bits(minimum),ThresholdBits=bits(epsilon),CPU_SHA256=hashlib.sha256(cpu).hexdigest(),GPU_SHA256=hashlib.sha256(gpu).hexdigest(),RustComparisonMaxBits=bits(old.group(1)),RustComparisonIndex=int(old.group(2))))
(out/'input-plan.json').write_text(json.dumps(dict(UpstreamComparatorSHA256=hashlib.sha256(source.read_bytes()).hexdigest(),GeneratedCppSHA256=hashlib.sha256(cpp.read_bytes()).hexdigest(),Inputs=prepared),indent=2)+'\n')
if '--prepare-only' in sys.argv: print('Prepared original C++ comparator and verified all10 capture spans/settings/GPU identities.'); sys.exit(0)
rows=[]
for optimization in ('-O0','-O2'):
    binary=out/('upstream-comparator-'+optimization[1:])
    subprocess.run(['g++','-std=c++17',optimization,str(cpp),'-o',str(binary)],check=True)
    for item in prepared:
        args=[str(item['Count']),str(int(item['Relative'])),str(item['MinimumBits']),str(item['ThresholdBits'])]
        native=json.loads(subprocess.check_output([str(binary),item['CPU'],item['GPU']]+args,text=True))
        control=json.loads(subprocess.check_output([str(binary),item['CPU'],item['CPU']]+args,text=True))
        row=dict(Profile=item['Profile'],Case=item['Case'],Optimization=optimization,Native=native,SelfComparisonControl=control,RustComparisonMatchesNative=(native['max_diff_bits']==item['RustComparisonMaxBits'] and native['index_diff']==item['RustComparisonIndex']))
        rows.append(row)
        (out/'results.json').write_text(json.dumps(dict(CaptureRun=40950573272,NoRustOrOCIOLink=True,OriginalTestsRemainFailing=True,Rows=rows),indent=2)+'\n')
        assert not control['fails_original_threshold'] and control['max_diff_bits']==0
        assert native['fails_original_threshold'],'native reference did not reproduce the alleged failure'
        assert native['index_inf'] is None and native['index_nan'] is None
        assert row['RustComparisonMatchesNative'],'Rust comparison disagrees with upstream C++'
assert len(rows)==20
print(json.dumps(dict(NativeOriginalFailuresReproduced=len(rows),SelfControlsPassed=20,AllRustComparisonsMatchNative=True,OriginalTestsRemainFailing=True),indent=2))
