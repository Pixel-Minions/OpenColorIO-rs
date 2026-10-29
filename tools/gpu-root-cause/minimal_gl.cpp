// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Independent diagnostic: no OCIO or Rust code is linked, no expected image is supplied.
// EGL/GL ABI declarations match the Khronos APIs; runtime libraries come from the fixed image.
#include <array>
#include <cstdint>
#include <cstring>
#include <dlfcn.h>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

using U = unsigned int;
using I = int;
using F = float;
using P = void*;
#define API(ret, name, args) using name##_t = ret (*) args; name##_t name = nullptr
API(P, eglGetDisplay, (P));
API(U, eglInitialize, (P,I*,I*));
API(U, eglChooseConfig, (P,const I*,P*,I,I*));
API(U, eglBindAPI, (U));
API(P, eglCreatePbufferSurface, (P,P,const I*));
API(P, eglCreateContext, (P,P,P,const I*));
API(U, eglMakeCurrent, (P,P,P,P));
API(U, eglDestroyContext, (P,P));
API(U, eglDestroySurface, (P,P));
API(U, eglTerminate, (P));
API(P, eglGetProcAddress, (const char*));
API(const unsigned char*, glGetString, (U));
API(U, glGetError, ());
API(void, glGenTextures, (I,U*));
API(void, glDeleteTextures, (I,const U*));
API(void, glBindTexture, (U,U));
API(void, glActiveTexture, (U));
API(void, glTexImage2D, (U,I,I,I,I,I,U,U,const P));
API(void, glTexParameteri, (U,U,I));
API(void, glGetTexImage, (U,I,U,U,P));
API(void, glGenFramebuffers, (I,U*));
API(void, glDeleteFramebuffers, (I,const U*));
API(void, glBindFramebuffer, (U,U));
API(void, glFramebufferTexture2D, (U,U,U,U,I));
API(U, glCheckFramebufferStatus, (U));
API(void, glGenRenderbuffers, (I,U*));
API(void, glDeleteRenderbuffers, (I,const U*));
API(void, glBindRenderbuffer, (U,U));
API(void, glRenderbufferStorage, (U,U,I,I));
API(void, glFramebufferRenderbuffer, (U,U,U,U));
API(U, glCreateShader, (U));
API(void, glShaderSource, (U,I,const char* const*,const I*));
API(void, glCompileShader, (U));
API(void, glGetShaderiv, (U,U,I*));
API(void, glGetShaderInfoLog, (U,I,I*,char*));
API(U, glCreateProgram, ());
API(void, glAttachShader, (U,U));
API(void, glLinkProgram, (U));
API(void, glGetProgramiv, (U,U,I*));
API(void, glGetProgramInfoLog, (U,I,I*,char*));
API(void, glUseProgram, (U));
API(void, glDeleteProgram, (U));
API(void, glDeleteShader, (U));
API(I, glGetUniformLocation, (U,const char*));
API(void, glUniform1i, (I,I));
API(void, glUniform1f, (I,F));
API(void, glUniform2f, (I,F,F));
API(void, glUniform4fv, (I,I,const F*));
API(void, glUniform4ui, (I,U,U,U,U));
API(void, glUniformMatrix4fv, (I,I,unsigned char,const F*));
API(void, glViewport, (I,I,I,I));
API(void, glMatrixMode, (U));
API(void, glLoadIdentity, ());
API(void, glOrtho, (double,double,double,double,double,double));
API(void, glClampColor, (U,U));
API(void, glDrawBuffer, (U));
API(void, glReadBuffer, (U));
API(void, glClearColor, (F,F,F,F));
API(void, glClear, (U));
API(void, glColor3f, (F,F,F));
API(void, glEnable, (U));
API(void, glDisable, (U));
API(void, glBegin, (U));
API(void, glEnd, ());
API(void, glTexCoord2f, (F,F));
API(void, glVertex2f, (F,F));
API(void, glFinish, ());
API(void, glReadPixels, (I,I,I,I,U,U,P));

