// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

// O7.1: lossless copies through OCIO 2.5.2's public getters/setters only.
// Public contracts: include/OpenColorIO/OpenColorTransforms.h:27-2213 @ v2.5.2.
// Wire peer: oracle/ocio_oracle/native_transport.py. No processor arithmetic is implemented.
#pragma once
#include <OpenColorIO/OpenColorIO.h>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <type_traits>
#include <vector>

namespace OCIO = OCIO_NAMESPACE;

struct Reader {
    std::istream & stream;
    template<class T> T pod() {
        T value{};
        stream.read(reinterpret_cast<char *>(&value), sizeof(value));
        if (!stream) throw std::runtime_error("truncated native helper request");
        return value;
    }
    uint32_t u() { return pod<uint32_t>(); }
    double d() { return pod<double>(); }
    std::string text() {
        const auto count = u();
        if (count > (1u << 30)) throw std::runtime_error("oversized native helper field");
        std::string value(count, '\0');
        stream.read(value.data(), count);
        if (!stream) throw std::runtime_error("truncated native helper field");
        return value;
    }
    template<size_t N> void ds(double (&values)[N]) { for (auto & value : values) value = d(); }
    std::vector<float> floats() {
        auto bytes = text();
        if (bytes.size() % sizeof(float)) throw std::runtime_error("unaligned float data");
        std::vector<float> values(bytes.size() / sizeof(float));
        if (!bytes.empty()) std::memcpy(values.data(), bytes.data(), bytes.size());
        return values;
    }
};

struct Writer {
    std::ostream & stream;
    template<class T> void pod(T value) { stream.write(reinterpret_cast<const char *>(&value), sizeof(value)); }
    void u(uint32_t value) { pod(value); }
    void d(double value) { pod(value); }
    void blob(const void * values, size_t bytes) {
        if (bytes > UINT32_MAX) throw std::runtime_error("oversized native helper response");
        u(static_cast<uint32_t>(bytes));
        if (bytes) stream.write(static_cast<const char *>(values), bytes);
    }
    void text(const std::string & value) { blob(value.data(), value.size()); }
    void text(const char * value) { text(std::string(value)); }
};

inline void read_metadata_contents(Reader & r, OCIO::FormatMetadata & metadata) {
    auto attributes = r.u();
    for (uint32_t i = 0; i < attributes; ++i) {
        auto key = r.text(); auto item = r.text();
        metadata.addAttribute(key.c_str(), item.c_str());
    }
    auto children = r.u();
    for (uint32_t i = 0; i < children; ++i) {
        auto name = r.text(); auto value = r.text();
        metadata.addChildElement(name.c_str(), value.c_str());
        read_metadata_contents(r, metadata.getChildElement(metadata.getNumChildrenElements() - 1));
    }
}
inline void read_metadata(Reader & r, OCIO::FormatMetadata & metadata) {
    metadata.clear();
    auto name = r.text(); auto value = r.text();
    // ROOT is reserved and its setters reject even setting unchanged ROOT/empty values.
    if (name != metadata.getElementName()) metadata.setElementName(name.c_str());
    if (value != metadata.getElementValue()) metadata.setElementValue(value.c_str());
    read_metadata_contents(r, metadata);
}

template<class T, class = void> struct HasMetadata : std::false_type {};
template<class T> struct HasMetadata<T, std::void_t<decltype(std::declval<T>().getFormatMetadata())>> : std::true_type {};
template<class T> OCIO::TransformRcPtr finish_transform(Reader & r, T transform, OCIO::TransformDirection direction) {
    transform->setDirection(direction);
    if (r.u()) {
        if constexpr (HasMetadata<typename T::element_type>::value) read_metadata(r, transform->getFormatMetadata());
        else throw std::runtime_error("unexpected transform metadata");
    }
    return transform;
}

