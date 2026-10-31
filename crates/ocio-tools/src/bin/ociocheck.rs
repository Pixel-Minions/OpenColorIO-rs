// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ociocheck`: validates an OpenColorIO configuration.
//!
//! Port of `src/apps/ociocheck/main.cpp` @ v2.5.2.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use ocio::{
    ColorSpace, ColorSpaceDirection, ColorSpaceVisibility, Config, EnvironmentMode, Exception,
    FixedFunctionStyle, FixedFunctionTransform, NamedTransformVisibility, ProcessorCacheFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, Transform, TransformDirection, ViewType,
    get_env_variable,
};
use ocio_ops::logging::set_logging_level;
use ocio_ops::open_color_types::LoggingLevel;
use ocio_ops::parse_utils::{
    ROLE_COLOR_PICKING, ROLE_COLOR_TIMING, ROLE_COMPOSITING_LOG, ROLE_DATA, ROLE_DEFAULT,
    ROLE_INTERCHANGE_DISPLAY, ROLE_INTERCHANGE_SCENE, ROLE_MATTE_PAINT, ROLE_REFERENCE,
    ROLE_RENDERING, ROLE_SCENE_LINEAR, ROLE_TEXTURE_PAINT,
};
use ocio_ops::utils::string_utils::{contain, split_by_lines};
use ocio_tools::argparse::{ArgParse, Param};
use ocio_tools::is_ocio_exception;
use ocio_tools::log_guard::LogGuard;
use ocio_tools::stdio::{self, OutputFile, cout, cout_endl, cout_line};

/// Port of `DESC_STRING` (src/apps/ociocheck/main.cpp:19-28 @ v2.5.2).
const DESC_STRING: &str = "\n\n\
Ociocheck is useful to validate that the specified OCIO configuration\n\
is valid, and that all the color transforms are defined and loadable.\n\
For example, it is possible that the configuration may reference\n\
lookup tables that do not exist and ociocheck will find these cases.\n\
Unlike the config validate method, ociocheck parses all required LUTs.\n\
All display/view pairs, color spaces, and named transforms are checked,\n\
regardless of whether they are active or inactive.\n\n\
Ociocheck can also be used to clean up formatting on an existing profile\n\
that has been manually edited, using the '-o' option.\n";

/// Port of `cifTextureIDs` and `cifDisplayIDs` (src/apps/ociocheck/main.cpp:36-68 @ v2.5.2).
fn is_color_interop_forum_id(id: &[u8]) -> bool {
    const IDS: [&[u8]; 28] = [
        b"lin_ap1_scene",
        b"lin_ap0_scene",
        b"lin_rec709_scene",
        b"lin_p3d65_scene",
        b"lin_rec2020_scene",
        b"lin_adobergb_scene",
        b"lin_ciexyzd65_scene",
        b"srgb_rec709_scene",
        b"g22_rec709_scene",
        b"g18_rec709_scene",
        b"srgb_ap1_scene",
        b"g22_ap1_scene",
        b"srgb_p3d65_scene",
        b"g22_adobergb_scene",
        b"data",
        b"unknown",
        b"srgb_rec709_display",
        b"g24_rec709_display",
        b"srgb_p3d65_display",
        b"srgbe_p3d65_display",
        b"pq_p3d65_display",
        b"pq_rec2020_display",
        b"hlg_rec2020_display",
        b"g22_rec709_display",
        b"g22_adobergb_display",
        b"g26_p3d65_display",
        b"g26_xyzd65_display",
        b"pq_xyzd65_display",
    ];
    IDS.contains(&id)
}

