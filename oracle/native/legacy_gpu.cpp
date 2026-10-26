// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

// O7.1: ask the pinned wheel's own library for its processor and GPU description. This is a
// client of the public API (OpenColorIO.h:2560-2900, 3359-3824 @ v2.5.2), not a legacy bake.
#include "transport.h"
#include <sstream>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <windows.h>
#else
#include <sys/resource.h>
#endif

static constexpr uint32_t MAGIC = 0x374f4349;

// Direct reads of public DynamicProperty value interfaces (OpenColorTransforms.h:781-898).
static void write_property(Writer & w, OCIO::DynamicPropertyRcPtr prop) {
    auto type = prop->getType(); w.u(type);
    using namespace OCIO::DynamicPropertyValue;
    if (type == OCIO::DYNAMIC_PROPERTY_EXPOSURE || type == OCIO::DYNAMIC_PROPERTY_CONTRAST || type == OCIO::DYNAMIC_PROPERTY_GAMMA) w.d(AsDouble(prop)->getValue());
    else if (type == OCIO::DYNAMIC_PROPERTY_GRADING_PRIMARY) write_primary(w, AsGradingPrimary(prop)->getValue());
    else if (type == OCIO::DYNAMIC_PROPERTY_GRADING_TONE) write_tone(w, AsGradingTone(prop)->getValue());
    else if (type == OCIO::DYNAMIC_PROPERTY_GRADING_RGBCURVE) {
        const auto & value = AsGradingRGBCurve(prop)->getValue();
        for (unsigned i = 0; i < 4; ++i) write_curve(w, value->getCurve(static_cast<OCIO::RGBCurveType>(i)));
    } else if (type == OCIO::DYNAMIC_PROPERTY_GRADING_HUECURVE) {
        const auto & value = AsGradingHueCurve(prop)->getValue();
        for (unsigned i = 0; i < 8; ++i) write_curve(w, value->getCurve(static_cast<OCIO::HueCurveType>(i)));
        w.u(value->getDrawCurveOnly());
    } else throw std::runtime_error("unknown native dynamic property");
}