inline OCIO::GradingRGBM read_rgbm(Reader & r) { double v[4]; r.ds(v); return OCIO::GradingRGBM(v); }
inline void write_rgbm(Writer & w, const OCIO::GradingRGBM & v) { w.d(v.m_red); w.d(v.m_green); w.d(v.m_blue); w.d(v.m_master); }
inline OCIO::GradingPrimary read_primary(Reader & r, OCIO::GradingStyle style) {
    OCIO::GradingPrimary v(style);
    for (auto p : { &v.m_brightness, &v.m_contrast, &v.m_gamma, &v.m_offset, &v.m_exposure, &v.m_lift, &v.m_gain }) *p = read_rgbm(r);
    for (auto p : { &v.m_saturation, &v.m_pivot, &v.m_pivotBlack, &v.m_pivotWhite, &v.m_clampBlack, &v.m_clampWhite }) *p = r.d();
    return v;
}
inline void write_primary(Writer & w, const OCIO::GradingPrimary & v) {
    for (auto p : { &v.m_brightness, &v.m_contrast, &v.m_gamma, &v.m_offset, &v.m_exposure, &v.m_lift, &v.m_gain }) write_rgbm(w, *p);
    for (auto p : { &v.m_saturation, &v.m_pivot, &v.m_pivotBlack, &v.m_pivotWhite, &v.m_clampBlack, &v.m_clampWhite }) w.d(*p);
}
inline OCIO::GradingTone read_tone(Reader & r, OCIO::GradingStyle style) {
    OCIO::GradingTone v(style);
    for (auto p : { &v.m_blacks, &v.m_shadows, &v.m_midtones, &v.m_highlights, &v.m_whites }) { double a[6]; r.ds(a); *p = OCIO::GradingRGBMSW(a); }
    v.m_scontrast = r.d(); return v;
}
inline void write_tone(Writer & w, const OCIO::GradingTone & v) {
    for (auto p : { &v.m_blacks, &v.m_shadows, &v.m_midtones, &v.m_highlights, &v.m_whites }) {
        for (auto n : { p->m_red, p->m_green, p->m_blue, p->m_master, p->m_start, p->m_width }) w.d(n);
    }
    w.d(v.m_scontrast);
}
inline void read_curve(Reader & r, OCIO::GradingBSplineCurveRcPtr curve) {
    curve->setSplineType(static_cast<OCIO::BSplineType>(r.u()));
    const auto count = r.u(); curve->setNumControlPoints(count);
    for (uint32_t i = 0; i < count; ++i) { auto & p = curve->getControlPoint(i); p.m_x = static_cast<float>(r.d()); p.m_y = static_cast<float>(r.d()); }
    if (r.u()) for (uint32_t i = 0; i < count; ++i) curve->setSlope(i, static_cast<float>(r.d()));
}
inline void write_curve(Writer & w, OCIO::ConstGradingBSplineCurveRcPtr curve) {
    w.u(curve->getSplineType()); w.u(static_cast<uint32_t>(curve->getNumControlPoints()));
    for (size_t i = 0; i < curve->getNumControlPoints(); ++i) { const auto & p = curve->getControlPoint(i); w.d(p.m_x); w.d(p.m_y); }
    w.u(!curve->slopesAreDefault());
    if (!curve->slopesAreDefault()) for (size_t i = 0; i < curve->getNumControlPoints(); ++i) w.d(curve->getSlope(i));
}
inline OCIO::GradingRGBCurveRcPtr read_rgb_curves(Reader & r, OCIO::GradingStyle style) {
    auto value = OCIO::GradingRGBCurve::Create(style);
    for (unsigned i = 0; i < 4; ++i) read_curve(r, value->getCurve(static_cast<OCIO::RGBCurveType>(i)));
    return value;
}
inline OCIO::GradingHueCurveRcPtr read_hue_curves(Reader & r, OCIO::GradingStyle style) {
    auto value = OCIO::GradingHueCurve::Create(style);
    for (unsigned i = 0; i < 8; ++i) read_curve(r, value->getCurve(static_cast<OCIO::HueCurveType>(i)));
    value->setDrawCurveOnly(r.u() != 0); return value;
}
template<class T> void read_log(Reader & r, T t) {
    t->setBase(r.d()); double v[3];
    r.ds(v); t->setLogSideSlopeValue(v); r.ds(v); t->setLogSideOffsetValue(v);
    r.ds(v); t->setLinSideSlopeValue(v); r.ds(v); t->setLinSideOffsetValue(v);
}