/// Returns true if the interopID is valid.
///
/// Port of `isValidInteropID` (src/apps/ociocheck/main.cpp:32-105 @ v2.5.2).
fn is_valid_interop_id(id: &[u8]) -> bool {
    // See https://github.com/AcademySoftwareFoundation/ColorInterop for the details.

    if id.is_empty() {
        return true;
    }

    // Check if the ID has a namespace.
    match id.iter().position(|&c| c == b':') {
        None => {
            // No namespace, so the ID must be in the Color Interop Forum ID list.
            if !is_color_interop_forum_id(id) {
                cout(b"WARNING: InteropID '");
                cout(id);
                cout_line(
                    b"' is not valid. It should either be one of the Color Interop Forum \
                      standard IDs or it must contain a namespace followed by ':', e.g. \
                      'mycompany:mycolorspace'.",
                );
                return false;
            }
        }
        Some(pos) => {
            // Namespace found, split into namespace and ID. The ID should not be in the Color
            // Interop Forum ID list.
            if is_color_interop_forum_id(&id[pos + 1..]) {
                cout(b"WARNING: InteropID '");
                cout(id);
                cout_line(
                    b"' is not valid. The ID part must not be one of the Color Interop Forum \
                      standard IDs when a namespace is used.",
                );
                return false;
            }
        }
    }

    // All clear.
    true
}

/// `OCIO::ConstProcessorRcPtr p = config->getProcessor(t)` for a transform that may be null,
/// inside upstream's `try` with `catch(OCIO::Exception & exception)`: the error text of what
/// that catches. Any other C++ exception goes on to `main`'s `catch (std::exception &)`.
fn try_load(config: &Config, transform: Option<&Transform>) -> Result<Option<Vec<u8>>, Exception> {
    let Some(transform) = transform else {
        return Ok(None);
    };
    match config.processor(transform) {
        Ok(_) => Ok(None),
        Err(exception) if is_ocio_exception(&exception) => Ok(Some(exception.what().to_vec())),
        Err(exception) => Err(exception),
    }
}

/// Prints what loading an item's two directions gave, and counts an error.
///
/// Port of the end of each loop of the color spaces, named transforms and looks
/// (src/apps/ociocheck/main.cpp:433-453, 523-543, 602-622 @ v2.5.2).
fn print_loaded(
    name: &[u8],
    first: Option<Vec<u8>>,
    second: Option<Vec<u8>>,
    errorcount: &mut i32,
) {
    if first.is_some() || second.is_some() {
        // There was a problem with one of the item's transforms.
        cout(name);
        cout_line(b" -- error");
        for error in [first, second].into_iter().flatten() {
            cout(b"\t");
            cout_line(error);
        }
        *errorcount += 1;
    } else {
        // The item's transforms load ok.
        cout_line(name);
    }
}

/// `getProcessor` for one (display, view) pair.
///
/// Port of the body of the loops over the views (src/apps/ociocheck/main.cpp:250-266, 274-290
/// @ v2.5.2).
fn check_view(
    display_test_config: &Config,
    src_color_space: &[u8],
    display_name: &[u8],
    view_name: &[u8],
    errorcount: &mut i32,
) -> Result<(), Exception> {
    let process = display_test_config.processor_for_display_view(
        src_color_space,
        display_name,
        view_name,
        TransformDirection::Forward,
    );
    match process {
        Ok(_) => {
            cout(b"(");
            cout(display_name);
            cout(b", ");
            cout(view_name);
            cout_line(b")");
        }
        Err(exception) if is_ocio_exception(&exception) => {
            cout(b"ERROR: ");
            cout_line(exception.what());
            *errorcount += 1;
        }
        Err(exception) => return Err(exception),
    }
    Ok(())
}

