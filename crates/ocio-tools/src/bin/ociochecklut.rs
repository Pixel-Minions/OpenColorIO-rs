// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociochecklut`: checks any LUT file and optionally converts a pixel.
//!
//! Port of `src/apps/ociochecklut/main.cpp` @ v2.5.2, as both wheels compile it: without
//! `OCIO_GPU_ENABLED`. So `ProcessorWrapper` holds a CPU processor only, and each GPU option
//! ends in "Compiled without OpenGL support".

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use ocio::{Config, Exception, FileTransform, Interpolation, Transform, TransformDirection};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::logging::{set_logging_function, set_logging_level};
use ocio_ops::open_color_types::LoggingLevel;
use ocio_tools::argparse::{ArgParse, Param};
use ocio_tools::c_numbers::strtof;
use ocio_tools::is_ocio_exception;
use ocio_tools::stdio::{self, TransformText, cerr, cerr_line, cout, cout_endl, cout_line};

/// Port of `ToString(float)` (src/apps/ociochecklut/main.cpp:138-144 @ v2.5.2).
fn float_to_string(value: f32) -> Vec<u8> {
    let mut outs = OStringStream::new(Crt::NATIVE);
    outs.precision = 7;
    outs.put_f32(value);
    outs.into_bytes()
}

/// `std::cout.width(max(in.size(), out.size())); std::cout << in;`: the fill goes before.
///
/// Port of `PrintFirstComponent` (src/apps/ociochecklut/main.cpp:146-150 @ v2.5.2).
fn print_first_component(input: &[u8], output: &[u8]) {
    let width = input.len().max(output.len());
    cout(" ".repeat(width - input.len()));
    cout(input);
}

/// Port of `PrintAlignedVec` (src/apps/ociochecklut/main.cpp:152-166 @ v2.5.2).
fn print_aligned_vec(text: &[Vec<u8>], align: &[Vec<u8>], comp: usize) {
    print_first_component(&text[0], &align[0]);
    cout(b" ");
    print_first_component(&text[1], &align[1]);
    cout(b" ");
    print_first_component(&text[2], &align[2]);
    if comp == 4 {
        cout(b" ");
        print_first_component(&text[3], &align[3]);
    }
}

/// Port of `ToString(std::vector<std::string> &, ...)` (src/apps/ociochecklut/main.cpp:168-178
/// @ v2.5.2).
fn to_strings(vec: &[f32], comp: usize) -> Vec<Vec<u8>> {
    let mut text = vec![
        float_to_string(vec[0]),
        float_to_string(vec[1]),
        float_to_string(vec[2]),
    ];
    if comp == 4 {
        text.push(float_to_string(vec[3]));
    }
    text
}

/// Prints `Input  [R G B]: [...]` and `Output [R G B]: [...]`.
///
/// Port of the two blocks that do so (src/apps/ociochecklut/main.cpp:407-423, 471-487 @ v2.5.2).
fn print_in_out(input: &[Vec<u8>], output: &[Vec<u8>], comp: usize) {
    for (label, text, align) in [(b"Input  ", input, output), (b"Output ", output, input)] {
        cout(label);
        cout(b"[R G B");
        if comp == 4 {
            cout(b" A");
        }
        cout(b"]: [");
        print_aligned_vec(text, align, comp);
        cout_line(b"]");
    }
}

/// Port of `DESC_STRING` (src/apps/ociochecklut/main.cpp:180-185 @ v2.5.2).
const DESC_STRING: &str = "\n\
OCIOCHECKLUT loads any LUT type supported by OCIO and prints any errors\n\
encountered.  Provide a normalized RGB or RGBA value to send that through\n\
the LUT.  Alternatively use the -t option to evaluate a set of test values.\n\
Otherwise, if no RGB value is provided, a list of the operators in the LUT is printed.\n\
Use -v to print warnings while parsing the LUT.\n";

/// The values `-t` evaluates: `input4test`.
///
/// Port of `input4test` (src/apps/ociochecklut/main.cpp:344-353 @ v2.5.2).
const INPUT_4_TEST: [f32; 27] = [
    0.0, 0.0, 0.0, //
    0.18, 0.18, 0.18, //
    0.5, 0.5, 0.5, //
    1.0, 1.0, 1.0, //
    2.0, 2.0, 2.0, //
    100.0, 100.0, 100.0, //
    1.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, //
    0.0, 0.0, 1.0,
];

