// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ExposureContrastOpData` against the wheel, through processors of an
//! `ExposureContrastTransform` (`oracle processor_ops`):
//! - the cache ID: the processor holds one ExposureContrast op, so its cache ID is
//!   `CacheIDHash(" <ExposureContrastOp " + data cache ID + ">")` (Processor.cpp:331-348,
//!   Op.cpp:448-465, ExposureContrastOp.cpp:120-129 @ v2.5.2); the data's text (the style, the
//!   values that aren't dynamic, 7 significant digits) is compared through its hash;
//! - the op data the processor built, in either direction: `createGroupTransform()` copies it
//!   into a transform (`CreateExposureContrastTransform`, ExposureContrastOp.cpp:197-212),
//!   whose getters give the style, the direction, the values and the dynamic flags.
//!
//! The transform's constructor sets the style, the values, the dynamic flags and then the
//! direction (src/bindings/python/transforms/PyExposureContrastTransform.cpp:19-44), and the
//! processor builds its op from a copy of the transform's data, inverted for an inverse
//! processor (ExposureContrastOp.cpp:173-189, 214-222).

use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::open_color_types::{
    DynamicPropertyType, ExposureContrastStyle, TransformDirection,
    exposure_contrast_style_to_string,
};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::ExposureContrastOpData;
use ocio_testkit::processor_ops::{Dumped, ProcessorOpsRequest};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// Doubles the cache ID prints: zeros, values that need all 7 significant digits or round in
/// the 7th, the tiniest and largest doubles, and the non-finite ones.
const VALUES: [f64; 24] = [
    0.0,
    -0.0,
    1.0,
    0.1,
    -1.4,
    0.18,
    0.088,
    0.435,
    1234567.8,
    9999999.5,
    0.000123456789,
    123.456789,
    1.23456789e20,
    -2.5e-300,
    1e-7,
    0.99999995,
    5e-324,
    f64::MAX,
    f64::MIN_POSITIVE,
    f64::NAN,
    -f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    2.0,
];

const STYLES: [ExposureContrastStyle; 3] = [
    ExposureContrastStyle::Linear,
    ExposureContrastStyle::Video,
    ExposureContrastStyle::Logarithmic,
];

/// One processor: the transform's parameters and the processor's direction.
#[derive(Debug, Clone)]
struct EcCase {
    style: ExposureContrastStyle,
    /// Exposure, contrast, gamma, pivot, log exposure step and log mid-gray.
    values: [f64; 6],
    /// Whether exposure, contrast and gamma are dynamic.
    dynamic: [bool; 3],
    transform_dir: TransformDirection,
    processor_dir: TransformDirection,
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

fn style_name(style: ExposureContrastStyle) -> &'static str {
    match style {
        ExposureContrastStyle::Linear => "EXPOSURE_CONTRAST_LINEAR",
        ExposureContrastStyle::Video => "EXPOSURE_CONTRAST_VIDEO",
        ExposureContrastStyle::Logarithmic => "EXPOSURE_CONTRAST_LOGARITHMIC",
    }
}

impl EcCase {
    fn processor(&self) -> Value {
        let v = |i: usize| f64_spec(self.values[i]);
        json!({
            "transform": {"class": "ExposureContrastTransform", "args": {
                "style": {"enum": style_name(self.style)},
                "exposure": v(0),
                "contrast": v(1),
                "gamma": v(2),
                "pivot": v(3),
                "logExposureStep": v(4),
                "logMidGray": v(5),
                "dynamicExposure": self.dynamic[0],
                "dynamicContrast": self.dynamic[1],
                "dynamicGamma": self.dynamic[2],
                "direction": {"enum": dir_name(self.transform_dir)},
            }},
            "direction": dir_name(self.processor_dir),
        })
    }

    /// The op data the processor holds, built as the wheel builds it.
    fn port(&self) -> ExposureContrastOpData {
        // The transform's data: ExposureContrastTransformImpl::setStyle keeps the direction
        // (forward for a new transform), then setDirection switches the style.
        let mut data =
            ExposureContrastOpData::new(ExposureContrastOpData::convert_transform_style(
                self.style,
                TransformDirection::Forward,
            ));
        data.set_exposure(self.values[0]);
        data.set_contrast(self.values[1]);
        data.set_gamma(self.values[2]);
        data.set_pivot(self.values[3]);
        data.set_log_exposure_step(self.values[4]);
        data.set_log_mid_gray(self.values[5]);
        if self.dynamic[0] {
            data.get_exposure_property().make_dynamic();
        }
        if self.dynamic[1] {
            data.get_contrast_property().make_dynamic();
        }
        if self.dynamic[2] {
            data.get_gamma_property().make_dynamic();
        }
        data.set_direction(self.transform_dir);
        // BuildExposureContrastOp: a clone, inverted for an inverse processor.
        let built = data.clone();
        match self.processor_dir {
            TransformDirection::Forward => built,
            TransformDirection::Inverse => built.inverse(),
        }
    }
}