/// The part of `main` that upstream runs in its outer `try` block: `Ok` holds the exit code
/// of an early return, or nothing to go on.
///
/// Port of `main` (src/apps/ociocheck/main.cpp:141-681 @ v2.5.2). `GetEnvVariable` never gives
/// a null pointer, so the branch that asks for a config is dead (docs/improvements.md, I-602),
/// and isn't ported.
fn check(
    inputconfig: &[u8],
    outputconfig: &[u8],
    errorcount: &mut i32,
    warningcount: &mut i32,
) -> Result<(), Exception> {
    cout_endl();
    cout_line(format!("OpenColorIO Library Version: {}", ocio::version()));
    cout_line(format!(
        "OpenColorIO Library VersionHex: {}",
        ocio::version_hex()
    ));

    let src_config: Arc<Config> = if !inputconfig.is_empty() {
        cout_endl();
        cout(b"Loading ");
        cout_line(inputconfig);
        Config::create_from_file(inputconfig)?
    } else {
        cout_endl();
        cout(b"Loading $OCIO ");
        cout_line(get_env_variable("OCIO"));
        Config::create_from_env()?
    };

    // This program calls getProcessor for every color space and display/view in the config,
    // so turn off the Processor cache.
    let config: Config = (*src_config).clone();
    config.set_processor_cache_flags(ProcessorCacheFlags::OFF);

    cout_endl();
    cout_line(b"** General **");

    if config.num_environment_vars() > 0 {
        cout_line(b"Environment:");
        for idx in 0..config.num_environment_vars() {
            let name = config.environment_var_name_by_index(idx);
            cout(b"  ");
            cout(name);
            cout(b": ");
            cout_line(config.environment_var_default(name));
        }
    } else if config.environment_mode() == EnvironmentMode::LoadPredefined {
        cout_line(b"Environment: {}");
    } else {
        cout_line(b"Environment: <missing>");
    }

    cout(b"Search Path: ");
    cout_line(config.search_path());
    cout(b"Working Dir: ");
    cout_line(config.working_dir());

    if config.num_displays() == 0 {
        cout_endl();
        cout_line(b"ERROR: At least one (display, view) pair must be defined.");
        *errorcount += 1;
    } else {
        cout_endl();
        cout(b"Default Display: ");
        cout_line(config.default_display());
        cout(b"Default View: ");
        cout_line(config.default_view(config.default_display()));

        // It's important that the getProcessor call below always loads the transforms
        // involved in each display/view pair.  However, if the src color space is a
        // data space or if the view's colorspace happens to be the same as the src
        // color space, it will effectively bypass the loading of the transform.
        // The work-around used here is to create a copy of the config and add a unique
        // src color space that will definitely allow the Processor to be created.

        let mut display_test_config = config.clone();
        let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Scene);
        let src_color_space = b"ocioCheckTotallyUniqueColorSpaceName";
        cs.set_name(src_color_space);
        let ff = FixedFunctionTransform::new(FixedFunctionStyle::AcesGlow10, &[])?;
        cs.set_transform(
            Some(&Transform::FixedFunction(ff)),
            ColorSpaceDirection::ToReference,
        )?;
        display_test_config.add_color_space(&cs)?;

        if config.num_color_spaces() > 0 {
            cout_endl();
            cout_line(b"** (Display, View) pairs **");

            // Iterate over all displays & views (active & inactive).

            for idx_disp in 0..config.num_displays_all() {
                let display_name = config.display_all(idx_disp);

                // Iterate over shared views, then over display-defined views.
                for view_type in [ViewType::Shared, ViewType::DisplayDefined] {
                    let num_views = config.num_views_of_type(view_type, display_name);
                    for idx_view in 0..num_views {
                        let view_name = config.view_of_type(view_type, display_name, idx_view);
                        check_view(
                            &display_test_config,
                            src_color_space,
                            display_name,
                            view_name,
                            errorcount,
                        )?;
                    }
                }
            }
        }
    }

    {
        cout_endl();
        cout_line(b"** Roles **");

        // All roles defined in OpenColorTypes.h.
        let all_roles = [
            ROLE_DEFAULT,
            ROLE_SCENE_LINEAR,
            ROLE_DATA,
            ROLE_REFERENCE,
            ROLE_COMPOSITING_LOG,
            ROLE_COLOR_TIMING,
            ROLE_COLOR_PICKING,
            ROLE_TEXTURE_PAINT,
            ROLE_MATTE_PAINT,
            ROLE_RENDERING,
            ROLE_INTERCHANGE_SCENE,
            ROLE_INTERCHANGE_DISPLAY,
        ];

        // Print a list of the config's roles, appending ": user" if they are not one
        // of the "standard" roles defined in the library.
        for i in 0..config.num_roles() {
            let role = config.role_name(i);
            if let Some(cs) = config.color_space(role) {
                cout(cs.name());
                cout(b" (");
                cout(role);
                if all_roles.iter().any(|r| r.as_bytes() == role) {
                    cout_line(b")");
                } else {
                    cout_line(b": user)");
                }
            } else {
                // Note: The config->validate check below will also fail due to this.
                cout(b"ERROR: SPACE MISSING (");
                cout(role);
                cout_line(b")");
                *errorcount += 1;
            }
        }

        // Roles that are actually used by the library or important tools/plug-ins.
        //   Config::validate ensures these are present for 2.2 and later configs,
        //   but want to warn if they are missing in earlier configs.  No warnings
        //   are given for other roles since most are not widely used anymore.
        let essential_roles = [
            ROLE_SCENE_LINEAR,        // LegacyViewingPipeline
            ROLE_COLOR_TIMING,        // LegacyViewingPipeline
            ROLE_COMPOSITING_LOG,     // LogConvert plug-in
            ROLE_INTERCHANGE_SCENE,   // Used by the library
            ROLE_INTERCHANGE_DISPLAY, // Used by the library
        ];

        // Print a warning for any essential roles that are missing.
        // (Subsequent sections may raise an error.)
        for role in essential_roles {
            if config.color_space(role).is_none() {
                cout(b"WARNING: NOT DEFINED (");
                cout(role);
                cout_line(b")");
                *warningcount += 1;
            }
        }
    }

    {
        cout_endl();
        cout_line(b"** ColorSpaces **");

        let num_cs = config.num_color_spaces_with(
            SearchReferenceSpaceType::All, // Iterate over scene & display color spaces.
            ColorSpaceVisibility::All,     // Iterate over active & inactive color spaces.
        );

        let mut found_category = false;
        let mut found_no_category = false;

        for i in 0..num_cs {
            let name = config.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::All,
                i,
            );
            let cs = config
                .color_space(name)
                .expect("a name of the config's own list");

            let interop_id = cs.interop_id();
            if !interop_id.is_empty() && !is_valid_interop_id(interop_id) {
                *warningcount += 1;
            }

            if !config.is_inactive_color_space(cs.name()) {
                if cs.num_categories() > 0 {
                    found_category = true;
                } else {
                    found_no_category = true;
                }
            }

            // Try to load the transform for the to_ref direction -- this will load any LUTs.
            let to_ref = try_load(&config, cs.transform(ColorSpaceDirection::ToReference))?;

            // Try to load the transform for the from_ref direction -- this will load any LUTs.
            let from_ref = try_load(&config, cs.transform(ColorSpaceDirection::FromReference))?;

            print_loaded(cs.name(), to_ref, from_ref, errorcount);
        }

        if found_category && found_no_category {
            // Categories should either be missing in all, or present in all active items.
            cout(
                b"\nWARNING: The config has some color spaces \
                  where the categories are not set.\n",
            );
            *warningcount += 1;
        }
    }

    {
        cout_endl();
        cout_line(b"** Named Transforms **");

        // Iterate over active & inactive named transforms.
        let num_nt = config.num_named_transforms_with(NamedTransformVisibility::All);
        if num_nt == 0 {
            cout_line(b"no named transforms defined");
        }

        let mut found_category = false;
        let mut found_no_category = false;

        for i in 0..num_nt {
            let name = config.named_transform_name_by_index_with(NamedTransformVisibility::All, i);
            let nt = config
                .named_transform(name)
                .expect("a name of the config's own list");

            if !config.is_inactive_color_space(nt.name()) {
                if nt.num_categories() > 0 {
                    found_category = true;
                } else {
                    found_no_category = true;
                }
            }

            // Try to load the transform -- this will load any LUTs.
            let fwd = try_load(&config, nt.transform(TransformDirection::Forward))?;

            // Try to load the inverse_transform -- this will load any LUTs.
            let inv = try_load(&config, nt.transform(TransformDirection::Inverse))?;

            print_loaded(nt.name(), fwd, inv, errorcount);
        }

        if found_category && found_no_category {
            // Categories should either be missing in all, or present in all active items.
            cout(
                b"\nWARNING: The config has some named transforms \
                  where the categories are not set.\n",
            );
            *warningcount += 1;
        }
    }

    {
        cout_endl();
        cout_line(b"** Looks **");

        let num_l = config.num_looks();
        if num_l == 0 {
            cout_line(b"no looks defined");
        }

        for i in 0..num_l {
            let look = config
                .look(config.look_name_by_index(i))
                .expect("a name of the config's own list");

            // Try to load the transform -- this will load any LUTs.
            let fwd = try_load(&config, look.transform())?;

            // Try to load the inverse transform -- this will load any LUTs.
            let inv = try_load(&config, look.inverse_transform())?;

            print_loaded(look.name(), fwd, inv, errorcount);
        }
    }

    cout_endl();
    cout_line(b"** Validation **");

    let mut cache_id = String::new();
    let mut is_archivable = false;
    let validation = (|| -> Result<(), Exception> {
        let log_guard = LogGuard::new()?;

        config.validate()?;
        cout(log_guard.output());

        cache_id = config.cache_id()?;
        is_archivable = config.is_archivable();

        // Passed if there are no Error level logs.
        let svec = split_by_lines(&log_guard.output());
        if !contain(&svec, b"[OpenColorIO Error]") {
            cout_line(b"Validation: passed");
        } else {
            cout_line(b"Validation: failed");
            *errorcount += 1;
        }
        Ok(())
    })();
    match validation {
        Ok(()) => {}
        Err(exception) if is_ocio_exception(&exception) => {
            cout_line(b"ERROR:");
            *errorcount += 1;
            cout_line(exception.what());
            cout_line(b"Validation: failed");
        }
        Err(exception) => return Err(exception),
    }

    cout_endl();
    cout_line(b"** Miscellaneous **");
    cout(b"CacheID: ");
    cout_line(cache_id);
    cout(b"Archivable: ");
    cout_line(if is_archivable { "yes" } else { "no" });

    if !outputconfig.is_empty() {
        match OutputFile::create(outputconfig) {
            Err(_) => {
                cout(b"Error opening ");
                cout(outputconfig);
                cout_line(b" for writing.");
            }
            Ok(mut output) => {
                use std::io::Write;
                // A failed write is ignored, as a C++ stream keeps it in its state bits.
                let _ = output.write_all(&config.serialize()?);
                cout(b"Wrote ");
                cout_line(outputconfig);
            }
        }
    }

    Ok(())
}

