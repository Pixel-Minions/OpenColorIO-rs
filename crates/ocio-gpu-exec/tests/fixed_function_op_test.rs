// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/FixedFunctionOp_test.cpp` @ v2.5.2, 36 other cases (7.3e).
//! Original inputs and settings; actual wheel CPU/shader references; no retests.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::FixedFunctionStyle;
use ocio_gpu_exec::gpu_unit_test::ReferenceRun;

#[path = "support/fixed_function_cases.rs"]
mod cases;
#[path = "support/suite1_reference.rs"]
mod reference;

use cases::{Build, Case, Input};

const CASES: &[Case] = &[
    // FixedFunctionOp_test.cpp:13-32 @ v2.5.2.
    Case {
        name: "style_aces_redmod03_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesRedMod03,
            oracle_style: "FIXED_FUNCTION_ACES_RED_MOD_03",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            0.90, 0.05, 0.22, 0.50, 0.97, 0.097, 0.0097, 1.0, 0.89, 0.15, 0.56, 0.0, -1.0, -0.001,
            1.2, 0.0,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 16,
        width: 4,
        height: 1,
        comparison_entries: 16,
        raw_input_digest: "d82390ca6c242da1c3c65f8bb47bb8924c56b94c8bd1ce1872c5648caf2b0ce6",
        padded_input_digest: "0bb2c056f99ab412d09ad08396fbfc59119d124bd8dfb2816e3cab02b45fd369",
    },
    // FixedFunctionOp_test.cpp:34-53 @ v2.5.2.
    Case {
        name: "style_aces_redmod03_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesRedMod03,
            oracle_style: "FIXED_FUNCTION_ACES_RED_MOD_03",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.90, 0.05, 0.22, 0.50, 0.97, 0.097, 0.0097, 1.0, 0.89, 0.15, 0.56, 0.0, -1.0, -0.001,
            1.2, 0.0,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 16,
        width: 4,
        height: 1,
        comparison_entries: 16,
        raw_input_digest: "d82390ca6c242da1c3c65f8bb47bb8924c56b94c8bd1ce1872c5648caf2b0ce6",
        padded_input_digest: "0bb2c056f99ab412d09ad08396fbfc59119d124bd8dfb2816e3cab02b45fd369",
    },
    // FixedFunctionOp_test.cpp:55-74 @ v2.5.2.
    Case {
        name: "style_aces_redmod10_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesRedMod10,
            oracle_style: "FIXED_FUNCTION_ACES_RED_MOD_10",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            0.90, 0.05, 0.22, 0.50, 0.97, 0.097, 0.0097, 1.0, 0.89, 0.15, 0.56, 0.0, -1.0, -0.001,
            1.2, 0.0,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 16,
        width: 4,
        height: 1,
        comparison_entries: 16,
        raw_input_digest: "d82390ca6c242da1c3c65f8bb47bb8924c56b94c8bd1ce1872c5648caf2b0ce6",
        padded_input_digest: "0bb2c056f99ab412d09ad08396fbfc59119d124bd8dfb2816e3cab02b45fd369",
    },
    // FixedFunctionOp_test.cpp:76-95 @ v2.5.2.
    Case {
        name: "style_aces_redmod10_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesRedMod10,
            oracle_style: "FIXED_FUNCTION_ACES_RED_MOD_10",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.90, 0.05, 0.22, 0.50, 0.97, 0.097, 0.0097, 1.0, 0.89, 0.15, 0.56, 0.0, -1.0, -0.001,
            1.2, 0.0,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 16,
        width: 4,
        height: 1,
        comparison_entries: 16,
        raw_input_digest: "d82390ca6c242da1c3c65f8bb47bb8924c56b94c8bd1ce1872c5648caf2b0ce6",
        padded_input_digest: "0bb2c056f99ab412d09ad08396fbfc59119d124bd8dfb2816e3cab02b45fd369",
    },
    // FixedFunctionOp_test.cpp:97-115 @ v2.5.2.
    Case {
        name: "style_aces_glow03_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGlow03,
            oracle_style: "FIXED_FUNCTION_ACES_GLOW_03",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            0.11, 0.02, 0., 0.5, 0.01, 0.02, 0.03, 1.0, 0.11, 0.91, 0.01, 0.,
        ]),
        threshold: 1e-7,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 12,
        width: 3,
        height: 1,
        comparison_entries: 12,
        raw_input_digest: "d95c68a64da386dd00d0bdfc4297f9f8de42c176d597e67561df48eab2c91e65",
        padded_input_digest: "35ae62078b3c4a482dd1e282fc9374897f0efb2dbdfaf5e1d8221a892819a79a",
    },
    // FixedFunctionOp_test.cpp:117-135 @ v2.5.2.
    Case {
        name: "style_aces_glow03_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGlow03,
            oracle_style: "FIXED_FUNCTION_ACES_GLOW_03",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.11, 0.02, 0., 0.5, 0.01, 0.02, 0.03, 1.0, 0.11, 0.91, 0.01, 0.,
        ]),
        threshold: 1e-7,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 12,
        width: 3,
        height: 1,
        comparison_entries: 12,
        raw_input_digest: "d95c68a64da386dd00d0bdfc4297f9f8de42c176d597e67561df48eab2c91e65",
        padded_input_digest: "35ae62078b3c4a482dd1e282fc9374897f0efb2dbdfaf5e1d8221a892819a79a",
    },
    // FixedFunctionOp_test.cpp:137-155 @ v2.5.2.
    Case {
        name: "style_aces_glow10_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGlow10,
            oracle_style: "FIXED_FUNCTION_ACES_GLOW_10",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            0.11, 0.02, 0., 0.5, 0.01, 0.02, 0.03, 1.0, 0.11, 0.91, 0.01, 0.,
        ]),
        threshold: 1e-7,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 12,
        width: 3,
        height: 1,
        comparison_entries: 12,
        raw_input_digest: "d95c68a64da386dd00d0bdfc4297f9f8de42c176d597e67561df48eab2c91e65",
        padded_input_digest: "35ae62078b3c4a482dd1e282fc9374897f0efb2dbdfaf5e1d8221a892819a79a",
    },
    // FixedFunctionOp_test.cpp:157-175 @ v2.5.2.
    Case {
        name: "style_aces_glow10_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGlow10,
            oracle_style: "FIXED_FUNCTION_ACES_GLOW_10",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.11, 0.02, 0., 0.5, 0.01, 0.02, 0.03, 1.0, 0.11, 0.91, 0.01, 0.,
        ]),
        threshold: 1e-7,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 12,
        width: 3,
        height: 1,
        comparison_entries: 12,
        raw_input_digest: "d95c68a64da386dd00d0bdfc4297f9f8de42c176d597e67561df48eab2c91e65",
        padded_input_digest: "35ae62078b3c4a482dd1e282fc9374897f0efb2dbdfaf5e1d8221a892819a79a",
    },
    // FixedFunctionOp_test.cpp:177-241 @ v2.5.2.
    Case {
        name: "style_aces_gamutcomp13_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGamutComp13,
            oracle_style: "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
            params: || vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
            inverse: false,
        },
        input: Input::Literal(&[
            0.96663409472,
            0.04819045216,
            0.00719300006,
            0.0,
            0.11554181576,
            1.18493819237,
            -0.06659350544,
            0.0,
            -0.08217582852,
            -0.23312863708,
            1.05940067768,
            0.0,
            0.92980366945,
            0.03025679290,
            -0.02240031771,
            0.0,
            0.12437260151,
            1.19238424301,
            -0.08014731854,
            0.0,
            -0.05417707562,
            -0.22264070809,
            1.10254764557,
            0.0,
            1.10869872570,
            -0.05317572504,
            -0.00306261564,
            0.0,
            0.00142395718,
            1.31239914894,
            -0.22332298756,
            0.0,
            -0.11012268066,
            -0.25922337174,
            1.22638559341,
            0.0,
            1.14983725548,
            -0.02548932098,
            -0.06720325351,
            0.0,
            -0.06796986610,
            1.30455482006,
            -0.31973674893,
            0.0,
            -0.08186896890,
            -0.27906489372,
            1.38694024086,
            0.0,
            1.08979821205,
            -0.03117186762,
            -0.00326358480,
            0.0,
            -0.03276504576,
            1.18293666840,
            -0.00156985107,
            0.0,
            -0.05703317001,
            -0.15176482499,
            1.00483345985,
            0.0,
            1.15183949471,
            -0.04052511975,
            -0.01231821068,
            0.0,
            -0.11769985408,
            1.20661473274,
            0.00725125661,
            0.0,
            -0.03413961083,
            -0.16608965397,
            1.00506699085,
            0.0,
            1.04839742184,
            -0.02998665348,
            -0.00313943392,
            0.0,
            0.01196120959,
            1.14840388298,
            -0.00963746291,
            0.0,
            -0.06036021933,
            -0.11841656268,
            1.01277709007,
            0.0,
            0.13911968470,
            0.08746965975,
            0.05927771702,
            0.0,
            0.45410454273,
            0.32112336159,
            0.23821924627,
            0.0,
            0.15262818336,
            0.19457373023,
            0.31270095706,
            0.0,
            0.11231111735,
            0.14410330355,
            0.06487321854,
            0.0,
            0.24113640189,
            0.22817260027,
            0.40912008286,
            0.0,
            0.27200737596,
            0.47832396626,
            0.40502992272,
            0.0,
            0.49412208796,
            0.23219805956,
            0.05947655812,
            0.0,
            0.09734666348,
            0.10917002708,
            0.33662334085,
            0.0,
            0.37841814756,
            0.12591768801,
            0.12897071242,
            0.0,
            0.09104857594,
            0.05404697359,
            0.13533248007,
            0.0,
            0.38014721870,
            0.47619381547,
            0.10615456849,
            0.0,
            0.60210841894,
            0.38621774316,
            0.08225912601,
            0.0,
            0.05051656812,
            0.05367648974,
            0.27239432931,
            0.0,
            0.14276765287,
            0.28139206767,
            0.09023084491,
            0.0,
            0.28782477975,
            0.06140174344,
            0.05256444961,
            0.0,
            0.70791155100,
            0.58026152849,
            0.09300658852,
            0.0,
            0.35456034541,
            0.12329842150,
            0.27530980110,
            0.0,
            0.08374430984,
            0.22774916887,
            0.35839819908,
            0.0,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 156,
        width: 39,
        height: 1,
        comparison_entries: 156,
        raw_input_digest: "3711207f649539f0911cec62dae05a59d2b7b14ce9209688564df22bad030434",
        padded_input_digest: "a1e10b53e849db3bf7c233cb4e5da58e826f66fd7f1189bf4c66e5540e2a21e6",
    },
    // FixedFunctionOp_test.cpp:243-307 @ v2.5.2.
    Case {
        name: "style_aces_gamutcomp13_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesGamutComp13,
            oracle_style: "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
            params: || vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2],
            inverse: true,
        },
        input: Input::Literal(&[
            0.96663409472,
            0.04819045216,
            0.00719300006,
            0.0,
            0.11554181576,
            1.18493819237,
            -0.06659350544,
            0.0,
            -0.08217582852,
            -0.23312863708,
            1.05940067768,
            0.0,
            0.92980366945,
            0.03025679290,
            -0.02240031771,
            0.0,
            0.12437260151,
            1.19238424301,
            -0.08014731854,
            0.0,
            -0.05417707562,
            -0.22264070809,
            1.10254764557,
            0.0,
            1.10869872570,
            -0.05317572504,
            -0.00306261564,
            0.0,
            0.00142395718,
            1.31239914894,
            -0.22332298756,
            0.0,
            -0.11012268066,
            -0.25922337174,
            1.22638559341,
            0.0,
            1.14983725548,
            -0.02548932098,
            -0.06720325351,
            0.0,
            -0.06796986610,
            1.30455482006,
            -0.31973674893,
            0.0,
            -0.08186896890,
            -0.27906489372,
            1.38694024086,
            0.0,
            1.08979821205,
            -0.03117186762,
            -0.00326358480,
            0.0,
            -0.03276504576,
            1.18293666840,
            -0.00156985107,
            0.0,
            -0.05703317001,
            -0.15176482499,
            1.00483345985,
            0.0,
            1.15183949471,
            -0.04052511975,
            -0.01231821068,
            0.0,
            -0.11769985408,
            1.20661473274,
            0.00725125661,
            0.0,
            -0.03413961083,
            -0.16608965397,
            1.00506699085,
            0.0,
            1.04839742184,
            -0.02998665348,
            -0.00313943392,
            0.0,
            0.01196120959,
            1.14840388298,
            -0.00963746291,
            0.0,
            -0.06036021933,
            -0.11841656268,
            1.01277709007,
            0.0,
            0.13911968470,
            0.08746965975,
            0.05927771702,
            0.0,
            0.45410454273,
            0.32112336159,
            0.23821924627,
            0.0,
            0.15262818336,
            0.19457373023,
            0.31270095706,
            0.0,
            0.11231111735,
            0.14410330355,
            0.06487321854,
            0.0,
            0.24113640189,
            0.22817260027,
            0.40912008286,
            0.0,
            0.27200737596,
            0.47832396626,
            0.40502992272,
            0.0,
            0.49412208796,
            0.23219805956,
            0.05947655812,
            0.0,
            0.09734666348,
            0.10917002708,
            0.33662334085,
            0.0,
            0.37841814756,
            0.12591768801,
            0.12897071242,
            0.0,
            0.09104857594,
            0.05404697359,
            0.13533248007,
            0.0,
            0.38014721870,
            0.47619381547,
            0.10615456849,
            0.0,
            0.60210841894,
            0.38621774316,
            0.08225912601,
            0.0,
            0.05051656812,
            0.05367648974,
            0.27239432931,
            0.0,
            0.14276765287,
            0.28139206767,
            0.09023084491,
            0.0,
            0.28782477975,
            0.06140174344,
            0.05256444961,
            0.0,
            0.70791155100,
            0.58026152849,
            0.09300658852,
            0.0,
            0.35456034541,
            0.12329842150,
            0.27530980110,
            0.0,
            0.08374430984,
            0.22774916887,
            0.35839819908,
            0.0,
        ]),
        threshold: 3e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 156,
        width: 39,
        height: 1,
        comparison_entries: 156,
        raw_input_digest: "3711207f649539f0911cec62dae05a59d2b7b14ce9209688564df22bad030434",
        padded_input_digest: "a1e10b53e849db3bf7c233cb4e5da58e826f66fd7f1189bf4c66e5540e2a21e6",
    },
    // FixedFunctionOp_test.cpp:1181-1192 @ v2.5.2.
    Case {
        name: "style_aces_darktodim10_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesDarkToDim10,
            oracle_style: "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1194-1205 @ v2.5.2.
    Case {
        name: "style_aces_darktodim10_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::AcesDarkToDim10,
            oracle_style: "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1207-1219 @ v2.5.2.
    Case {
        name: "style_rec2100_surround_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::Rec2100Surround,
            oracle_style: "FIXED_FUNCTION_REC2100_SURROUND",
            params: || vec![0.7],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 2e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1221-1234 @ v2.5.2.
    Case {
        name: "style_rec2100_surround_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::Rec2100Surround,
            oracle_style: "FIXED_FUNCTION_REC2100_SURROUND",
            params: || vec![1. / 0.7],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 2e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1236-1250 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSV_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsv,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSV",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: false,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
        padded_input_digest: "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489",
    },
    // FixedFunctionOp_test.cpp:1252-1271 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSV_fwd_custom",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsv,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSV",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            1.500,
            2.500,
            0.500,
            0.50,
            3.125,
            -0.625,
            1.250,
            1.00,
            -5. / 3.,
            -4. / 3.,
            -1. / 3.,
            0.25,
            0.100,
            -0.800,
            0.400,
            0.25,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 16,
        width: 4,
        height: 1,
        comparison_entries: 16,
        raw_input_digest: "21ba9ba22bda7a1a101539580a93a14482e5aa5c5f1b1ac09b39380609b2c3d7",
        padded_input_digest: "442b429cd16f6820efe7fc46c628fd5416864f51b6d54e5fae16331bbeabb71e",
    },
    // FixedFunctionOp_test.cpp:1273-1286 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSV_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsv,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSV",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 5e-4,
        relative: true,
        minimum: 5e-3,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        padded_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // FixedFunctionOp_test.cpp:1288-1311 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSV_inv_custom",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsv,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSV",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            3. / 12.,
            0.80,
            2.50,
            0.50,
            11. / 12.,
            1.20,
            2.50,
            1.00,
            15. / 24.,
            0.80,
            -2.00,
            0.25,
            19. / 24.,
            1.50,
            -0.40,
            0.25,
            -89. / 24.,
            0.50,
            0.40,
            2.00,
            81. / 24.,
            1.50,
            -0.40,
            -0.25,
            81. / 24.,
            -0.50,
            0.40,
            0.00,
            0.5000,
            2.50,
            0.04,
            0.00,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "4a0052f8e94b8b1eda8d4b52f4eb507dd71072b6cb9ff2fa0298f5a9c54d6991",
        padded_input_digest: "80c880d0919fce0da0dc43252de9ba36200d12bc7c129960a62c94e42f26d367",
    },
    // FixedFunctionOp_test.cpp:1313-1334 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_LIN_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyLin,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_LIN",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            -0.075290, 0.078996, -0.108397, 0., 0.3, 0.4, 0.5, 0., 0.05, 0.03, 0.04, 0., 0.01,
            0.01, -0.05, 1., 0.05, -0.005, -0.05, 1., -0.048, 0.01, 0.05, 0., 0.3, -0.4, 0.5, 0.,
            -0.055, 0.01, 0.05, 0.,
        ]),
        threshold: 5e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "d8e1581e87591670910d2a5e1e9507f746b4e5359cc24690f989e22dec60007b",
        padded_input_digest: "6da0edd2e144b7d8e76033681a4c6c7d86c37522dc19ba2069064b61c791393f",
    },
    // FixedFunctionOp_test.cpp:1336-1357 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_LIN_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyLin,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_LIN",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.470554752,
            9.12594033,
            0.0326650218,
            0.,
            0.75,
            0.22196741,
            0.38596,
            0.,
            0.08333333,
            0.12976444,
            0.034974,
            0.,
            0.333333333333,
            0.606036032,
            0.0056680,
            1.,
            0.241666666667,
            0.8372990325,
            0.0034440,
            1.,
            0.734693877551,
            0.752099600,
            0.0005572,
            0.,
            0.96296296,
            9.7034,
            -0.1862,
            0.,
            0.730158730159,
            0.811517000,
            -0.0009310,
            0.,
        ]),
        threshold: 5e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "6a862eaf7f59579bec01ff57e4da5f5e33b764e803ac1c76bbce57f21c484d85",
        padded_input_digest: "24b06465320519a1eafb40eaa8e5446149cfdd868f18174c7046b78a597e333a",
    },
    // FixedFunctionOp_test.cpp:1359-1380 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_LOG_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyLog,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_LOG",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            -0.075290, 0.078996, -0.108397, 0., 0.3, 0.4, 0.5, 0., 0.05, 0.03, 0.04, 0., 0.01,
            0.01, -0.05, 1., 0.05, -0.005, -0.05, 1., -0.048, 0.01, 0.05, 0., 0.3, -0.4, 0.5, 0.,
            -0.055, 0.01, 0.05, 0.,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "d8e1581e87591670910d2a5e1e9507f746b4e5359cc24690f989e22dec60007b",
        padded_input_digest: "6da0edd2e144b7d8e76033681a4c6c7d86c37522dc19ba2069064b61c791393f",
    },
    // FixedFunctionOp_test.cpp:1382-1403 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_LOG_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyLog,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_LOG",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.470554752,
            9.12594033,
            0.0326650218,
            0.,
            0.75,
            0.22196741,
            0.38596,
            0.,
            0.08333333,
            0.12976444,
            0.034974,
            0.,
            0.333333333333,
            0.606036032,
            0.0056680,
            1.,
            0.241666666667,
            0.8372990325,
            0.0034440,
            1.,
            0.734693877551,
            0.752099600,
            0.0005572,
            0.,
            0.96296296,
            9.7034,
            -0.1862,
            0.,
            0.730158730159,
            0.811517000,
            -0.0009310,
            0.,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "6a862eaf7f59579bec01ff57e4da5f5e33b764e803ac1c76bbce57f21c484d85",
        padded_input_digest: "24b06465320519a1eafb40eaa8e5446149cfdd868f18174c7046b78a597e333a",
    },
    // FixedFunctionOp_test.cpp:1405-1426 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_VID_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyVid,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_VID",
            params: || vec![],
            inverse: false,
        },
        input: Input::Literal(&[
            -0.075290, 0.078996, -0.108397, 0., 0.3, 0.4, 0.5, 0., 0.05, 0.03, 0.04, 0., 0.01,
            0.01, -0.05, 1., 0.05, -0.005, -0.05, 1., -0.048, 0.01, 0.05, 0., 0.3, -0.4, 0.5, 0.,
            -0.055, 0.01, 0.05, 0.,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "d8e1581e87591670910d2a5e1e9507f746b4e5359cc24690f989e22dec60007b",
        padded_input_digest: "6da0edd2e144b7d8e76033681a4c6c7d86c37522dc19ba2069064b61c791393f",
    },
    // FixedFunctionOp_test.cpp:1428-1449 @ v2.5.2.
    Case {
        name: "style_RGB_TO_HSY_VID_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::RgbToHsyVid,
            oracle_style: "FIXED_FUNCTION_RGB_TO_HSY_VID",
            params: || vec![],
            inverse: true,
        },
        input: Input::Literal(&[
            0.470554752,
            9.12594033,
            0.0326650218,
            0.,
            0.75,
            0.22196741,
            0.38596,
            0.,
            0.08333333,
            0.12976444,
            0.034974,
            0.,
            0.333333333333,
            0.606036032,
            0.0056680,
            1.,
            0.241666666667,
            0.8372990325,
            0.0034440,
            1.,
            0.734693877551,
            0.752099600,
            0.0005572,
            0.,
            0.96296296,
            9.7034,
            -0.1862,
            0.,
            0.730158730159,
            0.811517000,
            -0.0009310,
            0.,
        ]),
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 32,
        width: 8,
        height: 1,
        comparison_entries: 32,
        raw_input_digest: "6a862eaf7f59579bec01ff57e4da5f5e33b764e803ac1c76bbce57f21c484d85",
        padded_input_digest: "24b06465320519a1eafb40eaa8e5446149cfdd868f18174c7046b78a597e333a",
    },
    // FixedFunctionOp_test.cpp:1451-1460 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_xyY_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToXyy,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_xyY",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        padded_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // FixedFunctionOp_test.cpp:1462-1471 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_xyY_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToXyy,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_xyY",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        padded_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // FixedFunctionOp_test.cpp:1473-1482 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_uvY_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToUvy,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_uvY",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        padded_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // FixedFunctionOp_test.cpp:1484-1493 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_uvY_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToUvy,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_uvY",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-5,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: true,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
        padded_input_digest: "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af",
    },
    // FixedFunctionOp_test.cpp:1495-1505 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_LUV_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToLuv,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_LUV",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 5e-5,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1507-1517 @ v2.5.2.
    Case {
        name: "style_XYZ_TO_LUV_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::XyzToLuv,
            oracle_style: "FIXED_FUNCTION_XYZ_TO_LUV",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-5,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
        padded_input_digest: "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642",
    },
    // FixedFunctionOp_test.cpp:1519-1533 @ v2.5.2.
    Case {
        name: "style_LIN_TO_PQ_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToPq,
            oracle_style: "FIXED_FUNCTION_LIN_TO_PQ",
            params: || vec![],
            inverse: false,
        },
        input: Input::Ramp {
            low: -0.1,
            high: 100.1,
        },
        threshold: 0.0008,
        relative: false,
        minimum: 1e-6,
        nan: false,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "96b93a3c9246aabbce75d4dc376559e106b6f76da07670ec70b5c097a67b8c91",
        padded_input_digest: "96b93a3c9246aabbce75d4dc376559e106b6f76da07670ec70b5c097a67b8c91",
    },
    // FixedFunctionOp_test.cpp:1535-1554 @ v2.5.2.
    Case {
        name: "style_LIN_TO_PQ_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToPq,
            oracle_style: "FIXED_FUNCTION_LIN_TO_PQ",
            params: || vec![],
            inverse: true,
        },
        input: Input::Ramp {
            low: -0.1,
            high: 1.1,
        },
        threshold: 0.0023,
        relative: true,
        minimum: 1e-6,
        nan: false,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "e21aa65bb1c125b264f765e24f7cf3984cc9d4d8e5de81bf5d270f3c771e7230",
        padded_input_digest: "e21aa65bb1c125b264f765e24f7cf3984cc9d4d8e5de81bf5d270f3c771e7230",
    },
    // FixedFunctionOp_test.cpp:1581-1591 @ v2.5.2.
    Case {
        name: "style_LIN_TO_GAMMA_LOG_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToGammaLog,
            oracle_style: "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
            params: || {
                vec![
                    0.0,
                    0.25,
                    0.5,
                    1.0,
                    0.0,
                    1.0_f64.exp(),
                    0.17883277,
                    0.807825590164,
                    1.0,
                    -0.07116723,
                ]
            },
            inverse: false,
        },
        input: Input::Ramp {
            low: -0.1,
            high: 3.35,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: false,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "4a8ae4fde86bdbac7de5250320b1ccdfd67da01c23b0930fe1613edf2b41e279",
        padded_input_digest: "4a8ae4fde86bdbac7de5250320b1ccdfd67da01c23b0930fe1613edf2b41e279",
    },
    // FixedFunctionOp_test.cpp:1593-1602 @ v2.5.2.
    Case {
        name: "style_LIN_TO_GAMMA_LOG_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToGammaLog,
            oracle_style: "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
            params: || {
                vec![
                    0.0,
                    0.25,
                    0.5,
                    1.0,
                    0.0,
                    1.0_f64.exp(),
                    0.17883277,
                    0.807825590164,
                    1.0,
                    -0.07116723,
                ]
            },
            inverse: true,
        },
        input: Input::Ramp {
            low: -0.3,
            high: 1.02,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "367a169e58d25fa78e1e3e88946dcf6645f212f1e3663cc88d4080861bcee5ad",
        padded_input_digest: "367a169e58d25fa78e1e3e88946dcf6645f212f1e3663cc88d4080861bcee5ad",
    },
    // FixedFunctionOp_test.cpp:1604-1624 @ v2.5.2.
    Case {
        name: "style_LIN_TO_DOUBLE_LOG_fwd",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToDoubleLog,
            oracle_style: "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
            params: || {
                vec![
                    10.0, 0.1, 0.5, -1.0, -1.0, -1.0, 0.2, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
                ]
            },
            inverse: false,
        },
        input: Input::Ramp {
            low: -1.0,
            high: 2.0,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: false,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
        padded_input_digest: "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94",
    },
    // FixedFunctionOp_test.cpp:1626-1645 @ v2.5.2.
    Case {
        name: "style_LIN_TO_DOUBLE_LOG_inv",
        build: Build::Fixed {
            style: FixedFunctionStyle::LinToDoubleLog,
            oracle_style: "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
            params: || {
                vec![
                    10.0, 0.1, 0.5, -1.0, -1.0, -1.0, 0.2, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
                ]
            },
            inverse: true,
        },
        input: Input::Ramp {
            low: -1.1,
            high: 1.4,
        },
        threshold: 1e-6,
        relative: false,
        minimum: 1e-6,
        nan: true,
        infinity: false,
        allow_1d: true,
        original_entries: 262144,
        width: 256,
        height: 256,
        comparison_entries: 262144,
        raw_input_digest: "857e04fa67ea0903af5a0259b99538509e355294e0cb4b750af513db3e70bab8",
        padded_input_digest: "857e04fa67ea0903af5a0259b99538509e355294e0cb4b750af513db3e70bab8",
    },
];

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        assert_eq!(CASES.len(), 36);
        let tests: Vec<_> = CASES.iter().map(cases::input_test).collect();
        assert!(
            tests
                .iter()
                .all(|test| !test.legacy_shader && test.num_retests() == 0)
        );
        let processors: Vec<_> = CASES.iter().map(cases::processor_spec).collect();
        reference::batch(&tests, &processors)
    })
}