/// `catch (const OCIO::Exception &)` and `catch (...)` of a `try` block, with what each prints.
fn report(exception: &Exception, ocio_prefix: &str, unknown: &str) -> i32 {
    if is_ocio_exception(exception) {
        cerr(ocio_prefix);
        cerr_line(exception.what());
    } else {
        cerr_line(unknown);
    }
    1
}

/// Each step of a multi-transform LUT on one pixel.
///
/// Port of the `stepInfo` block (src/apps/ociochecklut/main.cpp:369-427 @ v2.5.2).
fn process_steps(
    config: &Config,
    t: &Transform,
    pixel: [f32; 4],
    comp: usize,
    printer: &mut TransformText,
) -> Result<(), Exception> {
    // Create GroupTransform so that each can be processed one at a time.
    let processor = config.processor(t)?;
    let transform = processor.create_group_transform()?;
    let mut input_pixel = pixel;
    let mut output_pixel = pixel;
    let num_transforms = transform.num_transforms();

    cout_endl();

    for i in 0..num_transforms {
        let transform_step = transform.transform(i)?;
        let processor_step = config.processor(transform_step)?;
        let cpu = processor_step.default_cpu_processor()?;

        // Process the pixel
        cpu.apply_rgba(&mut output_pixel)?;

        // Print the input/output pixel
        let input = to_strings(&input_pixel, comp);
        let output = to_strings(&output_pixel, comp);

        cout(b"\n");
        cout_line(printer.of(transform_step));
        print_in_out(&input, &output, comp);

        input_pixel = output_pixel;
    }
    Ok(())
}

