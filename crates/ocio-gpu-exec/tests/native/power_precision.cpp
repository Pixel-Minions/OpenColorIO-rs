// SPDX-License-Identifier: BSD-3-Clause
// Public-fixture diagnostic only. No OCIO or Rust linked, no parity expectation.
// The scalar expression is the wheel's captured GamutComp13 inverse blue branch.
#define FINITE_REPLAY_LIBRARY_ONLY
#include "finite_replay.cpp"

std::string power_shader(bool substitute) {
    return std::string(R"GLSL(#version 120
uniform sampler2D img;
uniform vec4 probe_control;
void main() {
  vec4 value = texture2D(img, gl_TexCoord[0].st);
  float ach = value.r;
  float dist = (ach - value.b) / abs(ach);
  float nd = (dist - 0.879999995) / 0.146821454;
  float p = )GLSL") + (substitute ? "probe_control.r" : "pow(nd, 1.20000005)") + R"GLSL(;
  float cdist = 0.879999995 + 0.146821454 * pow(-(p / (p - 1.0)), 0.833333313);
  float result = ach - cdist * abs(ach);
  gl_FragColor = vec4(nd, p, cdist, result);
}
)GLSL";
}

int main(int argc,char** argv) {
    try {
        need(argc==3,"usage: power-probe CAPTURE_ROOT LABEL");init();
        glTexImage1D=resolve<glTexImage1D_t>("glTexImage1D");glTexImage3D=resolve<glTexImage3D_t>("glTexImage3D");
        const auto folder=std::filesystem::path(argv[1])/"style_aces_gamutcomp13_inv";
        const auto point=read_binary(folder/"point-input.bin");
        const auto cpu=read_binary(folder/"point-cpu.bin");
        const auto observed=read_binary(folder/"point-port.bin");
        need(point.size()==4&&cpu.size()==4&&observed.size()==4,"public captured point");
        std::vector<F> image(256*256*4);for(size_t i=0;i<image.size();++i)image[i]=point[i%4];
        const auto native=replay(power_shader(false),image,{});
        const F platform_p=std::pow(native[0],1.2f);
        need(std::isfinite(native[0])&&std::isfinite(native[1])&&std::isfinite(platform_p),"finite pow operands/results");
        const auto replaced=replay(power_shader(true),image,{}, {platform_p,0,0,0});
        const auto distance=bits(native[1])>bits(platform_p)?bits(native[1])-bits(platform_p):bits(platform_p)-bits(native[1]);
        std::cout<<std::setprecision(17)<<"{\"point_input_bits\":"<<bit_array(point)<<",\"gpu_nd\":"<<native[0]<<",\"gpu_nd_bits\":"<<js(hex(native[0]))
            <<",\"gpu_pow\":"<<native[1]<<",\"gpu_pow_bits\":"<<js(hex(native[1]))<<",\"platform_pow\":"<<platform_p<<",\"platform_pow_bits\":"<<js(hex(platform_p))
            <<",\"pow_ulp_difference\":"<<distance<<",\"minimal_final\":"<<native[3]<<",\"minimal_final_bits\":"<<js(hex(native[3]))
            <<",\"original_gpu_final\":"<<observed[2]<<",\"original_gpu_final_bits\":"<<js(hex(observed[2]))
            <<",\"minimal_matches_original\":"<<(bits(native[3])==bits(observed[2])?"true":"false")
            <<",\"original_cpu_final\":"<<cpu[2]<<",\"substituted_platform_pow_final\":"<<replaced[3]<<",\"substituted_platform_pow_final_bits\":"<<js(hex(replaced[3]))
            <<",\"gl_error\":0,\"public_fixture_only\":true}\n";
        binary(folder/(std::string("power-stages-")+argv[2]+".bin"),std::vector<F>(native.begin(),native.begin()+4));
        binary(folder/(std::string("power-platform-substitution-")+argv[2]+".bin"),std::vector<F>(replaced.begin(),replaced.begin()+4));
        std::ofstream(folder/"minimal-power.glsl")<<power_shader(false);
        std::ofstream(folder/"minimal-power-substitution.glsl")<<power_shader(true);
        eglMakeCurrent(display,nullptr,nullptr,nullptr);eglDestroyContext(display,context);eglDestroySurface(display,surface);eglTerminate(display);
        return 0;
    }catch(const std::exception& e){std::cerr<<"power probe failed: "<<e.what()<<"\n";return 2;}
}
