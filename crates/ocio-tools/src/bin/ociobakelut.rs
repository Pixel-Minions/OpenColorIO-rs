// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociobakelut`: creates a new LUT or ICC profile from an OCIO config or LUT file(s).
//!
//! Port of `src/apps/ociobakelut/main.cpp` @ v2.5.2.
//!
//! **ICC output isn't ported.** `SaveICCProfileToFile` (`ocioicc.cpp`) needs lcms2, which is a
//! new dependency or a translation of third-party code: that is the owner's decision. Everything
//! up to that call is ported, so a command line the wheel refuses is refused the same way, and
//! one that would write a profile reports [`ICC_NOT_PORTED`] and exits with 1.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use ocio::{
    Baker, CdlTransform, ColorSpace, ColorSpaceDirection, Config, DisplayViewTransform, Exception,
    FileTransform, GroupTransform, Interpolation, LegacyViewingPipeline, LookTransform,
    OptimizationFlags, Transform, TransformDirection, get_env_variable,
};
use ocio_tools::argparse::{ArgParse, Param};
use ocio_tools::c_numbers::atof;
use ocio_tools::is_ocio_exception;
use ocio_tools::stdio::{self, CoutWriter, OutputFile, cerr, cerr_line, cout, cout_line};

/// What `--format icc` reports where the wheel would write the profile.
const ICC_NOT_PORTED: &str = "ERROR: ICC output is not available in this port of ociobakelut \
                              (SaveICCProfileToFile needs lcms2; docs/cards/phase10-tools.md).";

/// `std::cerr << "See --help for more info." << std::endl`, and upstream's `return 1`.
fn see_help() -> i32 {
    cerr_line(b"See --help for more info.");
    1
}

/// `std::cerr << "\nERROR: " << text << "\n\n"`, with the line that follows it.
fn refuse(text: &str) -> i32 {
    cerr(format!("\nERROR: {text}\n\n"));
    see_help()
}

/// Three numbers after a CDL option: `atof` of `argv[i+1]` to `argv[i+3]`.
fn three_numbers(argv: &[Vec<u8>], i: usize) -> [f64; 3] {
    [atof(&argv[i + 1]), atof(&argv[i + 2]), atof(&argv[i + 3])]
}

/// The transforms of the options that may repeat, in order.
///
/// Port of `parse_luts` (src/apps/ociobakelut/main.cpp:431-573 @ v2.5.2).
fn parse_luts(argv: &[Vec<u8>]) -> Result<GroupTransform, Exception> {
    let mut group_transform = GroupTransform::new();
    let mut last_ccc_id: Option<&[u8]> = None; // Ugly to use this but using
    // GroupTransform::getTransform() returns a const object so we must set this prior to using
    // --lut for now.

    let argc = argv.len();
    let invalid =
        |option: &str| Exception::new(format!("Error parsing --{option}. Invalid num args"));

    let mut i = 0;
    while i < argc {
        let arg = argv[i].as_slice();

        if arg == b"--lut" || arg == b"-lut" {
            if i + 1 >= argc {
                return Err(invalid("lut"));
            }

            let mut t = FileTransform::new();
            t.set_src(&argv[i + 1]);
            t.set_interpolation(Interpolation::Best);
            if let Some(ccc_id) = last_ccc_id {
                t.set_ccc_id(ccc_id);
            }
            group_transform.append_transform(Transform::File(t));

            i += 1;
        } else if arg == b"--cccid" || arg == b"-cccid" {
            if i + 1 >= argc {
                return Err(invalid("cccid"));
            }

            last_ccc_id = Some(&argv[i + 1]);

            i += 1;
        } else if arg == b"--invlut" || arg == b"-invlut" {
            if i + 1 >= argc {
                return Err(invalid("invlut"));
            }

            let mut t = FileTransform::new();
            t.set_src(&argv[i + 1]);
            t.set_interpolation(Interpolation::Best);
            t.set_direction(TransformDirection::Inverse);
            group_transform.append_transform(Transform::File(t));

            i += 1;
        } else if arg == b"--slope" || arg == b"-slope" {
            if i + 3 >= argc {
                return Err(invalid("slope"));
            }

            let mut t = CdlTransform::new();
            t.set_slope(&three_numbers(argv, i));
            group_transform.append_transform(Transform::Cdl(t));

            i += 3;
        } else if arg == b"--offset" || arg == b"-offset" {
            if i + 3 >= argc {
                return Err(invalid("offset"));
            }

            let mut t = CdlTransform::new();
            t.set_offset(&three_numbers(argv, i));
            group_transform.append_transform(Transform::Cdl(t));

            i += 3;
        } else if arg == b"--offset10" || arg == b"-offset10" {
            if i + 3 >= argc {
                return Err(invalid("offset10"));
            }

            let mut t = CdlTransform::new();
            let offset = three_numbers(argv, i).map(|value| value / 1023.0);
            t.set_offset(&offset);
            group_transform.append_transform(Transform::Cdl(t));

            i += 3;
        } else if arg == b"--power" || arg == b"-power" {
            if i + 3 >= argc {
                return Err(invalid("power"));
            }

            let mut t = CdlTransform::new();
            t.set_power(&three_numbers(argv, i));
            group_transform.append_transform(Transform::Cdl(t));

            i += 3;
        } else if arg == b"--sat" || arg == b"-sat" {
            if i + 1 >= argc {
                return Err(invalid("sat"));
            }

            let mut t = CdlTransform::new();
            t.set_sat(atof(&argv[i + 1]));
            group_transform.append_transform(Transform::Cdl(t));

            i += 1;
        }
        i += 1;
    }

    Ok(group_transform)
}

