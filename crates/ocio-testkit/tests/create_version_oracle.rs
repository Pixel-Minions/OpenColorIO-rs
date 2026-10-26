// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! O7.1 follow-up: the exact Config::Create/setMajorVersion sequence used by
//! GammaOp_test.cpp:20-37 and CDLOp_test.cpp:26-61 @ v2.5.2. Expectations are direct wheel calls.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

#[test]
fn create_major_version_matches_the_wheel_setter_and_order() {
    let script = r#"
import json,types
import PyOpenColorIO as OCIO
from ocio_oracle import spec

def report(config):
    transform=OCIO.MatrixTransform(offset=[0.25,0.0,0.0,0.0])
    first=config.getProcessor(transform)
    second=config.getProcessor(transform)
    return {"version":[config.getMajorVersion(),config.getMinorVersion()],
            "serialize":config.serialize(),"spaces":list(config.getColorSpaceNames()),
            "same_processor":first is second,"processor_id":first.getCacheID()}

versions=[]
for major in [1,2]:
    for off in [False,True]:
        create={"major_version":major}
        direct=OCIO.Config()
        direct.setMajorVersion(major)
        if off:
            create["processor_cache"]="off"
            direct.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
        actual=report(spec.config({"create":create}))
        expected=report(direct)
        assert actual==expected,(major,off,actual,expected)
        assert actual["same_processor"] is (not off)
        assert actual["spaces"]==[]
        versions.append(actual["version"])
assert versions[0]!=versions[2],versions

# Even setting the current major must invoke the real setter, before cache flags.
# Spy only on the requested API calls; the wrapped Config is still the actual wheel.
real_module=spec.OCIO
events=[]
class ObservedConfig:
    def __init__(self):
        self.value=OCIO.Config()
        events.append("create")
    def setMajorVersion(self,major):
        events.append(["major",major])
        self.value.setMajorVersion(major)
    def setProcessorCacheFlags(self,flags):
        events.append(["cache",int(flags)])
        self.value.setProcessorCacheFlags(flags)
try:
    spec.OCIO=types.SimpleNamespace(Config=ObservedConfig,PROCESSOR_CACHE_OFF=OCIO.PROCESSOR_CACHE_OFF)
    for major in [1,2]:
        events.clear()
        observed=spec.config({"create":{"major_version":major,"processor_cache":"off"}})
        assert events==["create",["major",major],["cache",int(OCIO.PROCESSOR_CACHE_OFF)]],events
        direct=OCIO.Config()
        direct.setMajorVersion(major)
        direct.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
        assert report(observed.value)==report(direct)
finally:
    spec.OCIO=real_module
print(json.dumps({"versions":versions,"ordered_setters":2}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("direct wheel version report");
    assert_eq!(report["versions"].as_array().unwrap().len(), 4);
    assert_ne!(report["versions"][0], report["versions"][2]);
    assert_eq!(report["ordered_setters"], json!(2));
}

#[test]
fn create_major_version_preserves_library_errors_and_refuses_non_uints() {
    let script = r#"
import json
import PyOpenColorIO as OCIO
from ocio_oracle import commands,gpu,legacy_gpu,processor_ops,spec

invalid=[None,True,False,-1,4294967296,1.0,"1",[],{}]
for value in invalid:
    try:
        spec.config({"create":{"major_version":value}})
    except ValueError as error:
        assert "major_version must be an integer from 0 to 4294967295" in str(error),error
    else:
        raise AssertionError(("accepted non-uint major",value))
try:
    spec.config({"create":{"major_version":1,"minor_version":0}})
except ValueError:
    pass
else:
    raise AssertionError("unrequested minor-version option accepted")

errors=[]
for major in [0,3,4294967295]:
    log=[]
    OCIO.SetLoggingFunction(log.append)
    try:
        direct=OCIO.Config()
        direct.setMajorVersion(major)
    except OCIO.Exception as error:
        expected={"exception":{"type":type(error).__name__,"message":str(error)},
                  "stage":"config","log":log}
    else:
        raise AssertionError(("unexpectedly supported major",major))
    finally:
        OCIO.ResetToDefaultLoggingFunction()
    try:
        spec.config({"create":{"major_version":major}})
    except OCIO.Exception as error:
        assert {"type":type(error).__name__,"message":str(error)}==expected["exception"]
    else:
        raise AssertionError(("ignored rejected major",major))
    args={"config":{"create":{"major_version":major,"processor_cache":"off"}},
          "transform":{"class":"MatrixTransform"}}
    for function in [commands.cpu_apply,processor_ops.processor_ops,gpu.gpu_shader,legacy_gpu.legacy_gpu_shader]:
        actual,blobs=function(args,[])
        assert actual==expected and not blobs,(major,function.__name__,actual,expected)
    errors.append(expected["exception"])
print(json.dumps({"errors":errors,"invalid_types":len(invalid),"consumers":4}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("direct wheel version errors");
    assert_eq!(report["errors"].as_array().unwrap().len(), 3);
    assert_eq!(report["invalid_types"], json!(9));
    assert_eq!(report["consumers"], json!(4));
}

#[test]
fn versioned_exponent_and_cdl_native_shaders_match_direct_wheel_processors() {
    let script = r#"
import json,struct
import PyOpenColorIO as OCIO
from ocio_oracle import commands,gpu,legacy_gpu,native_build,native_transport

# Additive tag5 leaves the existing raw/Create layouts untouched.
for value,kind,off in [(None,0,0),({"create":{}},1,0),({"create":{"processor_cache":"off"}},1,1)]:
    writer=native_transport.Writer()
    legacy_gpu._config(writer,value)
    assert bytes(writer.data)==struct.pack("<III",kind,0,off)
for major in [1,2]:
    for off in [False,True]:
        create={"major_version":major}
        if off:
            create["processor_cache"]="off"
        writer=native_transport.Writer()
        legacy_gpu._config(writer,{"create":create})
        assert bytes(writer.data)==struct.pack("<IIII",5,0,off,major)

program,provenance=native_build.executable()
seen=[]
versions={1:{},2:{}}
for major in [1,2]:
    for inverse in [False,True]:
        direction=OCIO.TRANSFORM_DIR_INVERSE if inverse else OCIO.TRANSFORM_DIR_FORWARD
        for name in ["ExponentTransform","CDLTransform"]:
            if name=="ExponentTransform":
                transform=OCIO.ExponentTransform()
                transform.setNegativeStyle(OCIO.NEGATIVE_CLAMP)
                transform.setDirection(direction)
                transform.setValue([2.6,1.0,1.8,1.1])
                transform_spec={"class":name,"calls":[["setNegativeStyle",{"enum":"NEGATIVE_CLAMP"}],
                    ["setDirection",{"enum":direction.name}],["setValue",[2.6,1.0,1.8,1.1]]]}
            else:
                transform=OCIO.CDLTransform()
                transform.setDirection(direction)
                transform.setSlope([1.35,1.10,0.71])
                transform.setOffset([0.05,-0.23,0.11])
                transform.setPower([0.93,0.81,1.27])
                transform_spec={"class":name,"calls":[["setDirection",{"enum":direction.name}],
                    ["setSlope",[1.35,1.10,0.71]],["setOffset",[0.05,-0.23,0.11]],
                    ["setPower",[0.93,0.81,1.27]]]}
            for off in [False,True]:
                with commands.captured_log() as log:
                    config=OCIO.Config()
                    config.setMajorVersion(major)
                    if off:
                        config.setProcessorCacheFlags(OCIO.PROCESSOR_CACHE_OFF)
                    processor=config.getProcessor(transform)
                    direct=processor.getDefaultGPUProcessor()
                    desc=OCIO.GpuShaderDesc.CreateShaderDesc()
                    direct.extractGpuShaderInfo(desc)
                    expected_blobs=[]
                    expected={"processor_cache_id":processor.getCacheID(),"gpu_cache_id":direct.getCacheID(),
                        "gpu_processor":{"isNoOp":direct.isNoOp(),"hasChannelCrosstalk":direct.hasChannelCrosstalk()},
                        "shader":gpu._shader(desc,expected_blobs),"log":log}
                create={"major_version":major}
                if off:
                    create["processor_cache"]="off"
                args={"config":{"create":create},"transform":transform_spec,"edge_len":2}
                ordinary={key:value for key,value in args.items() if key!="edge_len"}
                actual,blobs=gpu.gpu_shader(ordinary,[])
                assert actual==expected and blobs==expected_blobs,(major,name,inverse,off,"Python")
                actual,blobs,abi=legacy_gpu._native(program,legacy_gpu._message(ordinary,[],[{"kind":"default"}]))
                assert actual==expected and blobs==expected_blobs,(major,name,inverse,off,"native")
                # Op::supportedByLegacyShader defaults true (Op.h:248). These
                # Exponent/CDL analytic ops require no lattice, so legacy equals
                # the independent ordinary wheel processor, including cache IDs.
                actual,blobs=legacy_gpu.legacy_gpu_shader(args,[])
                assert actual.pop("oracle_native")["source_key"]==provenance["source_key"]
                assert actual==expected and blobs==expected_blobs,(major,name,inverse,off,"legacy")
                assert expected["shader"]["textures_3d"]==[]
                if not inverse and not off:
                    versions[major][name]=(expected,expected_blobs)
                seen.append([major,name,inverse,off])
for name in ["ExponentTransform","CDLTransform"]:
    assert versions[1][name]!=versions[2][name],("major ignored",name)

# Unsupported unsigned majors also reach the native setter with exact wheel errors.
for major in [0,3,4294967295]:
    args={"config":{"create":{"major_version":major}},"transform":{"class":"MatrixTransform"}}
    expected,expected_blobs=gpu.gpu_shader(args,[])
    actual,blobs,abi=legacy_gpu._native(program,legacy_gpu._message(args,[],[{"kind":"default"}]))
    assert expected.get("stage")=="config" and "exception" in expected
    assert actual==expected and blobs==expected_blobs,(major,actual,expected)
print(json.dumps({"cases":seen,"native_config_errors":3,"source_key":provenance["source_key"]}))
"#;
    let lines = Oracle::get().run_script(script, &[]);
    assert_eq!(lines.len(), 1);
    let report: Value = serde_json::from_str(&lines[0]).expect("versioned native shader parity");
    assert_eq!(report["cases"].as_array().unwrap().len(), 16);
    assert_eq!(report["native_config_errors"], json!(3));
    assert_eq!(report["source_key"].as_str().unwrap().len(), 64);
}