// Direct GpuShaderDesc getter order matches gpu.py::_shader (OpenColorIO.h:3725-3816).
static void write_shader(Writer & w, const OCIO::GpuShaderDescRcPtr & desc) {
    w.text(desc->getCacheID()); w.text(desc->getShaderText()); w.u(desc->getLanguage());
    w.text(desc->getFunctionName()); w.text(desc->getPixelName()); w.text(desc->getResourcePrefix()); w.text(desc->getUniqueID());
    w.u(desc->getDescriptorSetIndex()); w.u(desc->getTextureBindingStart()); w.u(desc->getTextureMaxWidth()); w.u(desc->getAllowTexture1D());
    w.u(static_cast<uint32_t>(desc->getUniformBufferSize())); w.u(desc->getNumUniforms());
    for (unsigned i = 0; i < desc->getNumUniforms(); ++i) {
        OCIO::GpuShaderDesc::UniformData data;
        w.text(desc->getUniform(i, data)); w.u(data.m_type); w.u(static_cast<uint32_t>(data.m_bufferOffset));
        switch (data.m_type) {
            case OCIO::UNIFORM_DOUBLE: w.d(data.m_getDouble()); break;
            case OCIO::UNIFORM_BOOL: w.u(data.m_getBool()); break;
            case OCIO::UNIFORM_FLOAT3: { const auto & v = data.m_getFloat3(); w.blob(v.data(), 3 * sizeof(float)); break; }
            case OCIO::UNIFORM_VECTOR_FLOAT: { auto n = data.m_vectorFloat.m_getSize(); w.blob(data.m_vectorFloat.m_getVector(), static_cast<size_t>(n) * sizeof(float)); break; }
            case OCIO::UNIFORM_VECTOR_INT: { auto n = data.m_vectorInt.m_getSize(); w.blob(data.m_vectorInt.m_getVector(), static_cast<size_t>(n) * sizeof(int)); break; }
            default: break;
        }
    }
    w.u(desc->getNumTextures());
    for (unsigned i = 0; i < desc->getNumTextures(); ++i) {
        const char * name = nullptr; const char * sampler = nullptr; unsigned width{}, height{};
        OCIO::GpuShaderCreator::TextureType channel{}; OCIO::GpuShaderCreator::TextureDimensions dimensions{}; OCIO::Interpolation interpolation{};
        desc->getTexture(i, name, sampler, width, height, channel, dimensions, interpolation);
        w.text(name); w.text(sampler); w.u(width); w.u(height); w.u(channel); w.u(dimensions); w.u(interpolation); w.u(desc->getTextureShaderBindingIndex(i));
        const float * values = nullptr; desc->getTextureValues(i, values);
        const size_t channels = channel == OCIO::GpuShaderCreator::TEXTURE_RED_CHANNEL ? 1 : 3;
        w.blob(values, static_cast<size_t>(width) * height * channels * sizeof(float));
    }
    w.u(desc->getNum3DTextures());
    for (unsigned i = 0; i < desc->getNum3DTextures(); ++i) {
        const char * name = nullptr; const char * sampler = nullptr; unsigned edge{}; OCIO::Interpolation interpolation{};
        desc->get3DTexture(i, name, sampler, edge, interpolation);
        w.text(name); w.text(sampler); w.u(edge); w.u(interpolation); w.u(desc->get3DTextureShaderBindingIndex(i));
        const float * values = nullptr; desc->get3DTextureValues(i, values);
        w.blob(values, static_cast<size_t>(edge) * edge * edge * 3 * sizeof(float));
    }
    w.u(desc->getNumDynamicProperties());
    for (unsigned i = 0; i < desc->getNumDynamicProperties(); ++i) write_property(w, desc->getDynamicProperty(i));
}

