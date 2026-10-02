// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config: a port of the parts of `src/OpenColorIO/Config.cpp` @ v2.5.2 that the transforms
//! and processors need so far: the version, the context, and the processors of a transform
//! with their cache. `Config::CreateRaw`'s full state comes with its color spaces; the rest of
//! the config (YAML, color spaces, displays, looks) is Phase 3.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::platform::is_env_present;

use crate::caching::{OCIO_DISABLE_CACHE_FALLBACK, ProcessorCache, std_hash_string};
use crate::context::Context;
use crate::context_variable_utils::collect_context_variables;
use crate::processor::{Processor, ProcessorCacheFlags};
use crate::transform::Transform;

/// A config: so far the version the op builders read (`BuildCDLOp` and `BuildExponentOp` build
/// version 1 configs' ops differently), the current context, and the processors it made.
///
/// Port of `Config` and `Config::Impl` (include/OpenColorIO/OpenColorIO.h,
/// src/OpenColorIO/Config.cpp @ v2.5.2), in part.
#[derive(Debug)]
pub struct Config {
    /// `m_majorVersion`.
    major_version: u32,
    /// `m_minorVersion`.
    minor_version: u32,
    /// `m_context`.
    context: Arc<Context>,
    /// `m_cacheFlags` (`mutable`: a const config changes it).
    cache_flags: AtomicU32,
    /// `m_processorCache`: the processors, by `std::hash` of their key.
    processor_cache: ProcessorCache<u64, Arc<Processor>>,
}

impl Clone for Config {
    /// A copy with the same cache flags and an empty processor cache, enabled by them.
    ///
    /// Port of `Config::Impl::operator=` (src/OpenColorIO/Config.cpp:379-457 @ v2.5.2), in
    /// part.
    fn clone(&self) -> Config {
        let config = Config {
            major_version: self.major_version,
            minor_version: self.minor_version,
            context: Arc::new((*self.context).clone()),
            cache_flags: AtomicU32::new(self.cache_flags.load(Ordering::Relaxed)),
            processor_cache: ProcessorCache::new(),
        };
        config.processor_cache.clear();
        config.processor_cache.enable(
            config
                .processor_cache_flags()
                .has_flag(ProcessorCacheFlags::ENABLED),
        );
        config
    }
}

impl Config {
    /// The raw config: version 2.0, its one color space `raw`, its roles and its display.
    /// So far only the version, the context and the processor cache; the rest of its state
    /// comes with the color spaces, built directly until the YAML reader (Phase 3.3) parses
    /// upstream's profile.
    ///
    /// Port of `Config::CreateRaw` (src/OpenColorIO/Config.cpp:74-92, 1127-1133 @ v2.5.2) and
    /// `Config::Impl::Impl` (Config.cpp:335-374 @ v2.5.2), in part.
    #[doc(alias = "CreateRaw")]
    pub fn create_raw() -> Arc<Config> {
        let config = Config {
            major_version: 2,
            minor_version: 0,
            context: Arc::new(Context::new()),
            cache_flags: AtomicU32::new(ProcessorCacheFlags::DEFAULT.0),
            processor_cache: ProcessorCache::new(),
        };
        config.processor_cache.enable(
            config
                .processor_cache_flags()
                .has_flag(ProcessorCacheFlags::ENABLED),
        );
        Arc::new(config)
    }

    /// Port of `Config::getMajorVersion`.
    #[doc(alias = "getMajorVersion")]
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Port of `Config::getMinorVersion`.
    #[doc(alias = "getMinorVersion")]
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Port of `Config::getCurrentContext`.
    #[doc(alias = "getCurrentContext")]
    pub fn current_context(&self) -> &Arc<Context> {
        &self.context
    }

    /// Port of `Config::getProcessorCacheFlags` (Config.cpp:924-927, 5333-5336 @ v2.5.2).
    #[doc(alias = "getProcessorCacheFlags")]
    pub fn processor_cache_flags(&self) -> ProcessorCacheFlags {
        ProcessorCacheFlags(self.cache_flags.load(Ordering::Relaxed))
    }

    /// Sets the cache flags of the config and of the processors it makes from now on, and
    /// enables or disables the config's cache of processors (which keeps its entries).
    ///
    /// Port of `Config::setProcessorCacheFlags` (Config.cpp:929-933, 5338-5341 @ v2.5.2).
    #[doc(alias = "setProcessorCacheFlags")]
    pub fn set_processor_cache_flags(&self, flags: ProcessorCacheFlags) {
        self.cache_flags.store(flags.0, Ordering::Relaxed);
        self.processor_cache
            .enable(flags.has_flag(ProcessorCacheFlags::ENABLED));
    }