inline OCIO::TransformRcPtr read_transform(Reader & r) {
    const auto name = r.text(); const auto dir = static_cast<OCIO::TransformDirection>(r.u());
#define DONE return finish_transform(r, t, dir)
    if (name == "GroupTransform") { auto t = OCIO::GroupTransform::Create(); auto n = r.u(); for (uint32_t i = 0; i < n; ++i) t->appendTransform(read_transform(r)); DONE; }
    if (name == "AllocationTransform") { auto t = OCIO::AllocationTransform::Create(); t->setAllocation(static_cast<OCIO::Allocation>(r.u())); auto v = r.floats(); t->setVars(static_cast<int>(v.size()), v.data()); DONE; }
    if (name == "BuiltinTransform") { auto t = OCIO::BuiltinTransform::Create(); t->setStyle(r.text().c_str()); DONE; }
    if (name == "CDLTransform") { auto t = OCIO::CDLTransform::Create(); t->setStyle(static_cast<OCIO::CDLStyle>(r.u())); double v[9]; r.ds(v); t->setSOP(v); t->setSat(r.d()); DONE; }
    if (name == "ColorSpaceTransform") { auto t = OCIO::ColorSpaceTransform::Create(); t->setSrc(r.text().c_str()); t->setDst(r.text().c_str()); t->setDataBypass(r.u() != 0); DONE; }
    if (name == "DisplayViewTransform") { auto t = OCIO::DisplayViewTransform::Create(); t->setSrc(r.text().c_str()); t->setDisplay(r.text().c_str()); t->setView(r.text().c_str()); t->setLooksBypass(r.u() != 0); t->setDataBypass(r.u() != 0); DONE; }
    if (name == "LookTransform") { auto t = OCIO::LookTransform::Create(); t->setSrc(r.text().c_str()); t->setDst(r.text().c_str()); t->setLooks(r.text().c_str()); t->setSkipColorSpaceConversion(r.u() != 0); DONE; }
    if (name == "ExponentTransform") { auto t = OCIO::ExponentTransform::Create(); t->setNegativeStyle(static_cast<OCIO::NegativeStyle>(r.u())); double v[4]; r.ds(v); t->setValue(v); DONE; }
    if (name == "ExponentWithLinearTransform") { auto t = OCIO::ExponentWithLinearTransform::Create(); t->setNegativeStyle(static_cast<OCIO::NegativeStyle>(r.u())); double v[4]; r.ds(v); t->setGamma(v); r.ds(v); t->setOffset(v); DONE; }
    if (name == "ExposureContrastTransform") {
        auto t = OCIO::ExposureContrastTransform::Create(); t->setStyle(static_cast<OCIO::ExposureContrastStyle>(r.u()));
        t->setExposure(r.d()); t->setContrast(r.d()); t->setGamma(r.d()); t->setPivot(r.d()); t->setLogExposureStep(r.d()); t->setLogMidGray(r.d());
        if (r.u()) t->makeExposureDynamic(); if (r.u()) t->makeContrastDynamic(); if (r.u()) t->makeGammaDynamic(); DONE;
    }
    if (name == "FileTransform") { auto t = OCIO::FileTransform::Create(); t->setSrc(r.text().c_str()); t->setCCCId(r.text().c_str()); t->setCDLStyle(static_cast<OCIO::CDLStyle>(r.u())); t->setInterpolation(static_cast<OCIO::Interpolation>(r.u())); DONE; }
    if (name == "FixedFunctionTransform") { auto style = static_cast<OCIO::FixedFunctionStyle>(r.u()); auto n = r.u(); std::vector<double> v(n); for (auto & x : v) x = r.d(); auto t = OCIO::FixedFunctionTransform::Create(style, v.data(), v.size()); DONE; }
    if (name == "GradingPrimaryTransform") { auto style = static_cast<OCIO::GradingStyle>(r.u()); auto t = OCIO::GradingPrimaryTransform::Create(style); t->setValue(read_primary(r, style)); if (r.u()) t->makeDynamic(); else t->makeNonDynamic(); DONE; }
    if (name == "GradingToneTransform") { auto style = static_cast<OCIO::GradingStyle>(r.u()); auto t = OCIO::GradingToneTransform::Create(style); t->setValue(read_tone(r, style)); if (r.u()) t->makeDynamic(); else t->makeNonDynamic(); DONE; }
    if (name == "GradingRGBCurveTransform") { auto style = static_cast<OCIO::GradingStyle>(r.u()); auto t = OCIO::GradingRGBCurveTransform::Create(style); t->setValue(read_rgb_curves(r, style)); t->setBypassLinToLog(r.u() != 0); if (r.u()) t->makeDynamic(); else t->makeNonDynamic(); DONE; }
    if (name == "GradingHueCurveTransform") { auto style = static_cast<OCIO::GradingStyle>(r.u()); auto t = OCIO::GradingHueCurveTransform::Create(style); t->setValue(read_hue_curves(r, style)); t->setRGBToHSY(static_cast<OCIO::HSYTransformStyle>(r.u())); if (r.u()) t->makeDynamic(); else t->makeNonDynamic(); DONE; }
    if (name == "LogTransform") { auto t = OCIO::LogTransform::Create(); t->setBase(r.d()); DONE; }
    if (name == "LogAffineTransform") { auto t = OCIO::LogAffineTransform::Create(); read_log(r, t); DONE; }
    if (name == "LogCameraTransform") { double v[3]; r.ds(v); auto t = OCIO::LogCameraTransform::Create(v); read_log(r, t); if (r.u()) { r.ds(v); t->setLinearSlopeValue(v); } DONE; }
    if (name == "Lut1DTransform") {
        auto length = r.u(); auto half = r.u(); auto t = OCIO::Lut1DTransform::Create(length, half != 0);
        t->setOutputRawHalfs(r.u() != 0); t->setHueAdjust(static_cast<OCIO::Lut1DHueAdjust>(r.u())); t->setInterpolation(static_cast<OCIO::Interpolation>(r.u())); t->setFileOutputBitDepth(static_cast<OCIO::BitDepth>(r.u()));
        auto values = r.floats(); if (values.size() != 3ull * length) throw std::runtime_error("wrong LUT1D payload size");
        for (uint32_t i = 0; i < length; ++i) t->setValue(i, values[3*i], values[3*i+1], values[3*i+2]); DONE;
    }
    if (name == "Lut3DTransform") {
        auto size = r.u(); auto t = OCIO::Lut3DTransform::Create(size); t->setInterpolation(static_cast<OCIO::Interpolation>(r.u())); t->setFileOutputBitDepth(static_cast<OCIO::BitDepth>(r.u()));
        auto values = r.floats(); if (values.size() != 3ull * size * size * size) throw std::runtime_error("wrong LUT3D payload size");
        size_t p = 0; for (uint32_t a=0; a<size; ++a) for (uint32_t b=0; b<size; ++b) for (uint32_t c=0; c<size; ++c) { t->setValue(a,b,c,values[p],values[p+1],values[p+2]); p += 3; } DONE;
    }
    if (name == "MatrixTransform") { auto t = OCIO::MatrixTransform::Create(); double v[16]; r.ds(v); t->setMatrix(v); double o[4]; r.ds(o); t->setOffset(o); t->setFileInputBitDepth(static_cast<OCIO::BitDepth>(r.u())); t->setFileOutputBitDepth(static_cast<OCIO::BitDepth>(r.u())); DONE; }
    if (name == "RangeTransform") { auto t = OCIO::RangeTransform::Create(); t->setStyle(static_cast<OCIO::RangeStyle>(r.u())); t->setFileInputBitDepth(static_cast<OCIO::BitDepth>(r.u())); t->setFileOutputBitDepth(static_cast<OCIO::BitDepth>(r.u())); if (r.u()) t->setMinInValue(r.d()); if (r.u()) t->setMaxInValue(r.d()); if (r.u()) t->setMinOutValue(r.d()); if (r.u()) t->setMaxOutValue(r.d()); DONE; }
#undef DONE
    throw std::runtime_error("unknown native transport transform: " + name);
}