/// The options of the bake.
struct Options {
    cubesize: i32,
    shapersize: i32,
    format: Vec<u8>,
    inputspace: Vec<u8>,
    shaperspace: Vec<u8>,
    looks: Vec<u8>,
    outputspace: Vec<u8>,
    display: Vec<u8>,
    view: Vec<u8>,
    usestdout: bool,
    verbose: bool,
    outputfile: Vec<u8>,
    description_is_empty: bool,
}

/// The `format == "icc"` branch, up to the call of `SaveICCProfileToFile`.
///
/// Port of src/apps/ociobakelut/main.cpp:292-356 @ v2.5.2.
fn icc(config: &Config, options: &Options) -> Result<i32, Exception> {
    let Options {
        looks,
        inputspace,
        outputspace,
        shaperspace,
        display,
        view,
        outputfile,
        usestdout,
        verbose,
        description_is_empty,
        ..
    } = options;

    if *description_is_empty && *verbose {
        cout(b"[OpenColorIO INFO]: \"--description\" set to default value of filename.icc: ");
        cout_line(outputfile);
    }

    if *usestdout {
        return Ok(refuse("--stdout not supported when writing ICC profiles."));
    }

    if outputfile.is_empty() {
        cerr(b"ERROR: you need to specify a output ICC path\n");
        return Ok(see_help());
    }

    if !shaperspace.is_empty() {
        cerr(b"WARNING: shaperspace is ignored when generating ICC profiles.\n");
    }

    let processor = if !display.is_empty() && !view.is_empty() {
        let mut transform = DisplayViewTransform::new();
        transform.set_src(inputspace);
        transform.set_display(display);
        transform.set_view(view);

        let mut vp = LegacyViewingPipeline::new();
        vp.set_display_view_transform(Some(&transform));
        vp.set_looks_override_enabled(!looks.is_empty());
        vp.set_looks_override(looks);

        vp.processor(config, None)?
    } else {
        let mut transform = LookTransform::new();
        transform.set_looks(looks);
        transform.set_src(inputspace);
        transform.set_dst(outputspace);
        config.processor_in_direction(&Transform::Look(transform), TransformDirection::Forward)?
    };

    processor.optimized_cpu_processor(OptimizationFlags::LOSSLESS)?;

    cerr_line(ICC_NOT_PORTED);
    Ok(1)
}

