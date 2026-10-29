// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Diagnostic only: independent of OCIO and its test harness.
fn main() {
    let count: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    for index in 0..count {
        // Join each worker before creating the next: only one worker is ever live.
        std::thread::spawn(|| {}).join().unwrap();
        if (index + 1).is_multiple_of(64) {
            println!("completed={}", index + 1);
        }
    }
    println!("COMPLETE count={count} sequential-workers=1");
}
