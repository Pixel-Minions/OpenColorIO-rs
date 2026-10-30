// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Diagnostic only: one worker at a time; no OCIO or Rust-port function is called.
fn main() {
    let count: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    for index in 0..count {
        std::thread::Builder::new().name(format!("probe-{index}")).spawn(|| {}).unwrap().join().unwrap();
        if (index + 1).is_multiple_of(128) { println!("completed={}", index + 1); }
    }
    println!("COMPLETE count={count} sequential-workers=1");
}
