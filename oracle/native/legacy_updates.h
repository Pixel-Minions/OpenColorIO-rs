// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

// P87: direct public-API observations after extraction. No processor math.
// OpenColorIO.h:2707-2709, 3476-3483; OpenColorTransforms.h:781-898 @ v2.5.2.
#pragma once

static constexpr uint32_t UPDATES_MAGIC = 0x38504349;

struct DynamicUpdate {
    bool shader;
    OCIO::DynamicPropertyType type;
    OCIO::TransformRcPtr value;
};

static std::vector<DynamicUpdate> read_dynamic_updates(Reader & r) {
    std::vector<DynamicUpdate> updates;
    auto count = r.u();
    for (uint32_t i = 0; i < count; ++i) {
        auto source = r.u();
        if (source > 1) throw std::runtime_error("unknown dynamic update source");
        auto type = static_cast<OCIO::DynamicPropertyType>(r.u());
        updates.push_back({source != 0, type, read_transform(r)});
    }
    return updates;
}

template<class T> static std::shared_ptr<T> update_value(const DynamicUpdate & update) {
    auto value = std::dynamic_pointer_cast<T>(update.value);
    if (!value) throw std::runtime_error("dynamic update value transform has the wrong type");
    return value;
}

static void apply_dynamic_update(const DynamicUpdate & update,
                                const OCIO::ConstProcessorRcPtr & processor,
                                const OCIO::GpuShaderDescRcPtr & desc) {
    auto prop = update.shader ? desc->getDynamicProperty(update.type)
                              : processor->getDynamicProperty(update.type);
    using namespace OCIO::DynamicPropertyValue;
    switch (update.type) {
        case OCIO::DYNAMIC_PROPERTY_EXPOSURE:
            AsDouble(prop)->setValue(update_value<OCIO::ExposureContrastTransform>(update)->getExposure()); break;
        case OCIO::DYNAMIC_PROPERTY_CONTRAST:
            AsDouble(prop)->setValue(update_value<OCIO::ExposureContrastTransform>(update)->getContrast()); break;
        case OCIO::DYNAMIC_PROPERTY_GAMMA:
            AsDouble(prop)->setValue(update_value<OCIO::ExposureContrastTransform>(update)->getGamma()); break;
        case OCIO::DYNAMIC_PROPERTY_GRADING_PRIMARY:
            AsGradingPrimary(prop)->setValue(update_value<OCIO::GradingPrimaryTransform>(update)->getValue()); break;
        case OCIO::DYNAMIC_PROPERTY_GRADING_TONE:
            AsGradingTone(prop)->setValue(update_value<OCIO::GradingToneTransform>(update)->getValue()); break;
        case OCIO::DYNAMIC_PROPERTY_GRADING_RGBCURVE:
            AsGradingRGBCurve(prop)->setValue(update_value<OCIO::GradingRGBCurveTransform>(update)->getValue()); break;
        case OCIO::DYNAMIC_PROPERTY_GRADING_HUECURVE:
            AsGradingHueCurve(prop)->setValue(update_value<OCIO::GradingHueCurveTransform>(update)->getValue()); break;
        default: throw std::runtime_error("unknown dynamic update property");
    }
}

// Getter order mirrors the existing legacy result, then reads the source processor's
// properties too. Keeping the original descriptor observes its live uniform callbacks;
// recreating or extracting a fresh descriptor here would miss stale callback defects.
static std::string dynamic_state(const OCIO::ConstProcessorRcPtr & processor,
                                 const OCIO::ConstGPUProcessorRcPtr & gpu,
                                 const OCIO::GpuShaderDescRcPtr & desc,
                                 const std::vector<std::string> & log) {
    std::ostringstream bytes(std::ios::out | std::ios::binary);
    Writer w{bytes};
    w.text(processor->getCacheID()); w.text(gpu->getCacheID());
    w.u(gpu->isNoOp()); w.u(gpu->hasChannelCrosstalk());
    write_shader(w, desc);
    std::vector<OCIO::DynamicPropertyRcPtr> props;
    for (auto type : {OCIO::DYNAMIC_PROPERTY_EXPOSURE, OCIO::DYNAMIC_PROPERTY_CONTRAST,
                      OCIO::DYNAMIC_PROPERTY_GAMMA, OCIO::DYNAMIC_PROPERTY_GRADING_PRIMARY,
                      OCIO::DYNAMIC_PROPERTY_GRADING_RGBCURVE,
                      OCIO::DYNAMIC_PROPERTY_GRADING_HUECURVE,
                      OCIO::DYNAMIC_PROPERTY_GRADING_TONE}) {
        if (processor->hasDynamicProperty(type)) props.push_back(processor->getDynamicProperty(type));
    }
    w.u(static_cast<uint32_t>(props.size()));
    for (const auto & prop : props) write_property(w, prop);
    w.u(static_cast<uint32_t>(log.size()));
    for (const auto & message : log) w.text(message);
    return bytes.str();
}
