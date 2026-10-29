// SPDX-License-Identifier: BSD-3-Clause
// Diagnostic branch only. Independent native GL replay of public upstream fixtures.
// No OCIO or Rust code is linked. Inputs, resources and shader source are captured
// observations, and no numerical expected output is calculated here.
#define main unused_nonfinite_diagnostic_main
#include "nonfinite.cpp"
#undef main
#include <cmath>
#include <iterator>
API(void, glTexImage1D, (U,I,I,I,I,U,U,const void*));
API(void, glTexImage3D, (U,I,I,I,I,I,I,U,U,const void*));

struct Resource { std::string sampler,file; I dimensions,width,height,edge,channels; U filter; };

std::vector<F> replay(const std::string& shader, const std::vector<F>& image,
                      const std::vector<Resource>& resources, const std::array<F,4>& control={0,0,0,0}) {
    const I w=256,h=256;
    need(image.size()==size_t(w*h*4),"complete input image");
    need([](const std::vector<F>& a){for(F v:a)if(!std::isfinite(v))return false;return true;}(image),"public input is finite");
    U texture=0,fbo=0,renderbuffer=0;
    glActiveTexture(0x84C0);glGenTextures(1,&texture);glBindTexture(0x0DE1,texture);
    glTexImage2D(0x0DE1,0,0x8814,w,h,0,0x1908,0x1406,const_cast<F*>(image.data()));
    glTexParameteri(0x0DE1,0x2801,0x2601);glTexParameteri(0x0DE1,0x2800,0x2601);
    glTexParameteri(0x0DE1,0x2802,0x2900);glTexParameteri(0x0DE1,0x2803,0x2900);
    std::vector<F> uploaded(image.size());glGetTexImage(0x0DE1,0,0x1908,0x1406,uploaded.data());
    need(std::memcmp(uploaded.data(),image.data(),image.size()*4)==0,"input upload exact");
    glGenFramebuffers(1,&fbo);glBindFramebuffer(0x8D40,fbo);glGenRenderbuffers(1,&renderbuffer);
    glBindRenderbuffer(0x8D41,renderbuffer);glRenderbufferStorage(0x8D41,0x8814,w,h);
    glFramebufferRenderbuffer(0x8D40,0x8CE0,0x8D41,renderbuffer);
    glFramebufferTexture2D(0x8D40,0x8CE1,0x0DE1,texture,0);
    need(glCheckFramebufferStatus(0x8D40)==0x8CD5,"complete FBO");
    U fragment=glCreateShader(0x8B30);const char* text=shader.c_str();
    glShaderSource(fragment,1,&text,nullptr);glCompileShader(fragment);I ok=0;char log[8192]={};
    glGetShaderiv(fragment,0x8B81,&ok);glGetShaderInfoLog(fragment,sizeof(log),nullptr,log);need(ok!=0,std::string("compile: ")+log);
    U program=glCreateProgram();glAttachShader(program,fragment);glLinkProgram(program);
    glGetProgramiv(program,0x8B82,&ok);glGetProgramInfoLog(program,sizeof(log),nullptr,log);need(ok!=0,std::string("link: ")+log);
    glUseProgram(program);I location=glGetUniformLocation(program,"img");need(location>=0,"image sampler active");glUniform1i(location,0);
    location=glGetUniformLocation(program,"probe_control");if(location>=0)glUniform4fv(location,1,control.data());
    std::vector<U> handles;
    for(const auto& r:resources) {
        const auto values=read_binary(r.file);
        size_t count=size_t(r.width)*size_t(r.height)*size_t(r.channels);
        if(r.dimensions==3)count=size_t(r.edge)*r.edge*r.edge*3;
        need(values.size()==count,"resource dimensions");
        U handle=0;glGenTextures(1,&handle);handles.push_back(handle);
        U unit=U(handles.size());U target=r.dimensions==1?0x0DE0:r.dimensions==2?0x0DE1:0x806F;
        U format=r.channels==1?0x1903:0x1907;I internal=r.channels==1?0x822E:0x8815;
        glActiveTexture(0x84C0+unit);glBindTexture(target,handle);
        glTexParameteri(target,0x2801,r.filter);glTexParameteri(target,0x2800,r.filter);
        for(U axis:{0x2802U,0x2803U,0x8072U})glTexParameteri(target,axis,0x812F);
        if(r.dimensions==1)glTexImage1D(target,0,internal,r.width,0,format,0x1406,values.data());
        else if(r.dimensions==2)glTexImage2D(target,0,internal,r.width,r.height,0,format,0x1406,const_cast<F*>(values.data()));
        else glTexImage3D(target,0,internal,r.edge,r.edge,r.edge,0,format,0x1406,values.data());
        std::vector<F> actual(values.size());glGetTexImage(target,0,format,0x1406,actual.data());
        need(std::memcmp(actual.data(),values.data(),values.size()*4)==0,"resource upload exact");
        location=glGetUniformLocation(program,r.sampler.c_str());if(location>=0)glUniform1i(location,I(unit));
    }
    glActiveTexture(0x84C0);glViewport(0,0,w,h);glMatrixMode(0x1701);glLoadIdentity();glOrtho(0,w,0,h,-1,1);glMatrixMode(0x1700);glLoadIdentity();
    glDrawBuffer(0x8CE0);glReadBuffer(0x8CE0);glClearColor(0.2f,0.2f,0.2f,0);glClear(0x4000);glColor3f(1,1,1);glEnable(0x0DE1);
    glBegin(7);glTexCoord2f(0,1);glVertex2f(0,F(h));glTexCoord2f(0,0);glVertex2f(0,0);glTexCoord2f(1,0);glVertex2f(F(w),0);glTexCoord2f(1,1);glVertex2f(F(w),F(h));glEnd();glDisable(0x0DE1);glFinish();
    std::vector<F> pixels(image.size());glReadPixels(0,0,w,h,0x1908,0x1406,pixels.data());error("native resource replay");
    glUseProgram(0);glDeleteProgram(program);glDeleteShader(fragment);glBindFramebuffer(0x8D40,0);
    glDeleteFramebuffers(1,&fbo);glDeleteRenderbuffers(1,&renderbuffer);glDeleteTextures(1,&texture);
    if(!handles.empty())glDeleteTextures(I(handles.size()),handles.data());
    return pixels;
}

