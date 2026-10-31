// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociowrite`: writes a color transformation to a file.
//!
//! Port of `src/apps/ociowrite/main.cpp` @ v2.5.2. The tool prints only through the streams of
//! `ocio_tools::stdio`, which are upstream's `std::cout` and `std::cerr`.

use std::cell::{Cell, RefCell};

use ocio::{
    Config, Exception, GroupTransform, Processor, TransformDirection, get_current_config,
    get_env_variable,
};
use ocio_tools::argparse::{ArgParse, Param};
use ocio_tools::is_ocio_exception;
use ocio_tools::stdio::{self, OutputFile, cerr, cerr_line, cout, cout_endl, cout_line};

/// The bytes of `a`, then `b`.
fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// `"(" + display + ", " + view + ")"`
fn display_view_pair(display: &[u8], view: &[u8]) -> Vec<u8> {
    cat(&[b"(", display, b", ", view, b")"])
}

/// The options that select the transform.
struct Selection {
    input_color_space: Vec<u8>,
    output_color_space: Vec<u8>,
    display: Vec<u8>,
    view: Vec<u8>,
}

/// The part of `main` that upstream runs in its `try` block: `Ok` holds the tool's exit code.
///
/// Port of `main` (src/apps/ociowrite/main.cpp:143-280 @ v2.5.2). The checks of the output
/// arguments inside each branch of `useColorspaces`, `useDisplayview` and `useInvertview` can't
/// fail, since the branch's own condition holds them, so they aren't ported.
fn process_transform(
    selection: &Selection,
    verbose: bool,
    filepath: &[u8],
    transform_file_format: &[u8],
) -> Result<i32, Exception> {
    let Selection {
        input_color_space,
        output_color_space,
        display,
        view,
    } = selection;

    // Checking for an input colorspace or input (display, view) pair.
    if input_color_space.is_empty() && (display.is_empty() || view.is_empty()) {
        cerr(b"\n");
        cerr_line(b"Colorspaces or (display,view) pair must be specified as source.");
        return Ok(1);
    }

    let env = get_env_variable("OCIO");
    if !env.is_empty() {
        if verbose {
            cout_endl();
            let input = if !input_color_space.is_empty() {
                input_color_space.clone()
            } else {
                display_view_pair(display, view)
            };
            let output = if !output_color_space.is_empty() {
                output_color_space.clone()
            } else {
                display_view_pair(display, view)
            };
            cout(cat(&[b"Processing from '", &input, b"' to '", &output]));
            cout_line(b"'");
        }
    } else {
        cerr(b"\n");
        cerr_line(b"Missing the ${OCIO} env. variable.");
        return Ok(1);
    }

    let config = Config::create_from_env()?;

    if verbose {
        cout_endl();
        cout(b"Config: ");
        cout(config.description());
        cout(format!(" - version: {}", config.major_version()));
        let minor = config.minor_version();
        if minor != 0 {
            cout(format!(".{minor}"));
        }
        cout_endl();
    }

    // --colorspaces
    let use_colorspaces = !input_color_space.is_empty() && !output_color_space.is_empty();
    // --view
    let use_displayview = !input_color_space.is_empty() && !display.is_empty() && !view.is_empty();
    // --invertview
    let use_invertview = !display.is_empty() && !view.is_empty() && !output_color_space.is_empty();

    // Errors validation
    if !((use_colorspaces && !use_displayview && !use_invertview)
        || (use_displayview && !use_colorspaces && !use_invertview)
        || (use_invertview && !use_colorspaces && !use_displayview))
    {
        cerr(b"\n");
        cerr_line(b"Any combinations of --colorspaces, --view or --invertview is invalid.");
        return Ok(1);
    }

    let processor: std::sync::Arc<Processor> = if use_colorspaces {
        // colorspaces to colorspaces
        config.processor_between(input_color_space, output_color_space)?
    } else if use_displayview {
        // colorspaces to (display,view) pair
        config.processor_for_display_view(
            input_color_space,
            display,
            view,
            TransformDirection::Forward,
        )?
    } else {
        // (display,view) pair to colorspaces
        config.processor_for_display_view(
            output_color_space,
            display,
            view,
            TransformDirection::Inverse,
        )?
    };

    match OutputFile::create(filepath) {
        Ok(mut outfs) => {
            let group = processor.create_group_transform()?;
            group.write(&config, transform_file_format, &mut outfs)?;
        }
        Err(_) => {
            cerr(b"\n");
            cerr(b"Could not open file: ");
            cerr_line(filepath);
            return Ok(1);
        }
    }
    Ok(0)
}