    /// Empties the config's cache of processors.
    ///
    /// Port of `Config::clearProcessorCache` (Config.cpp:5343-5346 @ v2.5.2).
    #[doc(alias = "clearProcessorCache")]
    pub fn clear_processor_cache(&self) {
        self.processor_cache.clear();
    }

    /// The processor of `transform`, forward, in the current context.
    ///
    /// Port of `Config::getProcessor(const ConstTransformRcPtr &)` (Config.cpp:4779-4782 @
    /// v2.5.2).
    #[doc(alias = "getProcessor")]
    pub fn processor(&self, transform: &Transform) -> Result<Arc<Processor>> {
        self.processor_in_direction(transform, TransformDirection::Forward)
    }

    /// The processor of `transform` in the direction `direction`, in the current context.
    ///
    /// Port of `Config::getProcessor(const ConstTransformRcPtr &, TransformDirection)`
    /// (Config.cpp:4784-4789 @ v2.5.2).
    #[doc(alias = "getProcessor")]
    pub fn processor_in_direction(
        &self,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<Arc<Processor>> {
        let context = self.current_context().clone();
        self.processor_with_context(&context, transform, direction)
    }

    /// The processor of `transform` in the direction `direction`, in `context`: from the
    /// config's cache when it has one under the same key (the transform's text, the direction,
    /// and the context variables it uses), or, unless `OCIO_DISABLE_CACHE_FALLBACK` is set, one
    /// with the same cache ID; otherwise a new one, which the cache keeps.
    ///
    /// Port of `Config::getProcessor(const ConstContextRcPtr &, const ConstTransformRcPtr &,
    /// TransformDirection)` (Config.cpp:4791-4880 @ v2.5.2). Its errors for a null context or
    /// transform can't happen with references.
    #[doc(alias = "getProcessor")]
    pub fn processor_with_context(
        &self,
        context: &Context,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<Arc<Processor>> {
        // The goal of the usedContext is to only contain the context vars that are actually
        // used for this transform. This allows the cache to be more efficient. However, there
        // are still some various TODOs since the usedContext will sometimes contain more vars
        // than are needed.
        //
        // The context's search path, working directory and IO proxy, which upstream copies
        // into it, come with the context's state (Phase 3).
        let mut used_context = Context::new();

        let need_context_variables =
            collect_context_variables(self, context, transform, &mut used_context);

        // Create helper method.
        let create_processor = || -> Result<Arc<Processor>> {
            let mut processor = Processor::new();
            processor.set_processor_cache_flags(self.processor_cache_flags());
            processor.set_transform(self, context, transform, direction)?;
            processor.compute_metadata();
            Ok(Arc::new(processor))
        };

        if let Some(mut cache) = self.processor_cache.lock() {
            // Note that the key includes a string description of the transform which does not
            // include all the LUT entries (just the arguments of the FileTransforms for LUTs).
            if need_context_variables {
                // Only the transforms that read context variables (color space, display view,
                // file and look transforms) need them, and none of those is ported yet; the
                // used context's cache ID comes with them.
                return Err(Exception::new(
                    "Config::getProcessor: the cache ID of a context is not ported yet.",
                ));
            }
            // `oss << "" << *transform << direction`: the direction prints as its value.
            let text = format!("{transform}{}", direction as i32);
            let key = std_hash_string(text.as_bytes());

            // Upstream's `m_processorCache[key]` adds an empty entry before it creates the
            // processor, which stays empty when the creation throws; the fallback below skips
            // empty entries, so the port adds none.
            if let Some(processor) = cache.entries().get(&key) {
                return Ok(processor.clone());
            }

            let proc = create_processor()?;

            let mut processor = None;

            let do_fallback = !is_env_present(OCIO_DISABLE_CACHE_FALLBACK);
            if do_fallback {
                // If an entry with the same cache ID already exists in the cache then reuse it
                // instead of the newly created one. Even with different context, the same
                // processor could be created (e.g. the processor creation does not rely on
                // some context variables).

                // The benefit to using the existing one is that it may already have an
                // optimized Processor, CPUProcessor, or GPUProcessor inside it.

                // Upstream compares the two cache IDs with `strcmp`; a cache ID is a hash or
                // "<NOOP>", without NUL. Both processors' ops are finalized, so neither cache
                // ID can fail.
                for entry in cache.entries().values() {
                    if entry.cache_id()? == proc.cache_id()? {
                        processor = Some(entry.clone());
                        break;
                    }
                }
            }

            let processor = processor.unwrap_or(proc);
            cache.entries().insert(key, processor.clone());
            return Ok(processor);
        }
        create_processor()
    }
}
