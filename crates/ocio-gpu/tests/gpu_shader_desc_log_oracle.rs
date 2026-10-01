// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The shader a finalize logs at the debug level, against the wheel's log. The logging state
//! is global, so this is the only test of its process.

use std::sync::{Arc, Mutex, PoisonError};

use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::logging::{
    get_logging_level, reset_to_default_logging_function, set_logging_function, set_logging_level,
};
use ocio_ops::open_color_types::LoggingLevel;
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::GpuLanguage as OracleLanguage;
use ocio_testkit::gpu_desc::{DescCall, GpuShaderDescRequest};

use DescCall::*;

const PARAMETERS: &str = "float p;\n\n  \n";
const HELPERS: &str = "float h() { return 1.; }   \n";
const HEADER: &str = "float4 OCIOMain(float4 inPixel)\n{\n";
const FOOTER: &str = "  return inPixel;\n}\n";

/// At the debug level, each finalize logs the shader program, line by line, in every language
/// (the OSL and Metal class wrappers' text included), as the wheel logs it. A finalize that
/// raises (an MSL class name starting with a digit) logs nothing, and neither does
/// `createShaderText`.
#[test]
fn finalize_logs_the_shader_at_the_debug_level() {
    // OCIO reads OCIO_LOGGING_LEVEL once: read it in an empty environment, as the oracle's
    // process does, so the developer's doesn't count.
    platform::set_env_provider(Some(Arc::new(MapEnv::default())));
    let _ = get_logging_level();
    platform::set_env_provider(None);

    let messages: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |line: &[u8]| {
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8(line.to_vec()).expect("UTF-8"));
    })))
    .unwrap();
    set_logging_level(LoggingLevel::Debug);

    for (language, oracle_language) in GpuLanguage::ALL.into_iter().zip(OracleLanguage::ALL) {
        let calls = vec![
            SetLanguage(oracle_language),
            AddToParameterDeclareShaderCode(PARAMETERS.into()),
            AddToHelperShaderCode(HELPERS.into()),
            AddToFunctionHeaderShaderCode(HEADER.into()),
            AddToFunctionFooterShaderCode(FOOTER.into()),
            SetResourcePrefix("9".into()),
            Finalize,
            SetResourcePrefix("ocio".into()),
            Finalize,
            CreateShaderText([
                "a".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
                "".into(),
            ]),
        ];
        let wheel = GpuShaderDescRequest::new(calls).with_debug_log().run();

        messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let mut desc = GpuShaderDesc::new(language);
        desc.add_to_parameter_declare_shader_code(PARAMETERS);
        desc.add_to_helper_shader_code(HELPERS);
        desc.add_to_function_header_shader_code(HEADER);
        desc.add_to_function_footer_shader_code(FOOTER);
        desc.set_resource_prefix("9");
        let _ = desc.finalize();
        desc.set_resource_prefix("ocio");
        desc.finalize().unwrap();
        desc.create_shader_text("a", "", "", "", "", "");
        let port = messages
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        assert_eq!(port, wheel.log, "{language:?}");
        assert!(!port.is_empty(), "{language:?}");
    }

    reset_to_default_logging_function();
    set_logging_level(LoggingLevel::Info);
}