/// Port of `main` (src/apps/ociowrite/main.cpp:19-294 @ v2.5.2).
fn run(argv: &[Vec<u8>]) -> i32 {
    let verbose = Cell::new(false);
    let input_color_space = RefCell::new(Vec::new());
    let output_color_space = RefCell::new(Vec::new());
    let display = RefCell::new(Vec::new());
    let view = RefCell::new(Vec::new());
    let filepath = RefCell::new(Vec::new());

    let help = Cell::new(false);

    // What are the allowed writing output formats?
    let mut formats =
        b"\n                            Formats to write to:\n                             "
            .to_vec();
    for i in 0..GroupTransform::num_write_formats() {
        formats.extend_from_slice(GroupTransform::format_name_by_index(i));
        formats.extend_from_slice(b" (.");
        formats.extend_from_slice(GroupTransform::format_extension_by_index(i));
        formats.extend_from_slice(b")\n                             ");
    }

    let path_desc = cat(&[
        b"Transform file path. Format is implied by extension. ",
        &formats,
    ]);

    let mut ap = ArgParse::new(
        "ociowrite -- write a color transformation to a file\n\n\
         usage: ociowrite [options] --file outputfile\n\n",
    );
    ap.option("--h", &[Param::Flag(&help)], "Display the help and exit");
    ap.option("--help", &[Param::Flag(&help)], "Display the help and exit");
    ap.option(
        "--v",
        &[Param::Flag(&verbose)],
        "Display some general information",
    );
    ap.option(
        "--colorspaces %s %s",
        &[
            Param::Text(&input_color_space),
            Param::Text(&output_color_space),
        ],
        "Provide the input and output color spaces to apply on the image",
    );
    ap.option(
        "--view %s %s %s",
        &[
            Param::Text(&input_color_space),
            Param::Text(&display),
            Param::Text(&view),
        ],
        "Provide the input color space and (display, view) pair to apply on the image",
    );
    ap.option(
        "--displayview %s %s %s",
        &[
            Param::Text(&input_color_space),
            Param::Text(&display),
            Param::Text(&view),
        ],
        "(Deprecated) Provide the input color space and (display, view) pair to apply on the image",
    );
    ap.option(
        "--invertview %s %s %s",
        &[
            Param::Text(&display),
            Param::Text(&view),
            Param::Text(&output_color_space),
        ],
        "Provide the (display, view) pair and output color space to apply on the image",
    );
    ap.option("--file %s", &[Param::Text(&filepath)], &path_desc);

    if argv.len() <= 1 || ap.parse(argv) < 0 {
        cerr_line(ap.geterror());
        ap.usage();
        return 1;
    }

    if help.get() {
        ap.usage();
        return 1;
    }

    let verbose = verbose.get();
    if verbose {
        cout_endl();
        cout_line(cat(&[b"OCIO Version: ", ocio::version().as_bytes()]));
        let env = get_env_variable("OCIO");
        if !env.is_empty() {
            cout_endl();
            cout_line(cat(&[b"OCIO Configuration: '", &env, b"'"]));
            match get_current_config() {
                Ok(config) => {
                    cout_line(cat(&[b"OCIO search_path:    ", &config.search_path()]));
                }
                Err(_) => {
                    cerr(cat(&[b"Error loading the config file: '", &env, b"'"]));
                    return 1;
                }
            }
        }
    }

    let filepath = filepath.into_inner();
    if filepath.is_empty() {
        cerr(b"\n");
        cerr_line(b"The output transform filepath is missing.");
        return 1;
    }

    let mut transform_file_format = Vec::new();
    if let Some(dot) = filepath.iter().rposition(|&c| c == b'.') {
        let requested_ext = filepath[dot + 1..].to_ascii_lowercase();
        for i in 0..GroupTransform::num_write_formats() {
            if requested_ext == GroupTransform::format_extension_by_index(i) {
                transform_file_format = GroupTransform::format_name_by_index(i).to_vec();
                break;
            }
        }
    }

    if transform_file_format.is_empty() {
        cerr(b"\n");
        cerr(b"Could not find a valid format from the extension of: '");
        cerr(&filepath);
        cerr(b"'. ");
        cerr_line(&formats);
        return 1;
    } else if verbose {
        cout_endl();
        cout_line(cat(&[b"File format being used: ", &transform_file_format]));
    }

    cout_endl();

    // Process transform.
    let selection = Selection {
        input_color_space: input_color_space.into_inner(),
        output_color_space: output_color_space.into_inner(),
        display: display.into_inner(),
        view: view.into_inner(),
    };
    match process_transform(&selection, verbose, &filepath, &transform_file_format) {
        Ok(code) => code,
        Err(exception) if is_ocio_exception(&exception) => {
            cerr(b"OCIO Error: ");
            cerr_line(exception.what());
            1
        }
        Err(_) => {
            cerr_line(b"Unknown OCIO error encountered.");
            1
        }
    }
}

fn main() {
    stdio::init();
    stdio::exit(run(&stdio::args()))
}