fn cases() -> Vec<EcCase> {
    let dirs = [TransformDirection::Forward, TransformDirection::Inverse];
    let mut cases = Vec::new();
    let mut k = 0usize;
    for style in STYLES {
        for flags in 0..8u8 {
            for transform_dir in dirs {
                for processor_dir in dirs {
                    // Each value slot walks the list at its own pace, so the slots see every
                    // value in several combinations.
                    let values = std::array::from_fn(|slot| {
                        VALUES[(k * (slot + 1) + slot * 5) % VALUES.len()]
                    });
                    cases.push(EcCase {
                        style,
                        values,
                        dynamic: [flags & 1 != 0, flags & 2 != 0, flags & 4 != 0],
                        transform_dir,
                        processor_dir,
                    });
                    k += 1;
                }
            }
        }
    }
    // Every value in every slot, nothing dynamic, forward: the defaults elsewhere.
    for slot in 0..6 {
        for &value in &VALUES {
            let mut values = [0.0, 1.0, 1.0, 0.18, 0.088, 0.435];
            values[slot] = value;
            cases.push(EcCase {
                style: ExposureContrastStyle::Linear,
                values,
                dynamic: [false; 3],
                transform_dir: TransformDirection::Forward,
                processor_dir: TransformDirection::Forward,
            });
        }
    }
    cases
}

fn getter_f64(dumped: &Dumped) -> f64 {
    match dumped {
        Dumped::F64(v) => *v,
        other => panic!("not a float: {other:?}"),
    }
}

fn getter_bool(dumped: &Dumped) -> bool {
    match dumped {
        Dumped::Bool(v) => *v,
        other => panic!("not a bool: {other:?}"),
    }
}

/// The processor's cache ID and the op data it built are the wheel's.
#[test]
fn cache_ids_and_built_data_are_the_wheels() {
    let cases = cases();
    let requests: Vec<ProcessorOpsRequest> = cases
        .iter()
        .map(|case| {
            let mut request = ProcessorOpsRequest::new(case.processor());
            request.optimization = Some(json!("OPTIMIZATION_NONE"));
            request
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), cases.len());
    for (case, response) in cases.iter().zip(responses) {
        let reply = ocio_testkit::processor_ops::ProcessorOpsReply::from_response(
            response.expect("the oracle"),
        );
        assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
        let wheel = reply.processor();
        let data = case.port();

        // The cache ID.
        let mut full = b" <ExposureContrastOp ".to_vec();
        full.extend_from_slice(&data.get_cache_id());
        full.push(b'>');
        assert_eq!(
            cache_id_hash(&full),
            wheel.cache_id,
            "{case:?}: {}",
            String::from_utf8_lossy(&full)
        );

        // The data, through the group transform's getters.
        assert_eq!(wheel.group.children.len(), 1, "{case:?}");
        let t = &wheel.group.children[0];
        assert_eq!(t.class, "ExposureContrastTransform", "{case:?}");
        let style = ExposureContrastOpData::convert_style(data.get_style());
        assert_eq!(
            t.getter("getStyle"),
            &Dumped::Enum(style_name(style).to_string()),
            "{case:?} ({})",
            exposure_contrast_style_to_string(style)
        );
        assert_eq!(
            t.getter("getDirection"),
            &Dumped::Enum(dir_name(data.get_direction()).to_string()),
            "{case:?}"
        );
        let values = [
            ("getExposure", data.get_exposure()),
            ("getContrast", data.get_contrast()),
            ("getGamma", data.get_gamma()),
            ("getPivot", data.get_pivot()),
            ("getLogExposureStep", data.get_log_exposure_step()),
            ("getLogMidGray", data.get_log_mid_gray()),
        ];
        for (getter, port) in values {
            assert_eq!(
                getter_f64(t.getter(getter)).to_bits(),
                port.to_bits(),
                "{case:?}: {getter}"
            );
        }
        let flags = [
            ("isExposureDynamic", DynamicPropertyType::Exposure),
            ("isContrastDynamic", DynamicPropertyType::Contrast),
            ("isGammaDynamic", DynamicPropertyType::Gamma),
        ];
        for (getter, type_) in flags {
            assert_eq!(
                getter_bool(t.getter(getter)),
                data.has_dynamic_property(type_),
                "{case:?}: {getter}"
            );
        }
        assert_eq!(wheel.is_dynamic, data.is_dynamic(), "{case:?}");
    }
}
