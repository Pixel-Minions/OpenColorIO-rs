// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Processors for step scripts: `Config.CreateRaw().getProcessor(t).getDefaultCPUProcessor()`,
//! each step compared with the wheel's (the config's and the processor's too, since the port
//! binds them: cards p6-config and p6-processors).
#![allow(dead_code)]

use super::script::{Script, n, r, ra};

/// Starts the interpreter.
pub(crate) fn start() {
    super::start_python();
}

/// The default CPU processor of the transform of step `t`.
pub(crate) fn cpu(sc: &mut Script, t: usize) -> usize {
    let config = sc.call(n("OCIO.Config.CreateRaw"), vec![]);
    let processor = sc.call(ra(config, "getProcessor"), vec![r(t)]);
    sc.call(ra(processor, "getDefaultCPUProcessor"), vec![])
}
