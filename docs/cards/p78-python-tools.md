# P78: Python package tool launcher

Own package/command_line.py's Rust-side staging in xtask/src/py_tools.rs, py.rs and
its module declaration, the package's verbatim Python glue and source-map entries.
Do not modify the developer tools or their existing numerical/ICC implementations.

Keep upstream's command_line.py byte for byte, as for __init__.py. py-build builds
the four in-scope ocio-tools binaries and copies only Cargo's declared executables
into PyOpenColorIO/bin. Native artifact selection rejects missing/duplicate/unknown
tools. Release follows py-build's existing --release choice. The upstream launcher
retains its own lookup, basename, argument forwarding and SystemExit behavior.

py-suite checks both call_program and main against the official wheel for all four
tools, using --help and an invalid option. Exit status and stdout/stderr compare
exactly, with actual reports in py-tools.json. Both packages run in their separate
existing interpreters. The original384 functions and standalone driver still run.
Source identity and binary-manifest rejection controls are local native checks;
both-platform live-wheel execution remains required in normal CI before landing.

This fills one source-map gap. The other upstream applications already retain
their reviewed scope; no new application is excluded here. Installing distribution
console entry points remains the release-packaging work recorded in Phase6/P6-3.
No oracle, fixture, threshold, renderer, unsafe code, dependency, waiver or public
Rust API shape is added. Help/argument checks are not CPU-dependent test targets.
