// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the baker's oracle tests share: a case is a config, metadata calls and setter calls,
//! made on the wheel's `Baker` (oracle `bake`) and on the port's; the getters after the
//! setters, then the baked text byte for byte (or the exception and where it was raised), and
//! the log, must agree.

use std::sync::{Arc, Mutex};

use ocio::{Baker, Config};
use ocio_ops::logging::{LoggingFunction, reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values;
use serde_json::{Value, json};

/// The logging function is the process's: the tests that capture the log hold this.
pub(crate) static LOGGING: Mutex<()> = Mutex::new(());

/// A call on the baker, as the wheel's binding and the port name it.
#[derive(Debug, Clone)]
pub(crate) enum Call {
    Format(&'static str),
    InputSpace(&'static str),
    ShaperSpace(&'static str),
    Looks(&'static str),
    TargetSpace(&'static str),
    DisplayView(&'static str, &'static str),
    ShaperSize(i32),
    CubeSize(i32),
}

/// A call on the baker's format metadata.
#[derive(Debug, Clone)]
pub(crate) enum Metadata {
    /// `addChildElement(name, value)`.
    Child(&'static str, &'static str),
    /// `setName(name)`.
    Name(&'static str),
    /// `setID(id)`.
    Id(&'static str),
}

/// A bake: the config's YAML (`None`: the raw config), or no config at all, then the metadata
/// calls and the setters, in order.
#[derive(Debug, Clone)]
pub(crate) struct BakeCase {
    pub label: String,
    pub yaml: Option<String>,
    /// The baker never gets a config (`setConfig` isn't called).
    pub no_config: bool,
    pub metadata: Vec<Metadata>,
    pub calls: Vec<Call>,
}

impl BakeCase {
    pub(crate) fn new(label: &str, yaml: Option<&str>, calls: &[Call]) -> BakeCase {
        BakeCase {
            label: label.to_owned(),
            yaml: yaml.map(str::to_owned),
            no_config: false,
            metadata: Vec::new(),
            calls: calls.to_vec(),
        }
    }

    /// A bake on a baker that never gets a config.
    #[allow(dead_code)] // Only baker_oracle.rs bakes without a config.
    pub(crate) fn without_config(label: &str, calls: &[Call]) -> BakeCase {
        BakeCase {
            no_config: true,
            ..BakeCase::new(label, None, calls)
        }
    }

    pub(crate) fn with_metadata(mut self, metadata: &[Metadata]) -> BakeCase {
        self.metadata = metadata.to_vec();
        self
    }

    /// The oracle's request.
    fn request(&self) -> Value {
        let metadata: Vec<Value> = self
            .metadata
            .iter()
            .map(|m| match m {
                Metadata::Child(name, value) => json!(["addChildElement", name, value]),
                Metadata::Name(name) => json!(["setName", name]),
                Metadata::Id(id) => json!(["setID", id]),
            })
            .collect();
        let calls: Vec<Value> = self
            .calls
            .iter()
            .map(|c| match c {
                Call::Format(v) => json!(["setFormat", v]),
                Call::InputSpace(v) => json!(["setInputSpace", v]),
                Call::ShaperSpace(v) => json!(["setShaperSpace", v]),
                Call::Looks(v) => json!(["setLooks", v]),
                Call::TargetSpace(v) => json!(["setTargetSpace", v]),
                Call::DisplayView(d, v) => json!(["setDisplayView", d, v]),
                Call::ShaperSize(n) => json!(["setShaperSize", n]),
                Call::CubeSize(n) => json!(["setCubeSize", n]),
            })
            .collect();
        let config = match &self.yaml {
            Some(yaml) => json!({ "yaml": yaml }),
            None => Value::Null,
        };
        if self.no_config {
            return json!({ "no_config": true, "metadata": metadata, "calls": calls });
        }
        json!({ "config": config, "metadata": metadata, "calls": calls })
    }
}

/// What the port did: the stage of its error, or the getters and the bake's text.
struct PortBake {
    getters: Option<Value>,
    text: Vec<u8>,
    error: Option<(&'static str, Vec<u8>)>,
}

/// The port's bake of `case`.
fn port_bake(case: &BakeCase) -> PortBake {
    let mut out = PortBake {
        getters: None,
        text: Vec::new(),
        error: None,
    };
    let mut baker = Baker::new();
    if !case.no_config {
        let config = match &case.yaml {
            Some(yaml) => Config::create_from_stream(yaml.as_bytes()),
            None => Config::create_raw(),
        };
        let config = match config {
            Ok(config) => config,
            Err(e) => {
                out.error = Some(("config", e.what().to_vec()));
                return out;
            }
        };
        baker.set_config(&config);
    }
    for m in &case.metadata {
        let md = baker.format_metadata_mut();
        let result = match m {
            Metadata::Child(name, value) => {
                md.add_child_element(Some(name.as_bytes()), Some(value.as_bytes()))
            }
            Metadata::Name(name) => {
                md.set_name(Some(name.as_bytes()));
                Ok(())
            }
            Metadata::Id(id) => {
                md.set_id(Some(id.as_bytes()));
                Ok(())
            }
        };
        if let Err(e) = result {
            out.error = Some(("metadata", e.what().to_vec()));
            return out;
        }
    }
    for c in &case.calls {
        let result = match *c {
            Call::Format(v) => baker.set_format(v),
            Call::InputSpace(v) => {
                baker.set_input_space(v);
                Ok(())
            }
            Call::ShaperSpace(v) => {
                baker.set_shaper_space(v);
                Ok(())
            }
            Call::Looks(v) => {
                baker.set_looks(v);
                Ok(())
            }
            Call::TargetSpace(v) => {
                baker.set_target_space(v);
                Ok(())
            }
            Call::DisplayView(d, v) => {
                baker.set_display_view(d, v);
                Ok(())
            }
            Call::ShaperSize(n) => {
                baker.set_shaper_size(n);
                Ok(())
            }
            Call::CubeSize(n) => {
                baker.set_cube_size(n);
                Ok(())
            }
        };
        if let Err(e) = result {
            out.error = Some(("setters", e.what().to_vec()));
            return out;
        }
    }
    let b = |v: &[u8]| oracle_values::bytes_arg(v);
    out.getters = Some(json!({
        "getFormat": b(baker.format()),
        "getInputSpace": b(baker.input_space()),
        "getShaperSpace": b(baker.shaper_space()),
        "getLooks": b(baker.looks()),
        "getTargetSpace": b(baker.target_space()),
        "getDisplay": b(baker.display()),
        "getView": b(baker.view()),
        "getShaperSize": baker.shaper_size(),
        "getCubeSize": baker.cube_size(),
    }));
    if let Err(e) = baker.bake(&mut out.text) {
        out.error = Some(("bake", e.what().to_vec()));
    }
    out
}

/// `text` as a text-mode `std::ofstream` writes it: each line feed as CR LF on Windows.
fn text_mode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in text {
        if cfg!(target_os = "windows") && b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

/// Each case in the wheel against the port: the getters, the text or the exception and its
/// stage, and the log (at the default logging level, this thread's messages).
pub(crate) fn check_bakes(cases: &[BakeCase]) {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|c| BatchCall {
            cmd: "bake",
            args: c.request(),
            blobs: Vec::new(),
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);

    let me = std::thread::current().id();
    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        if std::thread::current().id() == me {
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(m).into_owned());
        } else {
            eprint!("{}", String::from_utf8_lossy(m));
        }
    });

    let mut failures = Vec::new();
    for (case, reply) in cases.iter().zip(replies) {
        let reply = reply.unwrap_or_else(|e| panic!("{}: {e}", case.label));
        let wheel = &reply.result;

        messages.lock().unwrap().clear();
        set_logging_function(Some(function.clone())).unwrap();
        let port = port_bake(case);
        reset_to_default_logging_function();
        let port_log = messages.lock().unwrap().clone();

        let wheel_log: Vec<String> = wheel["log"]
            .as_array()
            .expect("the log")
            .iter()
            .map(|m| m.as_str().expect("a message").to_owned())
            .collect();
        if wheel_log != port_log {
            failures.push(format!(
                "{}: log\n  wheel {wheel_log:?}\n  port  {port_log:?}",
                case.label
            ));
        }

        if let Some(getters) = wheel.get("getters")
            && Some(getters) != port.getters.as_ref()
        {
            failures.push(format!(
                "{}: getters\n  wheel {getters}\n  port  {:?}",
                case.label, port.getters
            ));
        }
        match (wheel.get("exception"), &port.error) {
            (Some(exception), Some((stage, message))) => {
                // What `bake(fileName)` left in its file: what the bake wrote before it failed,
                // in text mode (each line feed a CR LF on Windows).
                if let Some(partial) = wheel.get("partial_file") {
                    let left = match partial["bytes"].as_str() {
                        Some(hex) => oracle_values::bytes(&json!({ "bytes": hex })),
                        None => Vec::new(),
                    };
                    if left != text_mode(&port.text) {
                        failures.push(format!(
                            "{}: partial file\n  wheel {:?}\n  port  {:?}",
                            case.label,
                            String::from_utf8_lossy(&left),
                            String::from_utf8_lossy(&port.text)
                        ));
                    }
                }
                let wheel_message = exception["message"].as_str().expect("a message");
                if wheel["stage"] != *stage || wheel_message.as_bytes() != message.as_slice() {
                    failures.push(format!(
                        "{}: wheel {} at {}\n  port  {:?} at {stage}",
                        case.label,
                        wheel_message,
                        wheel["stage"],
                        String::from_utf8_lossy(message)
                    ));
                }
            }
            (None, None) => {
                let text = oracle_values::bytes(&wheel["text"]);
                if text != port.text {
                    failures.push(format!(
                        "{}:\n  wheel {:?}\n  port  {:?}",
                        case.label,
                        String::from_utf8_lossy(&text),
                        String::from_utf8_lossy(&port.text)
                    ));
                }
            }
            (wheel_exception, port_error) => failures.push(format!(
                "{}: wheel {wheel_exception:?}, port {:?}",
                case.label,
                port_error
                    .as_ref()
                    .map(|(s, m)| format!("{} at {s}", String::from_utf8_lossy(m)))
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