int main() {
#ifdef _WIN32
    _setmode(_fileno(stdin), _O_BINARY); _setmode(_fileno(stdout), _O_BINARY);
    // Boundary probes must not exhaust the workstation. The job applies only to this helper.
    HANDLE memory_job = CreateJobObjectW(nullptr, nullptr);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
    limits.ProcessMemoryLimit = static_cast<SIZE_T>(8ull << 30);
    if (!memory_job || !SetInformationJobObject(memory_job, JobObjectExtendedLimitInformation, &limits, sizeof(limits))
        || !AssignProcessToJobObject(memory_job, GetCurrentProcess())) { std::cerr << "native helper memory cap failed"; return 3; }
#else
    struct rlimit limits{ static_cast<rlim_t>(8ull << 30), static_cast<rlim_t>(8ull << 30) };
    if (setrlimit(RLIMIT_AS, &limits)) { std::cerr << "native helper memory cap failed"; return 3; }
#endif
    static_assert(sizeof(float) == 4 && sizeof(double) == 8 && sizeof(int) == 4, "x86-64 wheel ABI");
    Reader r{std::cin}; Writer w{std::cout};
    std::vector<std::string> log;
    OCIO::SetLoggingFunction([&](const char * message) { log.emplace_back(message); });
    std::string stage = "config", exception_type, exception_message, processor_id;
    OCIO::ConstGPUProcessorRcPtr gpu;
    OCIO::GpuShaderDescRcPtr desc;
    bool extracted = false;
    try {
        if (r.u() != MAGIC) throw std::runtime_error("wrong native helper protocol");
        auto kind = r.u(); auto source = r.text(); auto cache_off = r.u();
        OCIO::ConstConfigRcPtr config;
        if (kind == 0) config = OCIO::Config::CreateRaw();
        else if (kind == 1) config = OCIO::Config::Create();
        else if (kind == 2) { std::istringstream text(source); config = OCIO::Config::CreateFromStream(text); }
        else if (kind == 3) config = OCIO::Config::CreateFromBuiltinConfig(source.c_str());
        else if (kind == 4) config = OCIO::Config::CreateFromFile(source.c_str());
        else if (kind == 5) {
            // Config.cpp:1285-1305; setter selects the wheel's latest minor for
            // this major, before the optional processor-cache setting below.
            auto created = OCIO::Config::Create();
            created->setMajorVersion(r.u());
            config = created;
        }
        else throw std::runtime_error("unknown native config kind");
        if (cache_off) config->setProcessorCacheFlags(OCIO::PROCESSOR_CACHE_OFF);
        stage = "transform";
        OCIO::TransformRcPtr transform; std::string src, dst;
        if (r.u()) transform = read_transform(r);
        else { src = r.text(); dst = r.text(); }
        const auto direction = static_cast<OCIO::TransformDirection>(r.u());
        stage = "processor";
        auto processor = transform ? config->getProcessor(transform, direction) : config->getProcessor(src.c_str(), dst.c_str());
        processor_id = processor->getCacheID();
        stage = "gpu_processor";
        const auto requests = r.u();
        for (uint32_t i = 0; i < requests; ++i) {
            auto legacy = r.u(); auto flags = static_cast<OCIO::OptimizationFlags>(r.u()); auto edge = r.u();
            auto requested_gpu = legacy ? processor->getOptimizedLegacyGPUProcessor(flags, edge) : processor->getOptimizedGPUProcessor(flags);
            // Earlier requests only prime this processor's cache. A failing final request
            // must not publish an earlier successful GPU as though it were the final result.
            if (i + 1 == requests) gpu = requested_gpu;
        }
        if (!gpu) throw std::runtime_error("no native GPU request");
        stage = "shader_desc";
        desc = OCIO::GpuShaderDesc::CreateShaderDesc();
        desc->setLanguage(static_cast<OCIO::GpuLanguage>(r.u()));
        desc->setFunctionName(r.text().c_str()); desc->setPixelName(r.text().c_str()); desc->setResourcePrefix(r.text().c_str()); desc->setUniqueID(r.text().c_str());
        auto set = r.u(); auto binding = r.u(); desc->setDescriptorSetIndex(set, binding);
        desc->setTextureMaxWidth(r.u()); desc->setAllowTexture1D(r.u() != 0);
        if (std::cin.peek() != std::char_traits<char>::eof()) throw std::runtime_error("trailing native helper request bytes");
        stage = "extract";
        gpu->extractGpuShaderInfo(desc); extracted = true;
    } catch (const OCIO::ExceptionMissingFile & e) { exception_type = "ExceptionMissingFile"; exception_message = e.what(); }
      catch (const OCIO::Exception & e) { exception_type = "Exception"; exception_message = e.what(); }
      catch (const std::exception & e) { exception_type = "RuntimeError"; exception_message = e.what(); }
    OCIO::ResetToDefaultLoggingFunction();
    try {
        w.u(MAGIC); w.text(OCIO::GetVersion());
#ifdef _MSC_VER
        w.text("msvc-release-md");
#else
        w.text(std::string("libstdc++-cxx11-abi-") + (_GLIBCXX_USE_CXX11_ABI ? "1" : "0"));
#endif
        w.text(exception_type); w.text(exception_message); w.text(stage);
        w.u(static_cast<uint32_t>(log.size())); for (const auto & message : log) w.text(message);
        w.u(!processor_id.empty()); if (!processor_id.empty()) w.text(processor_id);
        w.u(gpu != nullptr);
        if (gpu) { w.text(gpu->getCacheID()); w.u(gpu->isNoOp()); w.u(gpu->hasChannelCrosstalk()); }
        w.u(extracted); if (extracted) write_shader(w, desc);
    } catch (const std::exception & e) { std::cerr << e.what(); return 2; }
    return 0;
}