void need(bool value, const std::string& reason) { if (!value) throw std::runtime_error(reason); }
std::string js(const std::string& text) {
    std::ostringstream out; out << '"';
    for (unsigned char c : text) {
        if (c == '"' || c == '\\') out << '\\' << c;
        else if (c == '\n') out << "\\n";
        else if (c == '\r') out << "\\r";
        else if (c == '\t') out << "\\t";
        else if (c < 32) out << "\\u" << std::hex << std::setw(4) << std::setfill('0') << unsigned(c);
        else out << c;
    }
    return out.str() + '"';
}
F value(U raw) { F result; std::memcpy(&result, &raw, 4); return result; }
U bits(F v) { U result; std::memcpy(&result, &v, 4); return result; }
std::string hex(F v) { std::ostringstream s; s << std::hex << std::setw(8) << std::setfill('0') << bits(v); return s.str(); }
std::string bit_array(const std::vector<F>& v, size_t limit = 64) {
    std::string s = "["; for (size_t i=0; i<v.size() && i<limit; ++i) { if(i) s+=','; s+=js(hex(v[i])); } return s+"]";
}
void binary(const std::filesystem::path& p, const std::vector<F>& v) {
    std::ofstream f(p, std::ios::binary); f.write(reinterpret_cast<const char*>(v.data()), v.size()*4); need(bool(f), "write: "+p.string());
}
std::vector<F> read_binary(const std::filesystem::path& p) {
    std::ifstream f(p,std::ios::binary|std::ios::ate); need(bool(f),"read: "+p.string()); auto length=f.tellg();
    need(length>0 && length%4==0,"float file size"); std::vector<F> v(static_cast<size_t>(length)/4); f.seekg(0); f.read(reinterpret_cast<char*>(v.data()),length); need(bool(f),"float file read"); return v;
}
std::vector<F> default_image(bool nan=true, bool infinity=true, bool wide=true) {
    // Diagnostic input construction copied from GPUUnitTest.cpp:212-286 @ v2.5.2.
    // Default wide interval is [-1,2]; special values precede the finite ramp.
    const F minimum=wide?-1.0f:0.0f;
    std::vector<F> image(256*256*4,minimum);
    size_t start=0;
    for(U raw:{0x7fc00000u,0x7f800000u,0xff800000u}) {
        if(raw==0x7fc00000u ? !nan : !infinity) { continue; }
        for(size_t channel=0;channel<4;++channel)for(size_t i=0;i<4;++i)image[start+channel*4+i]=i==channel?value(raw):0;
        start+=16;
    }
    const size_t entries=image.size()-start;
    const F step=(wide?3.0f:1.0f)/F(entries);
    for(size_t i=0;i<entries;++i)image[start+i]=minimum+step*F(i);
    return image;
}
void error(const char* where) { U e=glGetError(); need(e==0,std::string(where)+" GL error "+std::to_string(e)); }
P egl_library=nullptr, gl_library=nullptr, display=nullptr, surface=nullptr, context=nullptr;
template<class T> T resolve(const char* name, bool egl=false) {
    P p=dlsym(egl?egl_library:gl_library,name); if(!p && !egl) p=eglGetProcAddress(name); need(p!=nullptr,std::string("missing ")+name); return reinterpret_cast<T>(p);
}
void init() {
    egl_library=dlopen("libEGL.so.1",RTLD_NOW|RTLD_LOCAL); gl_library=dlopen("libGL.so.1",RTLD_NOW|RTLD_LOCAL); need(egl_library&&gl_library,"runtime GL/EGL libraries");
#define E(name) name=resolve<name##_t>(#name,true)
    E(eglGetDisplay); E(eglInitialize); E(eglChooseConfig); E(eglBindAPI); E(eglCreatePbufferSurface); E(eglCreateContext); E(eglMakeCurrent); E(eglDestroyContext); E(eglDestroySurface); E(eglTerminate); E(eglGetProcAddress);
#define G(name) name=resolve<name##_t>(#name)
    display=eglGetDisplay(nullptr); I major=0,minor=0,count=0; need(display&&eglInitialize(display,&major,&minor),"initialize EGL");
    const I attrs[]={0x3033,1,0x3022,8,0x3023,8,0x3024,8,0x3025,8,0x3040,8,0x3038}; P config=nullptr;
    need(eglChooseConfig(display,attrs,&config,1,&count)&&count==1,"EGL config"); need(eglBindAPI(0x30A2),"desktop GL");
    const I size[]={0x3057,256,0x3056,256,0x3038}, none[]={0x3038};
    surface=eglCreatePbufferSurface(display,config,size); context=eglCreateContext(display,config,nullptr,none); need(surface&&context&&eglMakeCurrent(display,surface,surface,context),"current EGL context");
    G(glGetString); G(glGetError); G(glGenTextures); G(glDeleteTextures); G(glBindTexture); G(glActiveTexture); G(glTexImage2D); G(glTexParameteri); G(glGetTexImage);
    G(glGenFramebuffers); G(glDeleteFramebuffers); G(glBindFramebuffer); G(glFramebufferTexture2D); G(glCheckFramebufferStatus); G(glGenRenderbuffers); G(glDeleteRenderbuffers); G(glBindRenderbuffer); G(glRenderbufferStorage); G(glFramebufferRenderbuffer);
    G(glCreateShader); G(glShaderSource); G(glCompileShader); G(glGetShaderiv); G(glGetShaderInfoLog); G(glCreateProgram); G(glAttachShader); G(glLinkProgram); G(glGetProgramiv); G(glGetProgramInfoLog); G(glUseProgram); G(glDeleteProgram); G(glDeleteShader);
    G(glGetUniformLocation); G(glUniform1i); G(glUniform1f); G(glUniform2f); G(glUniform4fv); G(glUniform4ui); G(glUniformMatrix4fv); G(glViewport); G(glMatrixMode); G(glLoadIdentity); G(glOrtho); G(glClampColor); G(glDrawBuffer); G(glReadBuffer); G(glClearColor); G(glClear); G(glColor3f); G(glEnable); G(glDisable); G(glBegin); G(glEnd); G(glTexCoord2f); G(glVertex2f); G(glFinish); G(glReadPixels);
    for (U e : {0x891Au,0x891Bu,0x891Cu}) glClampColor(e,0);
    error("initialize");
}

