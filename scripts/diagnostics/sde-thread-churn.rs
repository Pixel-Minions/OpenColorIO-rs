// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Diagnostic only: independent of OCIO and its test harness.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let count: usize = args[1].parse().unwrap();
    let concurrent = args[2] == "concurrent";
    if concurrent {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(count + 1));
        let mut workers = Vec::new();
        for index in 0..count {
            let barrier = barrier.clone();
            workers.push(std::thread::Builder::new().name(format!("probe-{index}")).spawn(move || { barrier.wait(); }).unwrap());
        }
        barrier.wait();
        for worker in workers { worker.join().unwrap(); }
    } else {
        for index in 0..count {
            std::thread::Builder::new().name(format!("probe-{index}")).spawn(|| {}).unwrap().join().unwrap();
            if (index + 1).is_multiple_of(64) { println!("completed={}", index + 1); }
        }
    }
    println!("COMPLETE count={count} mode={}", args[2]);
}