/// Port of `main` (src/apps/ociochecklut/main.cpp:189-521 @ v2.5.2).
fn run(argv: &[Vec<u8>]) -> i32 {
    let verbose = Cell::new(false);
    let help = Cell::new(false);
    let test = Cell::new(false);
    let invlut = Cell::new(false);
    let usegpu = Cell::new(false);
    let usegpu_legacy = Cell::new(false);
    let outputgpu_info = Cell::new(false);
    let step_info = Cell::new(false);

    let inputfile = RefCell::new(Vec::new());
    let parsed = Cell::new(0);
    let input = RefCell::new(Vec::new());

    // Port of `parse_end_args` (src/apps/ociochecklut/main.cpp:123-136 @ v2.5.2).
    let parse_end_args = |arg: &[u8]| {
        if parsed.get() == 0 {
            *inputfile.borrow_mut() = arg.to_vec();
        } else {
            input.borrow_mut().push(strtof(arg));
        }
        parsed.set(parsed.get() + 1);
        0
    };

    let mut ap = ArgParse::new(
        "ociochecklut -- check any LUT file and optionally convert a pixel\n\n\
         usage:  ociochecklut <INPUTFILE> <R G B> or <R G B A>\n",
    );
    ap.sublist(&parse_end_args, "");
    ap.separator("Options:");
    ap.option(
        "-t",
        &[Param::Flag(&test)],
        "Test a set a predefined RGB values",
    );
    ap.option("-v", &[Param::Flag(&verbose)], "Verbose");
    ap.option(
        "-s",
        &[Param::Flag(&step_info)],
        "Print the output after each step in a multi - transform LUT",
    );
    ap.option("--help", &[Param::Flag(&help)], "Print help message");
    ap.option(
        "--inv",
        &[Param::Flag(&invlut)],
        "Apply LUT in inverse direction",
    );
    ap.option("--gpu", &[Param::Flag(&usegpu)], "Use GPU instead of CPU");
    ap.option(
        "--gpulegacy",
        &[Param::Flag(&usegpu_legacy)],
        "Use the legacy (i.e. baked) GPU color processing \
         instead of the CPU one (--gpu is ignored)",
    );
    ap.option(
        "--gpuinfo",
        &[Param::Flag(&outputgpu_info)],
        "Output the OCIO shader program",
    );

    if ap.parse(argv) < 0 || help.get() || inputfile.borrow().is_empty() {
        cout_line(ap.geterror());
        ap.usage();
        cout_line(DESC_STRING);
        if help.get() {
            // What are the allowed formats?
            cout_line(b"Formats supported:");
            for i in 0..FileTransform::num_formats() {
                cout(FileTransform::format_name_by_index(i));
                cout(b" (.");
                cout(FileTransform::format_extension_by_index(i));
                cout_line(b")");
            }
            return 0;
        }

        return 1;
    }

    let verbose = verbose.get();
    if verbose {
        cout_endl();
        cout_line(format!("OCIO Version: {}", ocio::version()));
    }

    if usegpu.get() || outputgpu_info.get() || usegpu_legacy.get() {
        cerr(b"Compiled without OpenGL support, GPU options are not available.");
        cerr(b"\n");
        return 1;
    }

    set_logging_level(LoggingLevel::Warning);

    // By default, the OCIO log goes to std::cerr, so we also print any log messages associated
    // with reading the transform.
    // Only a null function is refused.
    let _ = set_logging_function(Some(Arc::new(|message: &[u8]| cout(message))));

    let mut input = input.into_inner();
    let mut test = test.get();
    let step_info = step_info.get();
    let printops = input.is_empty() && !test;

    let config = Config::new().expect("Config::Create doesn't fail");

    // Create the OCIO processor for the specified transform.
    let mut t = FileTransform::new();
    t.set_src(inputfile.into_inner());
    t.set_interpolation(Interpolation::Best);
    t.set_direction(if invlut.get() {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    });
    let t = Transform::File(t);

    let mut printer = TransformText::default();
    let created = (|| -> Result<Arc<CpuProcessor>, Exception> {
        let processor = config.processor(&t)?;
        if printops {
            let transform = processor.create_group_transform()?;
            cout_line(b"Transform operators: ");
            let num_transforms = transform.num_transforms();
            for i in 0..num_transforms {
                cout(b"\t");
                cout_line(printer.of(transform.transform(i)?));
            }
            if num_transforms == 0 {
                cout_line(b"No transform.");
            }
        }
        processor.default_cpu_processor()
    })();
    let cpu = match created {
        Ok(cpu) => cpu,
        Err(exception) => {
            return report(
                &exception,
                "ERROR: ",
                "ERROR: Unknown error encountered while creating processor.",
            );
        }
    };

    if printops {
        // Only displays the LUT file content.
        return 0;
    }

    // Validate the input values.

    let mut num_input = input.len();

    if test && num_input > 0 {
        cerr_line(
            b"ERROR: Expecting either RGB (or RGBA) pixel or predefined RGB values (i.e. -t).",
        );
        return 1;
    }

    let mut comp = 3;
    if num_input == 4 {
        comp = 4;
    } else if num_input != 3 && !test {
        cerr_line(b"ERROR: Expecting either RGB or RGBA pixel.");
        return 1;
    }

    // Process the input values.

    let mut cur_pix = 0;

    if verbose || step_info {
        cout_endl();
    }

    loop {
        if cur_pix < num_input {
            let mut pixel = [
                input[cur_pix],
                input[cur_pix + 1],
                input[cur_pix + 2],
                if comp == 3 { 0.0 } else { input[cur_pix + 3] },
            ];

            if step_info {
                // Process each step in a multi - transform LUT
                if let Err(exception) = process_steps(&config, &t, pixel, comp, &mut printer) {
                    return report(
                        &exception,
                        "ERROR: ",
                        "ERROR: Unknown error encountered while processing single step operator.",
                    );
                }

                cur_pix += comp;
            } else {
                // Process in a single step
                if let Err(exception) = cpu.apply_rgba(&mut pixel) {
                    return report(
                        &exception,
                        "ERROR: Processing pixel: ",
                        "ERROR: Unknown error encountered while processing pixel.",
                    );
                }

                // Print to string so that in & out values can be aligned if needed.

                let out = to_strings(&pixel, comp);

                cout_endl();

                if verbose {
                    let in_text = to_strings(&input[cur_pix..], comp);
                    print_in_out(&in_text, &out, comp);
                } else {
                    cout(&out[0]);
                    cout(b" ");
                    cout(&out[1]);
                    cout(b" ");
                    cout(&out[2]);
                    if comp == 4 {
                        cout(b" ");
                        cout(&out[3]);
                    }
                    cout_endl();
                }
                cur_pix += comp;
            }
        } else if test {
            if verbose {
                cout_line(b"Testing with predefined set of RGB pixels.");
            }
            input = INPUT_4_TEST.to_vec();
            comp = 3;
            num_input = INPUT_4_TEST.len();
            cur_pix = 0;
            test = false;
        } else {
            return 0;
        }
    }
}

fn main() {
    stdio::init();
    stdio::exit(run(&stdio::args()))
}
