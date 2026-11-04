// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Every type the CPU half of `Processor`'s API hands out or takes can be named through this
//! crate alone, without a dependency on `opencolorio-rs-ops`: OCIO declares them all in one
//! namespace (include/OpenColorIO/OpenColorIO.h @ v2.5.2).

use std::sync::{Arc, RwLockReadGuard};

#[allow(clippy::too_many_arguments, dead_code)]
fn names(
    _: &Arc<ocio::CpuProcessor>,
    _: &dyn ocio::ImageDesc,
    _: &mut dyn ocio::ImageDescMut,
    _: &ocio::PackedImageDesc<&[u8]>,
    _: &ocio::PlanarImageDesc<&mut [u8]>,
    _: ocio::ChannelOrdering,
    _: ocio::ImageLayout,
    _: ocio::ChannelPos,
    _: ocio::At<ocio::Bytes<&[u8]>>,
    _: ocio::PixelParts<&[u8]>,
    _: &dyn ocio::PixelData<Bytes = &[u8]>,
) {
}

#[allow(clippy::too_many_arguments, dead_code)]
fn dynamic_names(
    _: &ocio::DynamicPropertyRcPtr,
    _: &ocio::DynamicPropertyImpl,
    _: &ocio::DynamicPropertyDoubleImplRcPtr,
    _: &ocio::DynamicPropertyGradingPrimaryImplRcPtr,
    _: &ocio::DynamicPropertyGradingRgbCurveImplRcPtr,
    _: &ocio::DynamicPropertyGradingToneImplRcPtr,
    _: &ocio::DynamicPropertyGradingHueCurveImplRcPtr,
    _: (
        &ocio::DynamicPropertyDoubleImpl,
        &ocio::DynamicPropertyGradingPrimaryImpl,
        &ocio::DynamicPropertyGradingRgbCurveImpl,
        &ocio::DynamicPropertyGradingToneImpl,
        &ocio::DynamicPropertyGradingHueCurveImpl,
    ),
    _: (
        RwLockReadGuard<'_, ocio::PrimaryState>,
        RwLockReadGuard<'_, ocio::RgbCurveState>,
        RwLockReadGuard<'_, ocio::ToneState>,
        RwLockReadGuard<'_, ocio::HueCurveState>,
    ),
) {
}

#[test]
fn an_image_and_a_dynamic_property_go_through_this_crates_names() -> ocio::Result<()> {
    let config = ocio::Config::create_raw()?;
    let mut transform = ocio::ExposureContrastTransform::new();
    transform.make_exposure_dynamic();
    let processor = config.processor(&transform.into())?;

    let cpu: Arc<ocio::CpuProcessor> = processor.default_cpu_processor()?;
    let property: ocio::DynamicPropertyRcPtr =
        cpu.get_dynamic_property(ocio::DynamicPropertyType::Exposure)?;
    let exposure: ocio::DynamicPropertyDoubleImplRcPtr = property.as_double()?;
    exposure.set_value(0.5);

    let mut pixels = vec![0.18_f32; 4 * 2 * 2];
    let mut image: ocio::PackedImageDesc<&mut [u8]> =
        ocio::PackedImageDesc::with_channel_order(&mut pixels, 2, 2, ocio::ChannelOrdering::Rgba)?;
    cpu.apply(&mut image)
}