fn run_case(index: usize, name: &str) {
    let case = &CASES[index];
    assert_eq!(case.name, name);
    let mut test = cases::input_test(case);
    test.set_processor_transform(&cases::transform(case))
        .unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_redmod03_fwd)` @ v2.5.2.
#[test]
fn style_aces_redmod03_fwd() {
    run_case(0, "style_aces_redmod03_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_redmod03_inv)` @ v2.5.2.
#[test]
fn style_aces_redmod03_inv() {
    run_case(1, "style_aces_redmod03_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_redmod10_fwd)` @ v2.5.2.
#[test]
fn style_aces_redmod10_fwd() {
    run_case(2, "style_aces_redmod10_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_redmod10_inv)` @ v2.5.2.
#[test]
fn style_aces_redmod10_inv() {
    run_case(3, "style_aces_redmod10_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_glow03_fwd)` @ v2.5.2.
#[test]
fn style_aces_glow03_fwd() {
    run_case(4, "style_aces_glow03_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_glow03_inv)` @ v2.5.2.
#[test]
fn style_aces_glow03_inv() {
    run_case(5, "style_aces_glow03_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_glow10_fwd)` @ v2.5.2.
#[test]
fn style_aces_glow10_fwd() {
    run_case(6, "style_aces_glow10_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_glow10_inv)` @ v2.5.2.
#[test]
fn style_aces_glow10_inv() {
    run_case(7, "style_aces_glow10_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_gamutcomp13_fwd)` @ v2.5.2.
#[test]
fn style_aces_gamutcomp13_fwd() {
    run_case(8, "style_aces_gamutcomp13_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_gamutcomp13_inv)` @ v2.5.2.
#[test]
fn style_aces_gamutcomp13_inv() {
    run_case(9, "style_aces_gamutcomp13_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_darktodim10_fwd)` @ v2.5.2.
#[test]
fn style_aces_darktodim10_fwd() {
    run_case(10, "style_aces_darktodim10_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_aces_darktodim10_inv)` @ v2.5.2.
#[test]
fn style_aces_darktodim10_inv() {
    run_case(11, "style_aces_darktodim10_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_rec2100_surround_fwd)` @ v2.5.2.
#[test]
fn style_rec2100_surround_fwd() {
    run_case(12, "style_rec2100_surround_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_rec2100_surround_inv)` @ v2.5.2.
#[test]
fn style_rec2100_surround_inv() {
    run_case(13, "style_rec2100_surround_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSV_fwd)` @ v2.5.2.
#[test]
fn style_rgb_to_hsv_fwd() {
    run_case(14, "style_RGB_TO_HSV_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSV_fwd_custom)` @ v2.5.2.
#[test]
fn style_rgb_to_hsv_fwd_custom() {
    run_case(15, "style_RGB_TO_HSV_fwd_custom");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSV_inv)` @ v2.5.2.
#[test]
fn style_rgb_to_hsv_inv() {
    run_case(16, "style_RGB_TO_HSV_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSV_inv_custom)` @ v2.5.2.
#[test]
fn style_rgb_to_hsv_inv_custom() {
    run_case(17, "style_RGB_TO_HSV_inv_custom");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_LIN_fwd)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_lin_fwd() {
    run_case(18, "style_RGB_TO_HSY_LIN_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_LIN_inv)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_lin_inv() {
    run_case(19, "style_RGB_TO_HSY_LIN_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_LOG_fwd)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_log_fwd() {
    run_case(20, "style_RGB_TO_HSY_LOG_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_LOG_inv)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_log_inv() {
    run_case(21, "style_RGB_TO_HSY_LOG_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_VID_fwd)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_vid_fwd() {
    run_case(22, "style_RGB_TO_HSY_VID_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_RGB_TO_HSY_VID_inv)` @ v2.5.2.
#[test]
fn style_rgb_to_hsy_vid_inv() {
    run_case(23, "style_RGB_TO_HSY_VID_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_xyY_fwd)` @ v2.5.2.
#[test]
fn style_xyz_to_xyy_fwd() {
    run_case(24, "style_XYZ_TO_xyY_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_xyY_inv)` @ v2.5.2.
#[test]
fn style_xyz_to_xyy_inv() {
    run_case(25, "style_XYZ_TO_xyY_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_uvY_fwd)` @ v2.5.2.
#[test]
fn style_xyz_to_uvy_fwd() {
    run_case(26, "style_XYZ_TO_uvY_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_uvY_inv)` @ v2.5.2.
#[test]
fn style_xyz_to_uvy_inv() {
    run_case(27, "style_XYZ_TO_uvY_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_LUV_fwd)` @ v2.5.2.
#[test]
fn style_xyz_to_luv_fwd() {
    run_case(28, "style_XYZ_TO_LUV_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_XYZ_TO_LUV_inv)` @ v2.5.2.
#[test]
fn style_xyz_to_luv_inv() {
    run_case(29, "style_XYZ_TO_LUV_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_PQ_fwd)` @ v2.5.2.
#[test]
fn style_lin_to_pq_fwd() {
    run_case(30, "style_LIN_TO_PQ_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_PQ_inv)` @ v2.5.2.
#[test]
fn style_lin_to_pq_inv() {
    run_case(31, "style_LIN_TO_PQ_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_GAMMA_LOG_fwd)` @ v2.5.2.
#[test]
fn style_lin_to_gamma_log_fwd() {
    run_case(32, "style_LIN_TO_GAMMA_LOG_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_GAMMA_LOG_inv)` @ v2.5.2.
#[test]
fn style_lin_to_gamma_log_inv() {
    run_case(33, "style_LIN_TO_GAMMA_LOG_inv");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_DOUBLE_LOG_fwd)` @ v2.5.2.
#[test]
fn style_lin_to_double_log_fwd() {
    run_case(34, "style_LIN_TO_DOUBLE_LOG_fwd");
}
/// Port of `OCIO_ADD_GPU_TEST(FixedFunction, style_LIN_TO_DOUBLE_LOG_inv)` @ v2.5.2.
#[test]
fn style_lin_to_double_log_inv() {
    run_case(35, "style_LIN_TO_DOUBLE_LOG_inv");
}
