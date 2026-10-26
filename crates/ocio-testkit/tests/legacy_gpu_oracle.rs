// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Contracts for O7.1's wheel-linked native oracle. Expected outputs come from the pinned
//! wheel's ordinary public GPU API, with legacy-specific invariants from Processor.cpp:
//! 447-525 and NoOps.cpp:175-206 @ v2.5.2. No port processor is an expectation.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

/// Every supported transform's native copy must give the wheel's ordinary shader, cache IDs,
/// uniform values, texture bytes, dynamic properties and log. Includes exact doubles, nested
/// transforms, file/allocation ops, every shading language and non-idempotent resource names.
#[test]
fn native_ordinary_gpu_matches_the_python_wheel() {
    let script = r#"
import json, pathlib, tempfile
import PyOpenColorIO as OCIO
from ocio_oracle import commands, gpu, legacy_gpu, native_build

root = pathlib.Path.cwd().parent
file = str(root / "upstream/OpenColorIO/tests/data/files/lut1d_1.spi1d")
transforms = [
 {"class":"AllocationTransform", "args":{"allocation":{"enum":"ALLOCATION_LG2"}, "vars":[-10.0,6.0]}},
 {"class":"BuiltinTransform", "args":{"style":list(OCIO.BuiltinTransformRegistry().getBuiltins())[0][0]}},
 {"class":"CDLTransform", "args":{"slope":[1.1,0.9,1.2]}},
 {"class":"ColorSpaceTransform", "args":{"src":"raw","dst":"raw"}},
 {"class":"DisplayViewTransform", "args":{"src":"raw","display":"sRGB","view":"Raw"}},
 {"class":"ExponentTransform", "args":{"value":[2.2,2.1,2.0,1.0]}},
 {"class":"ExponentWithLinearTransform", "args":{"gamma":[2.2,2.2,2.2,1.0],"offset":[0.1,0.1,0.1,0.0]}},
 {"class":"ExposureContrastTransform", "calls":[["setExposure",0.5],["makeExposureDynamic"]]},
 {"class":"FileTransform", "args":{"src":file}},
 {"class":"FixedFunctionTransform", "args":{"style":{"enum":"FIXED_FUNCTION_RGB_TO_HSV"}}},
 {"class":"GradingPrimaryTransform", "calls":[["makeDynamic"]]},
 {"class":"GradingToneTransform", "calls":[["makeDynamic"]]},
 {"class":"GradingRGBCurveTransform", "calls":[["makeDynamic"]]},
 {"class":"GradingHueCurveTransform", "calls":[["makeDynamic"]]},
 {"class":"GroupTransform", "children":[{"class":"AllocationTransform"},{"class":"MatrixTransform","args":{"offset":[0.1,0.2,0.3,0.0]}}]},
 {"class":"LogAffineTransform", "args":{"logSideSlope":[1.1,1.2,1.3]}, "calls":[["setBase",10.0]]},
 {"class":"LogCameraTransform", "args":{"linSideBreak":[0.01,0.02,0.03]}},
 {"class":"LogTransform", "args":{"base":10.0}},
 {"class":"LookTransform", "args":{"src":"raw","dst":"raw","looks":""}},
 {"class":"Lut1DTransform", "args":{"length":3}, "calls":[["setValue",1,0.2,0.3,0.4]]},
 {"class":"Lut3DTransform", "args":{"gridSize":2}, "calls":[["setValue",1,1,1,0.8,0.9,0.7]]},
 {"class":"MatrixTransform", "args":{"offset":[1.0000000000000002,-0.0,1.2345678901234567,0.0]}},
 {"class":"RangeTransform", "args":{"minInValue":0.0,"maxInValue":1.0,"minOutValue":0.1,"maxOutValue":0.9}},
]
program, provenance = native_build.executable()
seen = []
def prove(args, success=True):
    expected, blobs = gpu.gpu_shader(args,[])
    if success:
        assert "exception" not in expected, (args,expected)
    actual, actual_blobs, abi = legacy_gpu._native(program,legacy_gpu._message(args,[],[{"kind":"default","optimization":args.get("optimization")}]))
    assert actual == expected and actual_blobs == blobs, (args,actual,expected)
    return expected, abi
for transform in transforms:
    args = {"transform":transform,"optimization":"OPTIMIZATION_NONE"}
    expected, abi = prove(args)
    seen.append(transform["class"])
for language in OCIO.GpuLanguage.__members__:
    args = {"transform":transforms[-2],"shader":{"language":language,"resource_prefix":"a________b","uid":"native-id"}}
    expected, abi = prove(args)

# Nondefault grading values exercise value/uniform transport; default dynamic curves alone
# would not distinguish dropped authored knots/slopes. Every value is a public wheel input.
def obj(name, args=None, attrs=None, calls=None):
    return {"object":{"class":name,"args":args or [],"attrs":attrs or [],"calls":calls or []}}
rgb = obj("GradingRGBM",[0.1,0.2,0.3,0.4])
primary = obj("GradingPrimary",[{"enum":"GRADING_LIN"}],[["exposure",rgb],["saturation",0.75]])
tone = obj("GradingTone",[{"enum":"GRADING_LOG"}],[["scontrast",1.125]])
curve = obj("GradingBSplineCurve",[[-0.5,-0.4,0.0,0.1,1.0,0.9]],calls=[["setSlopes",[0.75,1.0,0.5]]])
rgbcurve = obj("GradingRGBCurve",[{"enum":"GRADING_LOG"}],[["red",curve]])
huepoints = obj("GradingBSplineCurve",[[0.0,0.8,0.5,1.2,1.0,1.0],{"enum":"LUM_SAT"}],calls=[["setSlopes",[0.75,1.0,0.5]]])
huecurve = obj("GradingHueCurve",[{"enum":"GRADING_LOG"}],[["lum_sat",huepoints]])
for name,value,style in [("GradingPrimaryTransform",primary,"GRADING_LIN"),
                         ("GradingToneTransform",tone,"GRADING_LOG"),
                         ("GradingRGBCurveTransform",rgbcurve,"GRADING_LOG"),
                         ("GradingHueCurveTransform",huecurve,"GRADING_LOG")]:
    args={"transform":{"class":name,"args":{"style":{"enum":style}},"calls":[["setValue",value],["makeDynamic"]]},
          "optimization":"OPTIMIZATION_NONE"}
    expected,abi=prove(args)
    assert expected["shader"]["dynamic_properties"],(name,expected)

# Exercise exact config-source transport, src/dst requests and actual error/log responses.
raw=OCIO.Config.CreateRaw()
yaml=raw.serialize()
with tempfile.TemporaryDirectory(prefix="ocio-native-config-") as directory:
    configfile=pathlib.Path(directory)/"config.ocio"
    configfile.write_text(yaml,encoding="utf-8")
    for config in [{"yaml":yaml},{"file":str(configfile)},{"builtin":"ocio://cg-config-latest"}]:
        expected,abi=prove({"config":config,"transform":transforms[-2]})
expected,abi=prove({"src":"raw","dst":"raw"})
for args in [{"config":{"create":{}},"src":"raw","dst":"raw"},
             {"transform":{"class":"FileTransform","args":{"src":str(root/"target/no-such-native-file.spi1d")}}}]:
    expected,abi=prove(args,False)
    assert expected.get("stage")=="processor" and "exception" in expected,expected
print(json.dumps({"classes":seen,"languages":len(OCIO.GpuLanguage.__members__),"abi":abi,"source_key":provenance["source_key"],
                  "nondefault_dynamic_values":4,"config_sources":3,"error_responses":2}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("native proof report");
    let classes = report["classes"].as_array().expect("classes tested");
    assert_eq!(classes.len(), 23);
    let unique: std::collections::BTreeSet<_> =
        classes.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(unique.len(), classes.len());
    assert!(report["languages"].as_u64().unwrap() >= 10);
    assert_eq!(report["abi"]["wheel_version"], json!("2.5.2"));
    assert_eq!(report["source_key"].as_str().unwrap().len(), 64);
    assert_eq!(report["nondefault_dynamic_values"], json!(4));
    assert_eq!(report["config_sources"], json!(3));
    assert_eq!(report["error_responses"], json!(2));
}

/// A LUT forces a real legacy bake. Different edges reach the library, earlier default/legacy
/// calls share its flags-only cache, and unbaked controls equal the independent Python GPU API.
#[test]
fn legacy_requests_reach_the_library_and_share_its_cache() {
    let script = r#"
import json
from ocio_oracle import commands, gpu, legacy_gpu

transform={"class":"Lut1DTransform","args":{"length":3},"calls":[["setValue",1,0.2,0.3,0.4]]}
args={"config":{"create":{}},"transform":transform,"edge_len":2}
two,two_blobs=legacy_gpu.legacy_gpu_shader(args,[])
three,three_blobs=legacy_gpu.legacy_gpu_shader(dict(args,edge_len=3),[])
assert "exception" not in two and "exception" not in three,(two,three)
assert len(two["shader"]["textures_3d"])==1 and len(three["shader"]["textures_3d"])==1
assert two["shader"]["textures_3d"][0]["edge_len"]==2
assert three["shader"]["textures_3d"][0]["edge_len"]==3
assert len(two_blobs[two["shader"]["textures_3d"][0]["values"]])==2**3*3*4
assert len(three_blobs[three["shader"]["textures_3d"][0]["values"]])==3**3*3*4
assert two["gpu_cache_id"]!=three["gpu_cache_id"]
first,first_blobs=legacy_gpu.legacy_gpu_shader(dict(args,edge_len=3,before=[{"kind":"legacy","edge_len":2}]),[])
first.pop("oracle_native")
two.pop("oracle_native")
assert first==two and first_blobs==two_blobs
cached,cached_blobs=legacy_gpu.legacy_gpu_shader(dict(args,before=[{"kind":"default"}]),[])
cached.pop("oracle_native")
ordinary,ordinary_blobs=gpu.gpu_shader({k:v for k,v in args.items() if k!="edge_len"},[])
assert cached==ordinary and cached_blobs==ordinary_blobs
assert len(cached["shader"]["textures"])==1 and cached["shader"]["textures_3d"]==[]
uncached,uncached_blobs=legacy_gpu.legacy_gpu_shader(dict(args,config={"create":{"processor_cache":"off"}},before=[{"kind":"default"}]),[])
assert "exception" not in uncached and len(uncached["shader"]["textures_3d"])==1,uncached
assert uncached["shader"]["textures_3d"][0]["edge_len"]==2
# With caches off, earlier successful requests cannot leak their GPU response fields when
# the final legacy request throws; its complete error response equals the standalone call.
failed,failed_blobs=legacy_gpu.legacy_gpu_shader(dict(args,config={"create":{"processor_cache":"off"}},edge_len=130),[])
after,after_blobs=legacy_gpu.legacy_gpu_shader(dict(args,config={"create":{"processor_cache":"off"}},edge_len=130,before=[{"kind":"default"}]),[])
failed.pop("oracle_native")
after.pop("oracle_native")
assert "exception" in failed and failed.get("stage")=="gpu_processor",failed
assert failed==after and failed_blobs==after_blobs,(failed,after)
print(json.dumps({"edges":[2,3],"cache_shared":True,"ordinary_matches":True}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("legacy cache report");
    assert_eq!(
        report,
        json!({"edges":[2,3],"cache_shared":true,"ordinary_matches":true})
    );
}

/// Constructor size errors precede allocation. Edge 0/1 probes run in capped children and
/// report their actual outcome; a non-LUT control never constructs a legacy lattice.
#[test]
fn native_legacy_boundary_probes_are_bounded() {
    let script = r#"
import json
from ocio_oracle import commands, legacy_gpu
lut={"class":"Lut1DTransform","args":{"length":3},"calls":[["setValue",1,0.2,0.3,0.4]]}
out=[]
for edge in [0,1,130,1626,4294967295]:
    result,blobs=legacy_gpu.legacy_gpu_shader({"transform":lut,"edge_len":edge},[])
    if edge>129:
        assert result.get("stage")=="gpu_processor" and result.get("exception",{}).get("type")=="Exception", result
        assert not blobs
    else:
        assert "crashed" in result or "exception" in result or "shader" in result,result
    out.append({"edge":edge,"exception":result.get("exception"),"stage":result.get("stage"),"crashed":result.get("crashed"),"has_shader":"shader" in result,
                "result":result,"blobs":[blob.hex() for blob in blobs]})
control,blobs=legacy_gpu.legacy_gpu_shader({"transform":{"class":"MatrixTransform"},"edge_len":4294967295},[])
assert "shader" in control and "exception" not in control,control
print(json.dumps({"probes":out,"analytic_control":True}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("bounded boundary report");
    let probes = report["probes"]
        .as_array()
        .expect("all five probes reported");
    assert_eq!(probes.len(), 5);
    assert_eq!(report["analytic_control"], json!(true));
    for (probe, edge) in probes.iter().zip([0_u64, 1, 130, 1626, u32::MAX as u64]) {
        assert_eq!(probe["edge"], json!(edge));
    }
}

/// A transport mismatch must refuse legacy output before the second native invocation.
/// Inject faults into actual ordinary responses so the proof guard itself is nonvacuous.
#[test]
fn legacy_output_requires_the_complete_ordinary_proof() {
    let script = r#"
import copy,json
from ocio_oracle import commands,legacy_gpu
args={"transform":{"class":"MatrixTransform","args":{"offset":[0.25,0.0,0.0,0.0]}},"edge_len":2}
real=legacy_gpu._native
refused=[]
for fault in ["field","blob","log","error"]:
    calls=[]
    def broken(program,data):
        calls.append(True)
        result,blobs,abi=real(program,data)
        result=copy.deepcopy(result)
        blobs=list(blobs)
        if fault=="field":
            result["gpu_cache_id"]="injected-transport-mismatch"
        elif fault=="blob":
            blobs[0]+=b"injected-transport-mismatch"
        elif fault=="log":
            result["log"].append("injected-transport-mismatch")
        else:
            result["exception"]={"type":"Exception","message":"injected-transport-mismatch"}
            result["stage"]="extract"
        return result,blobs,abi
    legacy_gpu._native=broken
    try:
        legacy_gpu.legacy_gpu_shader(args,[])
    except ValueError as error:
        assert "native ordinary GPU transport does not match gpu_shader" in str(error),error
        refused.append(fault)
    else:
        raise AssertionError(("legacy output accepted an ordinary proof mismatch",fault))
    finally:
        legacy_gpu._native=real
    assert len(calls)==1,(fault,calls)
print(json.dumps({"refused":refused,"legacy_invocations":0}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("proof refusal report");
    assert_eq!(
        report,
        json!({"refused":["field","blob","log","error"],"legacy_invocations":0})
    );
}

/// An explicit processor optimization override reaches both sides of the proof and the
/// final legacy call. The inverse matrix pair makes ignoring that override observable.
#[test]
fn explicit_processor_environment_reaches_the_native_calls() {
    let script = r#"
import json,os
from ocio_oracle import commands,child_env,gpu,legacy_gpu
transform={"class":"GroupTransform","children":[
    {"class":"MatrixTransform","args":{"offset":[0.25,0.5,0.0,0.0]}},
    {"class":"MatrixTransform","args":{"offset":[0.25,0.5,0.0,0.0]},"calls":[["setDirection",{"enum":"TRANSFORM_DIR_INVERSE"}]]}
]}
args={"config":{"create":{"processor_cache":"off"}},"transform":transform}
names=["OCIO_OPTIMIZATION_FLAGS","OCIO_RS_PRIVATE_PROBE"]
saved={name:os.environ.get(name) for name in names}
real=legacy_gpu.subprocess.run
children=[]
def observed(*args,**kwargs):
    environment=kwargs["env"]
    assert environment.get("OCIO_OPTIMIZATION_FLAGS")=="0"
    assert "OCIO_RS_PRIVATE_PROBE" not in environment
    assert set(environment)-set(child_env.ALLOWED)<={"OCIO_OPTIMIZATION_FLAGS","OCIO_DISABLE_ALL_CACHES",
        "OCIO_DISABLE_PROCESSOR_CACHES","OCIO_DISABLE_CACHE_FALLBACK"},sorted(environment)
    children.append(True)
    return real(*args,**kwargs)
try:
    os.environ.pop("OCIO_OPTIMIZATION_FLAGS",None)
    baseline,baseline_blobs=gpu.gpu_shader(args,[])
    assert baseline["gpu_processor"]["isNoOp"],baseline
    os.environ["OCIO_OPTIMIZATION_FLAGS"]="0"
    os.environ["OCIO_RS_PRIVATE_PROBE"]="must-stay-in-this-process"
    expected,expected_blobs=gpu.gpu_shader(args,[])
    assert "exception" not in expected and not expected["gpu_processor"]["isNoOp"],expected
    assert expected["gpu_cache_id"]!=baseline["gpu_cache_id"]
    assert expected_blobs!=baseline_blobs
    # Build before spying: a cold helper cache also starts explicitly isolated compiler
    # children, whose build environment deliberately carries no OCIO processor overrides.
    legacy_gpu.native_build.executable()
    legacy_gpu.subprocess.run=observed
    actual,actual_blobs=legacy_gpu.legacy_gpu_shader(dict(args,edge_len=2),[])
    actual.pop("oracle_native")
    assert actual==expected and actual_blobs==expected_blobs,(actual,expected)
    assert len(children)==2,children
finally:
    legacy_gpu.subprocess.run=real
    for name,value in saved.items():
        if value is None:
            os.environ.pop(name,None)
        else:
            os.environ[name]=value
print(json.dumps({"override_changes_shader":True,"ordinary_and_legacy_match":True,"native_calls":len(children),"private_value_forwarded":False}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value =
        serde_json::from_str(&lines[0]).expect("explicit processor environment report");
    assert_eq!(
        report,
        json!({"override_changes_shader":true,"ordinary_and_legacy_match":true,"native_calls":2,"private_value_forwarded":false})
    );
}
