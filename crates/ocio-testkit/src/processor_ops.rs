// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Typed requests and replies for the oracle's `processor_ops` command
//! (`oracle/ocio_oracle/processor_ops.py`, chunk O1.1): what `getOptimizedProcessor(in, out,
//! flags)` makes of a processor, as `createGroupTransform()` writes it out.
//!
//! Each transform comes back as a [`Dump`]: its class, the value of every getter that takes no
//! argument, its properties, the getters that need arguments, and for a group its children.
//! Values are [`Dumped`]: floats as their bits, enums by name, arrays with their bytes.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::Oracle;
use crate::battery::BitDepth;
use crate::oracle::{BatchCall, Response, bytes_to_f32};

/// One `processor_ops` call.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessorOpsRequest {
    /// The processor: `config`, `transform` or `src`/`dst`, `direction`.
    pub processor: Value,
    /// `getOptimizedProcessor`'s input bit depth (default F32).
    pub in_bitdepth: Option<BitDepth>,
    /// Its output bit depth (default F32).
    pub out_bitdepth: Option<BitDepth>,
    /// Its flags: a name, a list of names or an integer (default `OPTIMIZATION_DEFAULT`).
    pub optimization: Option<Value>,
}

impl ProcessorOpsRequest {
    /// A request for `processor` with the default bit depths and flags.
    pub fn new(processor: Value) -> ProcessorOpsRequest {
        ProcessorOpsRequest {
            processor,
            in_bitdepth: None,
            out_bitdepth: None,
            optimization: None,
        }
    }

    /// The command's arguments.
    pub fn args(&self) -> Value {
        let mut args = self.processor.clone();
        if let Some(depth) = self.in_bitdepth {
            args["in_bitdepth"] = json!(depth.oracle_name());
        }
        if let Some(depth) = self.out_bitdepth {
            args["out_bitdepth"] = json!(depth.oracle_name());
        }
        if let Some(flags) = &self.optimization {
            args["optimization"] = flags.clone();
        }
        args
    }

    /// The request as a call of an [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        BatchCall {
            cmd: "processor_ops",
            args: self.args(),
            blobs: Vec::new(),
        }
    }

    /// Runs the request alone; panics if the oracle fails (a refusal included).
    pub fn run(&self) -> ProcessorOpsReply {
        ProcessorOpsReply::from_response(Oracle::get().call("processor_ops", self.args(), &[]))
    }
}

