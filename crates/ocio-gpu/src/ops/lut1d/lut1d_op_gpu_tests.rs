// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/ops/lut1d/Lut1DOpGPU_tests.cpp` @ v2.5.2: the texture padding helpers.
//! `crates/ocio-gpu/tests/lut1d_op_gpu_oracle.rs` checks the shaders and textures against the
//! wheel.

use ocio_testkit::assert_f32_bits_eq;

use super::*;

/// Port of `OCIO_ADD_TEST(Lut1DOp, pad_lut_one_dimension)` @ v2.5.2.
#[test]
fn pad_lut_one_dimension() {
    const WIDTH: c_ulong = 6;

    // Create a channel multi row and smaller than the expected texture size.

    let mut channel = vec![0.0f32; ((WIDTH - 2) * 3) as usize];

    // Fill the channel.

    for idx in 0..channel.len() / 3 {
        channel[3 * idx] = idx as f32;
        channel[3 * idx + 1] = idx as f32 + 0.1f32;
        channel[3 * idx + 2] = idx as f32 + 0.2f32;
    }

    // Pad the texture values.

    let mut chn = Vec::new();
    create_padded_lut_channels(WIDTH, 1, &channel, &mut chn);

    // Check the values.

    const RES: [f32; 18] = [
        0.0, 0.1, 0.2, 1.0, 1.1, 1.2, //
        2.0, 2.1, 2.2, 3.0, 3.1, 3.2, //
        3.0, 3.1, 3.2, 3.0, 3.1, 3.2,
    ];

    assert_eq!(chn.len(), 18);
    assert_f32_bits_eq("chn", &RES, &chn);
}

/// Port of `OCIO_ADD_TEST(Lut1DOp, pad_lut_two_dimension_1)` @ v2.5.2.
#[test]
fn pad_lut_two_dimension_1() {
    const WIDTH: c_ulong = 4;
    const HEIGHT: c_ulong = 3;

    let mut channel = vec![0.0f32; ((HEIGHT * WIDTH - 4) * 3) as usize];

    for idx in 0..channel.len() / 3 {
        channel[3 * idx] = idx as f32;
        channel[3 * idx + 1] = idx as f32 + 0.1f32;
        channel[3 * idx + 2] = idx as f32 + 0.2f32;
    }

    let mut chn = Vec::new();
    create_padded_lut_channels(WIDTH, HEIGHT, &channel, &mut chn);

    const RES: [f32; 36] = [
        0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 2.0, 2.1, 2.2, 3.0, 3.1, 3.2, //
        3.0, 3.1, 3.2, 4.0, 4.1, 4.2, 5.0, 5.1, 5.2, 6.0, 6.1, 6.2, //
        6.0, 6.1, 6.2, 7.0, 7.1, 7.2, 7.0, 7.1, 7.2, 7.0, 7.1, 7.2,
    ];

    assert_eq!(chn.len(), 36);
    assert_f32_bits_eq("chn", &RES, &chn);
}

/// Port of `OCIO_ADD_TEST(Lut1DOp, pad_lut_two_dimension_2)` @ v2.5.2.
#[test]
fn pad_lut_two_dimension_2() {
    // Requested GPU texture dimensions.
    const WIDTH: c_ulong = 4;
    const HEIGHT: c_ulong = 3;

    // Internally, all LUTs have three channels (R, G & B).
    let lut_values: Vec<f32> = vec![
        0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 2.0, 2.1, 2.2, //
        3.0, 3.1, 3.2, 4.0, 4.1, 4.2, 5.0, 5.1, 5.2, //
        6.0, 6.1, 6.2, 7.0, 7.1, 7.2, 8.0, 8.1, 8.2,
    ];

    {
        // Create the padded buffer used by a GPU texture to perform the right
        // linear interpolation even for the last (on each row) texel value.
        let mut chn = Vec::new();
        create_padded_lut_channels(WIDTH, HEIGHT, &lut_values, &mut chn);

        // Here is the expected buffer for the 2D Texture padded to width & height for
        // the three channels (R, G, B).
        const RES: [f32; (WIDTH * HEIGHT * 3) as usize] = [
            0.0, 0.1, 0.2, 1.0, 1.1, 1.2, 2.0, 2.1, 2.2, 3.0, 3.1, 3.2, //
            3.0, 3.1, 3.2, 4.0, 4.1, 4.2, 5.0, 5.1, 5.2, 6.0, 6.1, 6.2, //
            6.0, 6.1, 6.2, 7.0, 7.1, 7.2, 8.0, 8.1, 8.2, 8.0, 8.1, 8.2,
        ];

        assert_eq!(chn.len(), (WIDTH * HEIGHT * 3) as usize);

        assert_f32_bits_eq("chn", &RES, &chn);
    }

    {
        // Test as all channels were identical i.e. only the RED one is used & padded.

        let mut padded_channel = Vec::new();
        create_padded_red_channel(WIDTH, HEIGHT, &lut_values, &mut padded_channel);

        // Here is the expected buffer for the 2D Texture padded to width & height for
        // the Red channel only i.e. no G & B channels.
        const RES: [f32; (WIDTH * HEIGHT) as usize] = [
            0.0, 1.0, 2.0, 3.0, //
            3.0, 4.0, 5.0, 6.0, //
            6.0, 7.0, 8.0, 8.0,
        ];

        assert_eq!(padded_channel.len(), (WIDTH * HEIGHT) as usize);

        assert_f32_bits_eq("padded_channel", &RES, &padded_channel);
    }
}

/// U-5: where upstream divides by zero (a width limit of 0), loops forever (a texture 1 texel
/// wide in more than one row) or pads more entries than the texture holds (8191 entries at the
/// default limit of 4096), the port refuses the LUT; the oracle refuses those requests too
/// (`gpu_shader`'s `_padding_fits`). The lengths around them fit.
#[test]
fn luts_that_do_not_fit_their_texture_are_refused() {
    let refused = |length: c_ulong, max_width: u32| {
        let lut = Lut1DOpData::new(length).expect("a LUT");
        let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
        desc.set_texture_max_width(max_width);
        match get_lut1d_gpu_shader_program(&mut desc, &lut) {
            Ok(()) => false,
            Err(e) => {
                assert_eq!(
                    e.message(),
                    format!(
                        "The Lut1DOp of {length} entries doesn't fit in a texture at most \
                         {max_width} texels wide."
                    )
                );
                assert_eq!(desc.num_textures(), 0);
                true
            }
        }
    };
    assert!(refused(16, 0));
    assert!(refused(2, 1));
    assert!(refused(8191, 4096));
    assert!(refused(12286, 4096));
    assert!(refused(12287, 4096));
    for (length, max_width) in [(8190, 4096), (8192, 4096), (12285, 4096), (2, 2), (17, 16)] {
        assert!(!refused(length, max_width), "{length} at {max_width}");
    }
}