/// The output of `main`'s last `try` block.
///
/// Port of src/apps/ociobakelut/main.cpp:288-395 @ v2.5.2.
fn bake(config: &Config, options: &Options) -> Result<i32, Exception> {
    if options.format == b"icc" {
        return icc(config, options);
    }

    let mut baker = Baker::new();

    // setup the baker for our LUT type
    baker.set_config(config);
    baker.set_format(&options.format)?;
    baker.set_input_space(&options.inputspace);
    baker.set_shaper_space(&options.shaperspace);
    baker.set_looks(&options.looks);
    baker.set_target_space(&options.outputspace);
    baker.set_display_view(&options.display, &options.view);
    if options.shapersize != -1 {
        baker.set_shaper_size(options.shapersize);
    }
    if options.cubesize != -1 {
        baker.set_cube_size(options.cubesize);
    }

    if !options.usestdout && options.verbose {
        cout(b"[OpenColorIO INFO]: Baking '");
        cout(&options.format);
        cout_line(b"' LUT");
    }

    if options.usestdout {
        baker.bake(&mut CoutWriter)?;
    } else {
        let Ok(mut f) = OutputFile::create(&options.outputfile) else {
            cerr(b"ERROR: Non-writable file path ");
            cerr(&options.outputfile);
            cerr_line(b" specified.");
            return Ok(1);
        };
        baker.bake(&mut f)?;
        if options.verbose {
            cout(b"[OpenColorIO INFO]: Wrote '");
            cout(&options.outputfile);
            cout_line(b"'");
        }
    }
    Ok(0)
}