/// A value a getter or property returned. Floats compare by their bits, so a NaN equals the
/// same NaN (a LogCameraTransform's unset linear slope is NaN).
#[derive(Debug, Clone)]
pub enum Dumped {
    /// `None`.
    Null,
    /// A bool.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A string.
    Str(String),
    /// A float: a C `double`, or a C `float` the binding widened.
    F64(f64),
    /// An enum value, by name.
    Enum(String),
    /// A numpy array: its dtype name, shape and bytes (little-endian, C order).
    Array {
        /// The numpy dtype name, such as `float32`.
        dtype: String,
        /// The shape.
        shape: Vec<u64>,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// A list, tuple or iterator.
    List(Vec<Dumped>),
    /// An object, written out.
    Object(Dump),
    /// An object too deep to write out: its class.
    ClassOnly(String),
}

impl PartialEq for Dumped {
    fn eq(&self, other: &Dumped) -> bool {
        match (self, other) {
            (Dumped::Null, Dumped::Null) => true,
            (Dumped::Bool(a), Dumped::Bool(b)) => a == b,
            (Dumped::Int(a), Dumped::Int(b)) => a == b,
            (Dumped::Str(a), Dumped::Str(b)) | (Dumped::Enum(a), Dumped::Enum(b)) => a == b,
            (Dumped::ClassOnly(a), Dumped::ClassOnly(b)) => a == b,
            (Dumped::F64(a), Dumped::F64(b)) => a.to_bits() == b.to_bits(),
            (
                Dumped::Array {
                    dtype: a,
                    shape: s,
                    bytes: x,
                },
                Dumped::Array {
                    dtype: b,
                    shape: t,
                    bytes: y,
                },
            ) => a == b && s == t && x == y,
            (Dumped::List(a), Dumped::List(b)) => a == b,
            (Dumped::Object(a), Dumped::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl Dumped {
    fn parse(value: &Value, blobs: &[Vec<u8>]) -> Dumped {
        match value {
            Value::Null => Dumped::Null,
            Value::Bool(b) => Dumped::Bool(*b),
            Value::Number(n) => {
                Dumped::Int(n.as_i64().unwrap_or_else(|| panic!("an integer: {n}")))
            }
            Value::String(s) => Dumped::Str(s.clone()),
            Value::Array(items) => {
                Dumped::List(items.iter().map(|v| Dumped::parse(v, blobs)).collect())
            }
            Value::Object(object) => {
                if let Some(bits) = object.get("f64") {
                    Dumped::F64(f64::from_bits(bits.as_u64().expect("f64 bits")))
                } else if let Some(name) = object.get("enum") {
                    Dumped::Enum(name.as_str().expect("an enum name").to_string())
                } else if let Some(blob) = object.get("blob") {
                    Dumped::Array {
                        dtype: object["dtype"].as_str().expect("a dtype").to_string(),
                        shape: object["shape"]
                            .as_array()
                            .expect("a shape")
                            .iter()
                            .map(|n| n.as_u64().expect("a dimension"))
                            .collect(),
                        bytes: blobs[blob.as_u64().expect("a blob index") as usize].clone(),
                    }
                } else if object.contains_key("getters") {
                    Dumped::Object(Dump::parse(object, blobs))
                } else {
                    Dumped::ClassOnly(object["class"].as_str().expect("a class").to_string())
                }
            }
        }
    }

    /// The float; panics for any other value.
    pub fn f64(&self) -> f64 {
        match self {
            Dumped::F64(v) => *v,
            other => panic!("not a float: {other:?}"),
        }
    }

    /// The list of floats; panics for any other value.
    pub fn f64s(&self) -> Vec<f64> {
        match self {
            Dumped::List(items) => items.iter().map(Dumped::f64).collect(),
            other => panic!("not a list of floats: {other:?}"),
        }
    }

    /// The `float32` array's values; panics for any other value.
    pub fn f32s(&self) -> Vec<f32> {
        match self {
            Dumped::Array { dtype, bytes, .. } if dtype == "float32" => bytes_to_f32(bytes),
            other => panic!("not a float32 array: {other:?}"),
        }
    }

    /// The enum's name; panics for any other value.
    pub fn name(&self) -> &str {
        match self {
            Dumped::Enum(name) => name,
            other => panic!("not an enum: {other:?}"),
        }
    }

    /// The object; panics for any other value.
    pub fn object(&self) -> &Dump {
        match self {
            Dumped::Object(dump) => dump,
            other => panic!("not an object: {other:?}"),
        }
    }
}

/// An object written out: a transform, or a value a getter returned.
#[derive(Debug, Clone, PartialEq)]
pub struct Dump {
    /// Its Python class.
    pub class: String,
    /// Each method named `get*`, `is*` or `has*` that takes no argument, called.
    pub getters: BTreeMap<String, Dumped>,
    /// Each property.
    pub properties: BTreeMap<String, Dumped>,
    /// The `get*`, `is*` and `has*` methods that need arguments.
    pub uncalled: Vec<String>,
    /// A group's transforms, in order.
    pub children: Vec<Dump>,
}

impl Dump {
    fn parse(object: &Map<String, Value>, blobs: &[Vec<u8>]) -> Dump {
        let map = |key: &str| -> BTreeMap<String, Dumped> {
            object
                .get(key)
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), Dumped::parse(v, blobs)))
                        .collect()
                })
                .unwrap_or_default()
        };
        Dump {
            class: object["class"].as_str().expect("a class").to_string(),
            getters: map("getters"),
            properties: map("properties"),
            uncalled: object
                .get("uncalled")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|n| n.as_str().expect("a name").to_string())
                        .collect()
                })
                .unwrap_or_default(),
            children: object
                .get("children")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|c| Dump::parse(c.as_object().expect("a transform"), blobs))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The value of getter `name`; panics if the dump has none.
    pub fn getter(&self, name: &str) -> &Dumped {
        self.getters
            .get(name)
            .unwrap_or_else(|| panic!("{} has no getter {name}", self.class))
    }

    /// The value of property `name`; panics if the dump has none.
    pub fn property(&self, name: &str) -> &Dumped {
        self.properties
            .get(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.class))
    }
}

/// A processor, written out.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessorDump {
    /// `getCacheID()`.
    pub cache_id: String,
    /// `createGroupTransform()`.
    pub group: Dump,
}

impl ProcessorDump {
    fn parse(value: &Value, blobs: &[Vec<u8>]) -> ProcessorDump {
        ProcessorDump {
            cache_id: value["cache_id"].as_str().expect("a cache ID").to_string(),
            group: Dump::parse(value["group"].as_object().expect("a group"), blobs),
        }
    }

    /// The classes of the group's transforms, in order.
    pub fn classes(&self) -> Vec<&str> {
        self.group
            .children
            .iter()
            .map(|c| c.class.as_str())
            .collect()
    }
}

/// What OCIO raised, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raised {
    /// The Python type: `Exception` for OCIO's.
    pub kind: String,
    /// The message.
    pub message: String,
    /// `config`, `transform`, `processor`, `group`, `optimize` or `optimized_group`.
    pub stage: String,
}

/// The wheel's answer to a [`ProcessorOpsRequest`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessorOpsReply {
    /// The command's result.
    pub result: Value,
    /// The processor, when it was built.
    pub processor: Option<ProcessorDump>,
    /// The optimized processor, when it was built.
    pub optimized: Option<ProcessorDump>,
}