#ifndef FINITE_REPLAY_LIBRARY_ONLY
int main(int argc,char** argv) {
    try {
        need(argc==3,"usage: finite-replay MANIFEST LABEL");init();
        glTexImage1D=resolve<glTexImage1D_t>("glTexImage1D");glTexImage3D=resolve<glTexImage3D_t>("glTexImage3D");
        std::ifstream manifest(argv[1]);need(bool(manifest),"manifest open");size_t count=0;manifest>>count;need(count==5,"five public fixtures");
        bool all=true;std::cout<<"{\"renderer\":"<<js(reinterpret_cast<const char*>(glGetString(0x1F01)))<<",\"label\":"<<js(argv[2])<<",\"cases\":[";
        for(size_t i=0;i<count;++i) {
            std::string directory,shader_file;size_t n=0;manifest>>directory>>shader_file>>n;
            std::vector<Resource> resources;
            for(size_t j=0;j<n;++j){Resource r{};manifest>>r.sampler>>r.dimensions>>r.width>>r.height>>r.edge>>r.channels>>r.filter>>r.file;resources.push_back(r);}
            need(bool(manifest),"complete manifest row");
            std::ifstream stream(shader_file);need(bool(stream),"compiled shader open");std::string shader((std::istreambuf_iterator<char>(stream)),{});
            const std::filesystem::path folder(directory);
            const auto pixels=replay(shader,read_binary(folder/"input.bin"),resources);
            const auto expected=read_binary(folder/"port.bin");need(pixels.size()==expected.size(),"full observed readback length");
            size_t mismatches=0,first_bad=pixels.size();
            for(size_t j=0;j<pixels.size();++j)if(bits(pixels[j])!=bits(expected[j])){++mismatches;if(first_bad==pixels.size())first_bad=j;}
            const bool exact=mismatches==0;all=all&&exact;
            const auto point=read_binary(folder/"point-input.bin");
            const auto point_gpu=read_binary(folder/"point-port.bin");
            need(point.size()==4&&point_gpu.size()==4,"actual four-component probe point");
            std::vector<F> constant_image(pixels.size());
            for(size_t j=0;j<constant_image.size();++j)constant_image[j]=point[j%4];
            const auto constant_pixels=replay(shader,constant_image,resources);
            size_t constant_mismatches=0;
            for(size_t j=0;j<constant_pixels.size();++j)if(bits(constant_pixels[j])!=bits(point_gpu[j%4]))++constant_mismatches;
            binary(folder/(std::string("constant-native-")+argv[2]+".bin"),constant_pixels);
            binary(folder/(std::string("native-")+argv[2]+".bin"),pixels);
            if(i)std::cout<<",";
            std::cout<<"{\"name\":"<<js(folder.filename().string())<<",\"resources\":"<<n<<",\"full_gpu_bits_exact\":"<<(exact?"true":"false")<<",\"mismatches\":"<<mismatches<<",\"first_mismatch\":"<<first_bad<<",\"constant_input_matches_original_pixel\":"<<(constant_mismatches==0?"true":"false")<<",\"constant_component_mismatches\":"<<constant_mismatches<<"}";
        }
        std::cout<<"],\"all_exact\":"<<(all?"true":"false")<<"}\n";
        eglMakeCurrent(display,nullptr,nullptr,nullptr);eglDestroyContext(display,context);eglDestroySurface(display,surface);eglTerminate(display);
        return all?0:1;
    }catch(const std::exception& e){std::cerr<<"native finite replay failed: "<<e.what()<<"\n";return 2;}
}

#endif
