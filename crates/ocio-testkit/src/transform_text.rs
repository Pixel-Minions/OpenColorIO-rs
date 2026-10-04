// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Typed requests and replies for the oracle's `transform_text` command
//! (`oracle/ocio_oracle/transform_text.py`, chunk O1.4): what transforms say about themselves,
//! `str()` and `repr()` (upstream's `operator<<`), what `validate()` raises, and what
//! `equals()` returns for pairs of them where the binding exposes it.

use serde_json::{Value, json};

use crate::Oracle;
use crate::oracle::{BatchCall, Response};

/// A double in a transform spec, by its bits (`{"f64": bits}`, `oracle/ocio_oracle/spec.py`):
/// JSON can't hold NaN or the infinities (serde_json writes them as `null`), so a spec passes
/// every special value this way, and any other exactly.
pub fn f64_spec(value: f64) -> Value {
    json!({"f64": value.to_bits()})
}

/// One `transform_text` call.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformTextRequest {
    /// Transform specs, each built on its own.
    pub transforms: Vec<Value>,
    /// Pairs `(i, j)` for `transforms[i].equals(transforms[j])`.
    pub pairs: Vec<(usize, usize)>,
}

impl TransformTextRequest {
    /// The command's arguments.
    pub fn args(&self) -> Value {
        let pairs: Vec<[usize; 2]> = self.pairs.iter().map(|&(i, j)| [i, j]).collect();
        json!({"transforms": self.transforms, "pairs": pairs})
    }

    /// The request as a call of an [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        BatchCall {
            cmd: "transform_text",
            args: self.args(),
            blobs: Vec::new(),
        }
    }

    /// Runs the request alone; panics if the oracle fails (a refusal included).
    pub fn run(&self) -> TransformTextReply {
        TransformTextReply::from_response(Oracle::get().call("transform_text", self.args(), &[]))
    }
}

/// An exception OCIO raised: its Python type and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    /// The Python type: `Exception` for OCIO's.
    pub kind: String,
    /// The message.
    pub message: String,
}

impl Exception {
    fn parse(value: &Value) -> Exception {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        Exception {
            kind: text("type"),
            message: text("message"),
        }
    }
}

/// What a built transform says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformText {
    /// Its Python class.
    pub class: String,
    /// `repr()`.
    pub repr: String,
    /// `str()`.
    pub str: String,
    /// What `validate()` raised, if anything.
    pub validate: Option<Exception>,
}

/// One transform spec's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Built {
    /// The transform was built.
    Text(TransformText),
    /// Building it raised (the binding's constructors validate some transforms).
    Raised(Exception),
}

impl Built {
    /// The text; panics if building raised.
    pub fn text(&self) -> &TransformText {
        match self {
            Built::Text(text) => text,
            Built::Raised(e) => panic!("not built: {e:?}"),
        }
    }
}

/// The wheel's answer to a [`TransformTextRequest`].
#[derive(Debug, Clone, PartialEq)]
pub struct TransformTextReply {
    /// The command's result.
    pub result: Value,
    /// Per spec, in order.
    pub transforms: Vec<Built>,
    /// Per pair, in order: what `equals()` returned, or `None` where the binding has none
    /// for the pair or a transform wasn't built.
    pub pairs: Vec<Option<bool>>,
}

impl TransformTextReply {
    /// Reads the command's response.
    pub fn from_response(response: Response) -> TransformTextReply {
        let list = |key: &str| {
            response.result[key]
                .as_array()
                .cloned()
                .unwrap_or_else(|| panic!("{key} in {}", response.result))
        };
        let transforms = list("transforms")
            .iter()
            .map(|t| match t.get("exception") {
                Some(exception) => Built::Raised(Exception::parse(exception)),
                None => Built::Text(TransformText {
                    class: t["class"].as_str().expect("a class").to_string(),
                    repr: t["repr"].as_str().expect("a repr").to_string(),
                    str: t["str"].as_str().expect("a str").to_string(),
                    validate: match &t["validate"] {
                        Value::Null => None,
                        raised => Some(Exception::parse(raised)),
                    },
                }),
            })
            .collect();
        let pairs = list("pairs")
            .iter()
            .map(|p| p["equals"].as_bool())
            .collect();
        TransformTextReply {
            result: response.result,
            transforms,
            pairs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request's arguments and a reply's reading: built transforms with their texts and
    /// validation, unbuilt ones with their exception, pairs with their equality.
    #[test]
    fn requests_and_replies() {
        let request = TransformTextRequest {
            transforms: vec![json!({"class": "LogTransform"})],
            pairs: vec![(0, 0)],
        };
        assert_eq!(
            request.args(),
            json!({"transforms": [{"class": "LogTransform"}], "pairs": [[0, 0]]})
        );
        let result = json!({
            "transforms": [
                {"class": "LogTransform", "repr": "r", "str": "s", "validate": null},
                {"class": "RangeTransform", "repr": "r", "str": "s",
                 "validate": {"type": "Exception", "message": "m"}},
                {"exception": {"type": "Exception", "message": "e"}, "stage": "transform"},
            ],
            "pairs": [{"equals": true}, {"equals": false}, {"equals": null}],
            "log": [],
        });
        let reply = TransformTextReply::from_response(Response {
            result,
            blobs: Vec::new(),
        });
        assert_eq!(reply.transforms[0].text().repr, "r");
        assert_eq!(reply.transforms[0].text().str, "s");
        assert_eq!(reply.transforms[0].text().validate, None);
        assert_eq!(
            reply.transforms[1].text().validate,
            Some(Exception {
                kind: "Exception".into(),
                message: "m".into()
            })
        );
        assert_eq!(
            reply.transforms[2],
            Built::Raised(Exception {
                kind: "Exception".into(),
                message: "e".into()
            })
        );
        assert_eq!(reply.pairs, vec![Some(true), Some(false), None]);
    }
}