/// Port of `main` (src/apps/ociocheck/main.cpp:107-711 @ v2.5.2).
fn run(argv: &[Vec<u8>]) -> i32 {
    let help = Cell::new(false);
    let inputconfig = RefCell::new(Vec::new());
    let outputconfig = RefCell::new(Vec::new());

    let mut ap = ArgParse::new(
        "ociocheck -- validate an OpenColorIO configuration\n\n\
         usage:  ociocheck [options]\n",
    );
    ap.option("--help", &[Param::Flag(&help)], "Print help message");
    ap.option(
        "--iconfig %s",
        &[Param::Text(&inputconfig)],
        "Input .ocio configuration file (default: $OCIO)",
    );
    ap.option(
        "--oconfig %s",
        &[Param::Text(&outputconfig)],
        "Output .ocio file",
    );

    if ap.parse(argv) < 0 {
        cout_line(ap.geterror());
        ap.usage();
        cout(DESC_STRING);
        return 1;
    }

    if help.get() {
        ap.usage();
        cout(DESC_STRING);
        return 1;
    }

    // Set the logging level to INFO.
    set_logging_level(LoggingLevel::Info);

    let mut errorcount = 0;
    let mut warningcount = 0;
    if let Err(exception) = check(
        &inputconfig.borrow(),
        &outputconfig.borrow(),
        &mut errorcount,
        &mut warningcount,
    ) {
        cout(b"ERROR: ");
        cout(exception.what());
        if is_ocio_exception(&exception) {
            cout_endl();
        } else {
            cout(b"\n");
        }
        return 1;
    }

    if warningcount > 0 {
        cout(format!("\nWarnings encountered: {warningcount}"));
        cout_endl();
    }

    cout_endl();
    if errorcount == 0 {
        cout_line(b"Tests complete.");
        cout_endl();
        0
    } else {
        cout(format!("{errorcount} tests failed."));
        cout_endl();
        cout_endl();
        1
    }
}

fn main() {
    stdio::init();
    stdio::exit(run(&stdio::args()))
}