std::filesystem::path output;
bool first=true;
std::vector<F> run(const std::string& name,const std::string& shader,const std::vector<F>& image,I w,I h,U filter,U wrap,const std::array<F,2>& uv={0.25f,0.25f},const std::array<F,4>& v={1,2,4,8},const std::array<F,16>& matrix={1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1}) {
    need(image.size()==size_t(w*h*4),"input size");
    U texture=0,fbo=0,renderbuffer=0;
    glActiveTexture(0x84C0); glGenTextures(1,&texture); glBindTexture(0x0DE1,texture);
    glTexImage2D(0x0DE1,0,0x8814,w,h,0,0x1908,0x1406,const_cast<F*>(image.data()));
    glTexParameteri(0x0DE1,0x2801,filter); glTexParameteri(0x0DE1,0x2800,filter); glTexParameteri(0x0DE1,0x2802,wrap); glTexParameteri(0x0DE1,0x2803,wrap);
    std::vector<F> texels(image.size()); glGetTexImage(0x0DE1,0,0x1908,0x1406,texels.data());
    need(std::memcmp(image.data(),texels.data(),image.size()*4)==0,"texture upload changed bits");
    glGenFramebuffers(1,&fbo); glBindFramebuffer(0x8D40,fbo); glGenRenderbuffers(1,&renderbuffer); glBindRenderbuffer(0x8D41,renderbuffer); glRenderbufferStorage(0x8D41,0x8814,w,h);
    glFramebufferRenderbuffer(0x8D40,0x8CE0,0x8D41,renderbuffer); glFramebufferTexture2D(0x8D40,0x8CE1,0x0DE1,texture,0); need(glCheckFramebufferStatus(0x8D40)==0x8CD5,"complete FBO");
    U fragment=glCreateShader(0x8B30); const char* text=shader.c_str(); glShaderSource(fragment,1,&text,nullptr); glCompileShader(fragment); I ok=0; char log[8192]={};
    glGetShaderiv(fragment,0x8B81,&ok); glGetShaderInfoLog(fragment,sizeof(log),nullptr,log); need(ok!=0,"shader "+name+": "+log);
    U program=glCreateProgram(); glAttachShader(program,fragment); glLinkProgram(program); glGetProgramiv(program,0x8B82,&ok); glGetProgramInfoLog(program,sizeof(log),nullptr,log); need(ok!=0,"link "+name+": "+log); glUseProgram(program);
    I location=glGetUniformLocation(program,"img"); if(location>=0)glUniform1i(location,0);
    location=glGetUniformLocation(program,"uv"); if(location>=0)glUniform2f(location,uv[0],uv[1]);
    location=glGetUniformLocation(program,"v"); if(location>=0)glUniform4fv(location,1,v.data());
    location=glGetUniformLocation(program,"a"); if(location>=0)glUniform1f(location,0);
    location=glGetUniformLocation(program,"m"); if(location>=0)glUniformMatrix4fv(location,1,0,matrix.data());
    location=glGetUniformLocation(program,"raw"); if(location>=0)glUniform4ui(location,bits(v[0]),bits(v[1]),bits(v[2]),bits(v[3]));
    location=glGetUniformLocation(program,"uvraw"); if(location>=0)glUniform4ui(location,bits(uv[0]),bits(uv[1]),0,0);
    glViewport(0,0,w,h); glMatrixMode(0x1701); glLoadIdentity(); glOrtho(0,w,0,h,-1,1); glMatrixMode(0x1700); glLoadIdentity();
    glDrawBuffer(0x8CE0); glReadBuffer(0x8CE0); glClearColor(0.2f,0.2f,0.2f,0); glClear(0x4000); glColor3f(1,1,1); glEnable(0x0DE1);
    glBegin(7); glTexCoord2f(0,1);glVertex2f(0,F(h));glTexCoord2f(0,0);glVertex2f(0,0);glTexCoord2f(1,0);glVertex2f(F(w),0);glTexCoord2f(1,1);glVertex2f(F(w),F(h));glEnd(); glDisable(0x0DE1); glFinish();
    std::vector<F> pixels(image.size()); glReadPixels(0,0,w,h,0x1908,0x1406,pixels.data()); error(name.c_str());
    auto folder=output/name; std::filesystem::create_directories(folder); binary(folder/"input.bin",image);binary(folder/"output.bin",pixels); std::ofstream(folder/"fragment.glsl")<<shader;
    if(!first) { std::cout<<",\n"; }
    first=false;
    std::cout<<"{\"name\":"<<js(name)<<",\"width\":"<<w<<",\"height\":"<<h<<",\"filter\":"<<js(filter==0x2601?"GL_LINEAR":"GL_NEAREST")<<",\"wrap\":"<<js(wrap==0x2900?"GL_CLAMP":"GL_CLAMP_TO_EDGE")<<",\"uv_bits\":"<<bit_array({uv[0],uv[1]})<<",\"v_bits\":"<<bit_array({v.begin(),v.end()})<<",\"first_output_bits\":"<<bit_array(pixels)<<",\"upload_exact\":true,\"gl_error\":0}";
    glUseProgram(0);glDeleteProgram(program);glDeleteShader(fragment);glBindFramebuffer(0x8D40,0);glDeleteFramebuffers(1,&fbo);glDeleteRenderbuffers(1,&renderbuffer);glDeleteTextures(1,&texture);
    return pixels;
}