impl ProcessorOpsReply {
    /// Reads the command's response.
    pub fn from_response(response: Response) -> ProcessorOpsReply {
        let dump = |key: &str| {
            response
                .result
                .get(key)
                .map(|v| ProcessorDump::parse(v, &response.blobs))
        };
        ProcessorOpsReply {
            processor: dump("processor"),
            optimized: dump("optimized"),
            result: response.result,
        }
    }

    /// What OCIO raised, if anything.
    pub fn raised(&self) -> Option<Raised> {
        let exception = self.result.get("exception")?;
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        Some(Raised {
            kind: text(&exception["type"]),
            message: text(&exception["message"]),
            stage: text(&self.result["stage"]),
        })
    }

    /// The optimized processor; panics with the result if there is none.
    pub fn optimized(&self) -> &ProcessorDump {
        self.optimized
            .as_ref()
            .unwrap_or_else(|| panic!("no optimized processor: {}", self.result))
    }

    /// The processor; panics with the result if there is none.
    pub fn processor(&self) -> &ProcessorDump {
        self.processor
            .as_ref()
            .unwrap_or_else(|| panic!("no processor: {}", self.result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request's arguments: the processor keys, then the depths and flags given.
    #[test]
    fn requests_become_the_commands_arguments() {
        let mut request = ProcessorOpsRequest::new(json!({"transform": {"class": "LogTransform"}}));
        assert_eq!(
            request.args(),
            json!({"transform": {"class": "LogTransform"}})
        );
        request.in_bitdepth = Some(BitDepth::Uint8);
        request.optimization = Some(json!(["OPTIMIZATION_IDENTITY"]));
        assert_eq!(
            request.args(),
            json!({"transform": {"class": "LogTransform"}, "in_bitdepth": "BIT_DEPTH_UINT8",
                "optimization": ["OPTIMIZATION_IDENTITY"]})
        );
    }

    /// Dumps read every kind of value: floats from their bits, arrays from their blobs,
    /// objects with their getters, properties and children.
    #[test]
    fn dumps_read_every_kind_of_value() {
        let result = json!({
            "processor": {"cache_id": "p", "group": {"class": "GroupTransform",
                "getters": {}, "properties": {}, "uncalled": [], "children": []}},
            "optimized": {"cache_id": "o", "group": {"class": "GroupTransform",
                "getters": {"getDirection": {"enum": "TRANSFORM_DIR_FORWARD"}},
                "properties": {}, "uncalled": [],
                "children": [{"class": "Lut1DTransform",
                    "getters": {"getData": {"blob": 0, "dtype": "float32", "shape": [2]},
                                "getLength": 2, "getInputHalfDomain": false,
                                "getOffset": [{"f64": 0x3FF0_0000_0000_0000u64}],
                                "getValue": {"class": "GradingRGBM", "getters": {},
                                    "properties": {"red": {"f64": 0}}, "uncalled": []},
                                "getName": "n", "getMissing": null, "getDeep": {"class": "X"}},
                    "properties": {}, "uncalled": ["getValue"]}]}},
            "log": [],
        });
        let blobs = vec![[0.5f32, -1.0].map(f32::to_le_bytes).concat()];
        let reply = ProcessorOpsReply::from_response(Response { result, blobs });
        assert_eq!(reply.processor().cache_id, "p");
        let optimized = reply.optimized();
        assert_eq!(optimized.classes(), vec!["Lut1DTransform"]);
        assert_eq!(
            optimized.group.getter("getDirection").name(),
            "TRANSFORM_DIR_FORWARD"
        );
        let lut = &optimized.group.children[0];
        assert_eq!(lut.getter("getData").f32s(), vec![0.5, -1.0]);
        assert_eq!(lut.getter("getLength"), &Dumped::Int(2));
        assert_eq!(lut.getter("getInputHalfDomain"), &Dumped::Bool(false));
        assert_eq!(lut.getter("getOffset").f64s(), vec![1.0]);
        assert_eq!(
            lut.getter("getValue")
                .object()
                .property("red")
                .f64()
                .to_bits(),
            0
        );
        assert_eq!(lut.getter("getName"), &Dumped::Str("n".into()));
        assert_eq!(lut.getter("getMissing"), &Dumped::Null);
        assert_eq!(lut.getter("getDeep"), &Dumped::ClassOnly("X".into()));
        assert_eq!(lut.uncalled, vec!["getValue"]);
        assert!(reply.raised().is_none());
        // Floats compare by their bits.
        assert_eq!(Dumped::F64(f64::NAN), Dumped::F64(f64::NAN));
        assert_ne!(Dumped::F64(0.0), Dumped::F64(-0.0));
        let array = |bytes: Vec<u8>| Dumped::Array {
            dtype: "float32".into(),
            shape: vec![1],
            bytes,
        };
        assert_ne!(array(vec![0, 0, 0, 0]), array(vec![0, 0, 0, 1]));
    }
}
