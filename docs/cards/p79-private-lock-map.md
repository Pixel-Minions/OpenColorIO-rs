# P79: account for the existing private lock types

Mutex.h defines only internal types: release std::mutex and std::lock_guard, or a
debug wrapper asserting against invalid recursive/owner/unbalanced use. The Rust
cache and logging implementations already use std::sync::Mutex and scoped guards:

- caching.rs: the generic cache's shared mutable state.
- context.rs: m_resultsCacheMutex's cache state and lock helper.
- processor.rs: the lazy results/cache ID.
- ocio-ops/logging.rs: the source explicitly maps its guard to AutoMutex lock(g_logmutex).

Rust guards are noncopyable, release on scope exit/unwind and cannot be sent to
another thread. Internal callers use those scoped guards rather than exposing a
manual unlock operation. This accounts for balanced ownership; it does not prevent
a second lock acquisition on the same thread, so it does not reproduce DebugLock's
explicit recursive-acquisition assertion. Retain partial status until that diagnostic
boundary is resolved. No public Rust mutex/debug API or diagnostic exclusion is added.
The only try_lock occurrences are the DebugLock definition and its contained call
to std::mutex; there is no caller of DebugLock::try_lock elsewhere in upstream.

Replace the nonexistent mutex.rs target with these existing uses. No lock code,
concurrency guarantee, renderer or test is changed, and no new N/A/waiver/deviation
is added. Existing full normal cache/logging tests remain the runtime validation.