int main(int argc,char** argv) {
 try {
    need(argc==3,"usage: minimal_gl OUTPUT DEFAULT_INPUT_BIN"); output=argv[1];std::filesystem::create_directories(output);init();
    auto str=[](U n){return reinterpret_cast<const char*>(glGetString(n));};
    need(std::string(str(0x1F01)).find("llvmpipe")!=std::string::npos,"llvmpipe renderer required");
    std::cout<<"{\"purpose\":\"independent native EGL/OpenGL diagnostics; no OCIO linked\",\"vendor\":"<<js(str(0x1F00))<<",\"renderer\":"<<js(str(0x1F01))<<",\"version\":"<<js(str(0x1F02))<<",\"glsl\":"<<js(str(0x8B8C))<<",\"rows\":[\n";
    const std::string prefix="#version 120\nuniform sampler2D img; uniform vec2 uv; uniform vec4 v; uniform float a; uniform mat4 m;\n";
    const std::string copy=prefix+"void main(){gl_FragColor=texture2D(img,gl_TexCoord[0].st);}\n";
    const std::string coord=prefix+"void main(){gl_FragColor=texture2D(img,uv);}\n";
    const std::string modern="#version 450 compatibility\nuniform sampler2D img; uniform uvec4 raw; uniform uvec4 uvraw; uniform float a; uniform mat4 m;\n";
    const std::string modern_coord=modern+"void main(){vec2 p=uintBitsToFloat(uvraw.xy);gl_FragColor=texture(img,p);}\n";
    std::vector<F> finite(4*4*4);for(size_t i=0;i<finite.size();++i)finite[i]=F(i+1)/16;
    std::array<U,3> special={0x7fc00000,0x7f800000,0xff800000};
    for(U filter:{0x2601u,0x2600u}) {
      auto suffix=filter==0x2601?"_linear":"_nearest";
      auto exact=run(std::string("finite_identity")+suffix,copy,finite,4,4,filter,0x2900);
      need(exact==finite,"finite identity control");
      for(U wrap:{0x2900u,0x812Fu}) {
        std::string tag=std::string(suffix)+(wrap==0x2900?"_clamp":"_edge");
        run("finite_coordinate"+tag,coord,finite,4,4,filter,wrap,{0.125f,0.125f});
        for(size_t k=0;k<special.size();++k)for(size_t axis=0;axis<2;++axis){
          std::array<F,2> uv={0.125f,0.125f};uv[axis]=value(special[k]);std::string label="coordinate_"+std::to_string(k)+"_axis"+std::to_string(axis)+tag;
          run(label,coord,finite,4,4,filter,wrap,uv);
          run(label+"_glsl450_bits",modern_coord,finite,4,4,filter,wrap,uv);
        }
      }
      for(size_t k=0;k<special.size();++k) {
        auto image=finite;image[4]=value(special[k]);
        run("texel_"+std::to_string(k)+suffix,copy,image,4,4,filter,0x2900);
        run("neighbor_"+std::to_string(k)+suffix,coord,image,4,4,filter,0x2900,{0.125f,0.125f});
      }
      auto actual=std::string(argv[2])=="--generate-upstream-input"?default_image():read_binary(argv[2]);need(actual.size()==256*256*4,"actual default image");
      run(std::string("actual_identity")+suffix,copy,actual,256,256,filter,0x2900);
      run(std::string("actual_coordinate_echo")+suffix,prefix+"void main(){gl_FragColor=vec4(gl_TexCoord[0].st,0.0,1.0);}\n",actual,256,256,filter,0x2900);
      run(std::string("profile_nan_only")+suffix,copy,default_image(true,false,true),256,256,filter,0x2900);
      run(std::string("profile_inf_only")+suffix,copy,default_image(false,true,true),256,256,filter,0x2900);
      run(std::string("profile_finite_wide")+suffix,copy,default_image(false,false,true),256,256,filter,0x2900);
      run(std::string("profile_finite_unit")+suffix,copy,default_image(false,false,false),256,256,filter,0x2900);
    }
    const std::array<std::pair<std::string,std::string>,7> math={{{"echo","v"},{"matrix_uniform","m*v"},{"matrix_literal","mat4(1.0)*v"},{"component_identity","v*vec4(1.0)"},{"scalar_uniform_zero","vec4(v.x*a,v.y*a,v.z*a,v.w*a)"},{"scalar_literal_zero","v*0.0"},{"dot_zero","vec4(dot(v,vec4(a)))"}}};
    for(int k=-1;k<3;++k)for(int channel=0;channel<(k<0?1:4);++channel){
      std::array<F,4> v={1,2,4,8};if(k>=0)v[channel]=value(special[k]);
      auto tag=k<0?"_finite":"_special"+std::to_string(k)+"_channel"+std::to_string(channel);
      for(auto& item:math){
        run(item.first+tag,prefix+"void main(){gl_FragColor="+item.second+";}\n",finite,4,4,0x2600,0x812F,{0.125f,0.125f},v);
        run(item.first+tag+"_glsl450_bits",modern+"void main(){vec4 v=uintBitsToFloat(raw);gl_FragColor="+item.second+";}\n",finite,4,4,0x2600,0x812F,{0.125f,0.125f},v);
      }
    }
    std::cout<<"\n],\"completed\":true}\n";
    eglMakeCurrent(display,nullptr,nullptr,nullptr);eglDestroyContext(display,context);eglDestroySurface(display,surface);eglTerminate(display);
    return 0;
 } catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}
}
