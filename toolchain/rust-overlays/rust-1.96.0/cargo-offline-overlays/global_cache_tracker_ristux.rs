//! Ristux-offline replacement for Cargo's SQLite-backed global cache tracker.
//!
//! Local/path builds do not need persistent cache last-use metadata. Keeping
//! this API as a no-op removes Cargo's bundled SQLite C dependency while
//! preserving the rest of Cargo's cache and locking behavior.

use crate::core::gc::GcOpts;
use crate::ops::CleanContext;
use crate::util::interning::InternedString;
use crate::util::Filesystem;
use crate::{CargoResult, GlobalContext};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct RegistryIndex {
    pub encoded_registry_name: InternedString,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct RegistryCrate {
    pub encoded_registry_name: InternedString,
    pub crate_filename: InternedString,
    pub size: u64,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct RegistrySrc {
    pub encoded_registry_name: InternedString,
    pub package_dir: InternedString,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct GitDb {
    pub encoded_git_name: InternedString,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct GitCheckout {
    pub encoded_git_name: InternedString,
    pub short_name: InternedString,
    pub size: Option<u64>,
}

#[derive(Debug)]
pub struct GlobalCacheTracker;

impl GlobalCacheTracker {
    pub fn new(_gctx: &GlobalContext) -> CargoResult<Self> {
        Ok(Self)
    }

    pub fn db_path(gctx: &GlobalContext) -> Filesystem {
        gctx.home().join(".global-cache")
    }

    pub fn registry_index_all(&self) -> CargoResult<Vec<(RegistryIndex, u64)>> {
        Ok(Vec::new())
    }

    pub fn registry_crate_all(&self) -> CargoResult<Vec<(RegistryCrate, u64)>> {
        Ok(Vec::new())
    }

    pub fn registry_src_all(&self) -> CargoResult<Vec<(RegistrySrc, u64)>> {
        Ok(Vec::new())
    }

    pub fn git_db_all(&self) -> CargoResult<Vec<(GitDb, u64)>> {
        Ok(Vec::new())
    }

    pub fn git_checkout_all(&self) -> CargoResult<Vec<(GitCheckout, u64)>> {
        Ok(Vec::new())
    }

    pub fn should_run_auto_gc(&mut self, _frequency: Duration) -> CargoResult<bool> {
        Ok(false)
    }

    pub fn set_last_auto_gc(&self) -> CargoResult<()> {
        Ok(())
    }

    pub fn clean(
        &mut self,
        _clean_ctx: &mut CleanContext<'_>,
        _gc_opts: &GcOpts,
    ) -> CargoResult<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct DeferredGlobalLastUse;

impl DeferredGlobalLastUse {
    pub fn new() -> Self {
        Self
    }

    pub fn is_empty(&self) -> bool {
        true
    }

    pub fn mark_registry_index_used(&mut self, _registry_index: RegistryIndex) {}

    pub fn mark_registry_crate_used(&mut self, _registry_crate: RegistryCrate) {}

    pub fn mark_registry_src_used(&mut self, _registry_src: RegistrySrc) {}

    pub fn mark_git_checkout_used(&mut self, _git_checkout: GitCheckout) {}

    pub fn mark_registry_index_used_stamp(
        &mut self,
        _registry_index: RegistryIndex,
        _timestamp: Option<&SystemTime>,
    ) {
    }

    pub fn mark_registry_crate_used_stamp(
        &mut self,
        _registry_crate: RegistryCrate,
        _timestamp: Option<&SystemTime>,
    ) {
    }

    pub fn mark_registry_src_used_stamp(
        &mut self,
        _registry_src: RegistrySrc,
        _timestamp: Option<&SystemTime>,
    ) {
    }

    pub fn mark_git_checkout_used_stamp(
        &mut self,
        _git_checkout: GitCheckout,
        _timestamp: Option<&SystemTime>,
    ) {
    }

    pub fn save(&mut self, _tracker: &mut GlobalCacheTracker) -> CargoResult<()> {
        Ok(())
    }

    pub fn save_no_error(&mut self, _gctx: &GlobalContext) {}
}

pub fn is_silent_error(_error: &anyhow::Error) -> bool {
    false
}
