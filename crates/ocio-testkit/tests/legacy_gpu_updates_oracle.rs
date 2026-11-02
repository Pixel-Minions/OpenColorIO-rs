// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! P87 native-client contracts. Both sides call the official OCIO 2.5.2 library;
//! no Rust SDK output supplies an expectation. The script reads the same live
//! descriptor before and after setters (OpenColorIO.h:2707-2709, 3476-3483;
//! OpenColorTransforms.h:781-898 @ v2.5.2).

use ocio_testkit::Oracle;
use serde_json::Value;
use std::sync::{Mutex, PoisonError};

// Bound native compiler/Python memory and avoid overlapping first helper builds.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn native_post_extraction_states_match_direct_wheel_getters() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let lines = Oracle::get().run_script(include_str!("scripts/legacy_gpu_updates.py"), &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("direct-wheel contract report");
    let properties = report["property_types"]
        .as_array()
        .expect("input property types");
    assert_eq!(properties.len(), 7);
    assert_eq!(report["languages"], 10);
    assert_eq!(report["ordinary_comparisons"], properties.len() * 10 * 2);
    let compared = report["ordinary_comparisons"].as_u64().unwrap();
    assert_eq!(
        report["successful"].as_u64().unwrap() + report["exact_refusals"].as_u64().unwrap(),
        compared
    );
    assert!(report["successful"].as_u64().unwrap() > 0);
    assert_eq!(report["missing_property_prefix"], true);
    assert_eq!(report["legacy_live_update"], true);
    assert_eq!(report["extended_comparisons"], 40);
    assert_eq!(report["source_key"].as_str().unwrap().len(), 64);
}

#[test]
fn native_update_protocol_refuses_changed_initial_states_and_missing_steps() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let script = r#"
import copy, json
from ocio_oracle import commands, legacy_gpu_updates as updates
args={"transform":{"class":"ExposureContrastTransform","calls":[["makeExposureDynamic"]]},
      "edge_len":2,"optimization":"OPTIMIZATION_NONE","updates":[
        {"from":"shader","property":"DYNAMIC_PROPERTY_EXPOSURE",
         "value_transform":{"class":"ExposureContrastTransform","calls":[["setExposure",1.25]]}}]}
real=updates._native
count=0
for mutation in ["no-initial","drop-step","duplicate-step","text","uniform","texture-blob", "failed-duplicate", "failed-complete", "failed-stage"]:
    def changed(*a):
        result,blobs,abi=real(*a)
        result=copy.deepcopy(result); blobs=list(blobs)
        if mutation=="no-initial": result["states"]=[]
        elif mutation=="drop-step": result["states"].pop()
        elif mutation=="duplicate-step": result["states"].append(copy.deepcopy(result["states"][-1]))
        elif mutation=="text": result["states"][0]["shader"]["function_name"]="foreign-initial-function"
        elif mutation=="uniform": result["states"][0]["shader"]["uniforms"][0]["value"]^=1
        elif mutation=="texture-blob": blobs[0]+=b"foreign bytes"
        else:
            result["exception"]={"type":"Exception","message":"injected setter error"}
            result["stage"]="update"
            if mutation=="failed-duplicate": result["states"].append(copy.deepcopy(result["states"][-1]))
            elif mutation=="failed-stage":
                result["states"].pop(); result["stage"]="extract"
        return result,blobs,abi
    updates._native=changed
    try:
        updates.legacy_gpu_updates(args,[])
        raise AssertionError("accepted "+mutation)
    except ValueError as error:
        assert "native dynamic" in str(error),(mutation,error)
        count+=1
    finally: updates._native=real
# Required keys fail before native execution; extra values cannot silently alter scope.
for bad in [{}, {"from":"shader"}, dict(args["updates"][0],extra=True)]:
    try:
        updates.legacy_gpu_updates(dict(args,updates=[bad]),[])
        raise AssertionError("accepted malformed update")
    except ValueError: count+=1
print(json.dumps({"rejections":count}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(report["rejections"], 12);
}