/// Port of `main` (src/apps/ociobakelut/main.cpp:33-417 @ v2.5.2).
fn run(argv: &[Vec<u8>]) -> i32 {
    let help = Cell::new(false);
    let cubesize = Cell::new(-1);
    let shapersize = Cell::new(-1); // cubsize^2
    let format = RefCell::new(Vec::new());
    let inputconfig = RefCell::new(Vec::new());
    let inputspace = RefCell::new(Vec::new());
    let shaperspace = RefCell::new(Vec::new());
    let looks = RefCell::new(Vec::new());
    let outputspace = RefCell::new(Vec::new());
    let display = RefCell::new(Vec::new());
    let view = RefCell::new(Vec::new());
    let usestdout = Cell::new(false);
    let verbose = Cell::new(false);

    let whitepointtemp = Cell::new(6505);
    let displayicc = RefCell::new(Vec::new());
    let description = RefCell::new(Vec::new());
    let copyright = RefCell::new(b"No copyright. Use freely.".to_vec());

    let outputfile = RefCell::new(Vec::new());

    // Port of `parse_end_args` (src/apps/ociobakelut/main.cpp:21-29 @ v2.5.2).
    let parse_end_args = |arg: &[u8]| {
        *outputfile.borrow_mut() = arg.to_vec();
        0
    };

    // What are the allowed baker output formats?
    let mut formats = b"the LUT format to bake: ".to_vec();
    for i in 0..Baker::num_formats() {
        if i != 0 {
            formats.extend_from_slice(b", ");
        }
        formats.extend_from_slice(Baker::format_name_by_index(i));
        formats.extend_from_slice(b" (.");
        formats.extend_from_slice(Baker::format_extension_by_index(i));
        formats.extend_from_slice(b")");
    }
    formats.extend_from_slice(b", icc (.icc)");

    let dummystr = RefCell::new(Vec::new());
    let dummyf1 = Cell::new(0.0);
    let dummyf2 = Cell::new(0.0);
    let dummyf3 = Cell::new(0.0);

    let dummy_floats = [
        Param::Float(&dummyf1),
        Param::Float(&dummyf2),
        Param::Float(&dummyf3),
    ];

    let mut ap = ArgParse::new(
        "ociobakelut -- create a new LUT or ICC profile from an OCIO config or LUT file(s)\n\n\
         usage:  ociobakelut [options] <OUTPUTFILE.LUT>\n\n\
         example:  ociobakelut --inputspace lg10 --outputspace srgb8 --format flame lg_to_srgb.3dl\n\
         example:  ociobakelut --lut filmlut.3dl --lut calibration.3dl --format flame display.3dl\n\
         example:  ociobakelut --cccid 0 --lut cdlgrade.ccc --lut calibration.3dl --format flame graded_display.3dl\n\
         example:  ociobakelut --lut look.3dl --offset 0.01 -0.02 0.03 --lut display.3dl --format flame display_with_look.3dl\n\
         example:  ociobakelut --inputspace lg10 --outputspace srgb8 --format icc ~/Library/ColorSync/Profiles/test.icc\n\
         example:  ociobakelut --inputspace lin --shaperspace lg10 --outputspace lg10 --format spi1d lintolog.spi1d\n\
         example:  ociobakelut --inputspace lg10 --displayview sRGB Film --format spi3d display_view.spi3d\n\
         example:  ociobakelut --lut filmlut.3dl --lut calibration.3dl --format icc ~/Library/ColorSync/Profiles/test.icc\n\n",
    );
    ap.sublist(&parse_end_args, "");
    ap.separator("Using Existing OCIO Configurations");
    ap.separator("    (use either displayview or outputspace, but not both)");
    ap.option(
        "--inputspace %s",
        &[Param::Text(&inputspace)],
        "Input OCIO ColorSpace (or Role)",
    );
    ap.option(
        "--displayview %s %s",
        &[Param::Text(&display), Param::Text(&view)],
        "Output OCIO Display and View",
    );
    ap.option(
        "--outputspace %s",
        &[Param::Text(&outputspace)],
        "Output OCIO ColorSpace (or Role)",
    );
    ap.option(
        "--shaperspace %s",
        &[Param::Text(&shaperspace)],
        "the OCIO ColorSpace or Role, for the shaper",
    );
    ap.option(
        "--looks %s",
        &[Param::Text(&looks)],
        "the OCIO looks to apply",
    );
    ap.option(
        "--iconfig %s",
        &[Param::Text(&inputconfig)],
        "Input .ocio configuration file (default: $OCIO)\n",
    );
    ap.separator("Config-Free LUT Baking");
    ap.separator("    (all options can be specified multiple times, each is applied in order)");
    ap.option(
        "--cccid %s",
        &[Param::Text(&dummystr)],
        "Specify a CCCId for any following LUTs",
    );
    ap.option(
        "--lut %s",
        &[Param::Text(&dummystr)],
        "Specify a LUT (forward direction)",
    );
    ap.option(
        "--invlut %s",
        &[Param::Text(&dummystr)],
        "Specify a LUT (inverse direction)",
    );
    ap.option("--slope %f %f %f", &dummy_floats, "slope");
    ap.option("--offset %f %f %f", &dummy_floats, "offset (float)");
    ap.option("--offset10 %f %f %f", &dummy_floats, "offset (10-bit)");
    ap.option("--power %f %f %f", &dummy_floats, "power");
    ap.option(
        "--sat %f",
        &[Param::Float(&dummyf1)],
        "saturation (ASC-CDL luma coefficients)\n",
    );
    ap.separator("Baking Options");
    ap.option("--format %s", &[Param::Text(&format)], &formats);
    ap.option(
        "--shapersize %d",
        &[Param::Int(&shapersize)],
        "size of the shaper (default: format specific)",
    );
    ap.option(
        "--cubesize %d",
        &[Param::Int(&cubesize)],
        "size of the main LUT (3d or 1d) (default: format specific)",
    );
    ap.option(
        "--stdout",
        &[Param::Flag(&usestdout)],
        "Write to stdout (rather than file)",
    );
    ap.option("--v", &[Param::Flag(&verbose)], "Verbose");
    ap.option("--help", &[Param::Flag(&help)], "Print help message\n");
    ap.separator("ICC Options");
    ap.option(
        "--whitepoint %d",
        &[Param::Int(&whitepointtemp)],
        "whitepoint for the profile (default: 6505)",
    );
    ap.option(
        "--displayicc %s",
        &[Param::Text(&displayicc)],
        "an ICC profile which matches the OCIO profiles target display",
    );
    ap.option(
        "--description %s",
        &[Param::Text(&description)],
        "a meaningful description, this will show up in UI like photoshop (defaults to \"filename.icc\")",
    );
    ap.option(
        "--copyright %s",
        &[Param::Text(&copyright)],
        "a copyright field added in the file (default: \"No copyright. Use freely.\")\n",
    );

    if ap.parse(argv) < 0 {
        cout_line(ap.geterror());
        ap.usage();
        cout(b"\n");
        return 1;
    }

    if help.get() || argv.len() == 1 {
        ap.usage();
        cout(b"\n");
        return 1;
    }

    // If we're printing to stdout, disable verbose printouts
    if usestdout.get() {
        verbose.set(false);
    }

    let usestdout = usestdout.get();
    let verbose = verbose.get();
    let mut inputspace = inputspace.into_inner();
    let mut outputspace = outputspace.into_inner();
    let looks = looks.into_inner();
    let shaperspace = shaperspace.into_inner();
    let display = display.into_inner();
    let view = view.into_inner();
    let format = format.into_inner();
    let inputconfig = inputconfig.into_inner();
    let outputfile = outputfile.into_inner();

    let group_transform = match parse_luts(argv) {
        Ok(group_transform) => group_transform,
        Err(exception) => {
            cerr(b"\nERROR: ");
            cerr_line(exception.what());
            return see_help();
        }
    };

    // Create the OCIO processor for the specified transform.

    // If --luts have been specified, synthesize a new (temporary) configuration
    // with the transformation embedded in a colorspace.
    let config: Arc<Config> = if group_transform.num_transforms() > 0 {
        if !inputspace.is_empty() {
            return refuse("--inputspace is not allowed when using --lut");
        }
        if !outputspace.is_empty() {
            return refuse("--outputspace is not allowed when using --lut");
        }
        if !looks.is_empty() {
            return refuse("--looks is not allowed when using --lut");
        }
        if !shaperspace.is_empty() {
            return refuse("--shaperspace is not allowed when using --lut");
        }
        if !display.is_empty() || !view.is_empty() {
            return refuse("--displayview is not allowed when using --lut");
        }

        let mut editable_config = Config::new().expect("Config::Create doesn't fail");

        let mut input_color_space = ColorSpace::new();
        inputspace = b"RawInput".to_vec();
        input_color_space.set_name(&inputspace);
        // An unnamed color space or a name in use is refused; this one is neither.
        editable_config
            .add_color_space(&input_color_space)
            .expect("a new color space");

        let mut output_color_space = ColorSpace::new();
        outputspace = b"ProcessedOutput".to_vec();
        output_color_space.set_name(&outputspace);

        if let Err(exception) = output_color_space.set_transform(
            Some(&Transform::Group(group_transform.clone())),
            ColorSpaceDirection::FromReference,
        ) {
            return unhandled(&exception);
        }

        if verbose {
            cout(b"[OpenColorIO DEBUG]: Specified Transform:");
            cout(Transform::Group(group_transform).to_bytes());
            cout(b"\n");
        }

        editable_config
            .add_color_space(&output_color_space)
            .expect("a new color space");
        Arc::new(editable_config)
    } else {
        if inputspace.is_empty() {
            return refuse("You must specify the --inputspace.");
        }

        if outputspace.is_empty() && (display.is_empty() && view.is_empty()) {
            return refuse("You must specify either --outputspace or --displayview.");
        }

        if display.is_empty() != view.is_empty() {
            return refuse("You must specify both display and view with --displayview.");
        }

        if format.is_empty() {
            return refuse("You must specify the LUT format using --format.");
        }

        // GetEnvVariable never gives a null pointer, so the branch that asks for a config is
        // dead (docs/improvements.md, I-602), and isn't ported.
        let loaded = if !inputconfig.is_empty() {
            if !usestdout && verbose {
                cout(b"[OpenColorIO INFO]: Loading ");
                cout_line(&inputconfig);
            }
            Config::create_from_file(&inputconfig)
        } else {
            if !usestdout && verbose {
                cout(b"[OpenColorIO INFO]: Loading $OCIO ");
                cout_line(get_env_variable("OCIO"));
            }
            Config::create_from_env()
        };
        match loaded {
            Ok(config) => config,
            Err(exception) => return unhandled(&exception),
        }
    };

    if outputfile.is_empty() && !usestdout {
        return refuse("You must specify the outputfile or --stdout.");
    }

    let options = Options {
        cubesize: cubesize.get(),
        shapersize: shapersize.get(),
        format,
        inputspace,
        shaperspace,
        looks,
        outputspace,
        display,
        view,
        usestdout,
        verbose,
        outputfile,
        description_is_empty: description.borrow().is_empty(),
    };

    match bake(&config, &options) {
        Ok(code) => code,
        Err(exception) => {
            if is_ocio_exception(&exception) {
                cerr(b"OCIO Error: ");
                cerr_line(exception.what());
            } else {
                cerr(b"Error: ");
                cerr(exception.what());
                cerr(b"\n");
            }
            see_help()
        }
    }
}

/// An exception that upstream's `main` doesn't catch where it happens, which ends its process
/// (`std::terminate`). The port reports it (docs/improvements.md, U-600).
fn unhandled(exception: &Exception) -> i32 {
    cerr(b"ERROR: ");
    cerr_line(exception.what());
    see_help()
}

fn main() {
    stdio::init();
    stdio::exit(run(&stdio::args()))
}
