// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read-only queries over a successfully built component graph.

mod internal;
mod query_index;

use std::any::TypeId;
use std::sync::Arc;
use std::sync::Mutex;

pub(crate) use internal::BuiltBinding;
use query_index::QueryIndex;

use crate::builder::ContainerBuilder;
use crate::error::ResolveError;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::managed::CleanupJournal;
use crate::managed::ShutdownHandle;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// Shared, read-only component context after successful construction.
///
/// Cloning an `Arc<ApplicationContext>` permits concurrent queries from
/// multiple threads. Shutdown remains a single-owner operation: release all
/// shared context handles, recover the context with `Arc::try_unwrap`, then
/// call [`Self::begin_shutdown`].
///
/// Every successful query clones an existing `Arc`; factories never run again.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ApplicationContext;
///
/// let mut builder = ApplicationContext::builder();
/// builder.register_instance(Arc::new(String::from("hello")))?;
/// let context = builder.build_all()?;
/// assert_eq!(context.get::<String>()?.as_str(), "hello");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ApplicationContext {
    /// Immutable constructed instances shared with lookup callers.
    store: InstanceStore,
    /// Active binding metadata used to resolve and order queries.
    bindings: Vec<BuiltBinding>,
    /// Immutable indexes for exact-key and same-type metadata lookup.
    query_index: QueryIndex,
    /// Explicit lifecycle actions for managed concrete bindings.
    cleanup: Mutex<CleanupJournal>,
}

impl ApplicationContext {
    /// Creates a builder for this context.
    ///
    /// # Returns
    ///
    /// A new builder with the default activation profile.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }

    /// Returns the active source and earlier sources replaced for one exact
    /// key.
    ///
    /// `None` means that `key` is absent. The returned slice borrows this
    /// immutable context and lists removed active bindings in registration
    /// order.
    ///
    /// # Parameters
    ///
    /// `key` is the exact typed binding key whose source history is queried.
    ///
    /// # Returns
    ///
    /// `Some` contains the active definition source and replaced sources;
    /// `None` means no active binding has the exact key.
    #[must_use]
    pub fn binding_sources(&self, key: &BindingKey) -> Option<(DefinitionSource, &[DefinitionSource])> {
        self.query_index
            .by_key(key)
            .map(|index| &self.bindings[index])
            .map(|binding| (binding.source, binding.replaced_sources.as_slice()))
    }

    /// Creates a context from a fully constructed store and active lookup
    /// metadata.
    ///
    /// The caller must publish only bindings whose keys and erased values
    /// agree.
    pub(crate) fn new(store: InstanceStore, bindings: Vec<BuiltBinding>, cleanup: CleanupJournal) -> Self {
        let query_index = QueryIndex::new(&bindings);
        let mut cleanup = cleanup;
        cleanup.disarm_abort();
        Self {
            store,
            bindings,
            query_index,
            cleanup: Mutex::new(cleanup),
        }
    }

    /// Sends every managed stop request and returns a handle for waiting.
    ///
    /// Stop actions are attempted in reverse construction order before any wait
    /// begins. Dropping the context without calling this method does not stop
    /// components. The returned handle keeps managed values alive while waits
    /// remain. Cloned component `Arc`s may remain alive after shutdown.
    ///
    /// # Returns
    ///
    /// Returns a handle that can resume an interrupted wait.
    ///
    /// # Errors
    ///
    /// This method does not return cleanup errors; [`ShutdownHandle::wait`]
    /// returns all stop and wait failures after waiting completes.
    pub fn begin_shutdown(self) -> ShutdownHandle {
        let mut cleanup = self
            .cleanup
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let failures = cleanup.stop_reverse();
        ShutdownHandle::new(self.store, cleanup, failures)
    }

    /// Returns the only `T`, or the unique primary when several exist.
    ///
    /// Missing and ambiguous selections return structured [`ResolveError`].
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object type used to identify the binding.
    /// It must be thread-safe and `'static` so the context can share it.
    ///
    /// # Parameters
    ///
    /// `id` is the exact, case-sensitive identifier of the requested binding.
    ///
    /// # Returns
    ///
    /// The selected shared component.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when no component matches or selection is
    /// ambiguous.
    #[must_use = "handle the component lookup result"]
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        let request = BindingKey::of::<T>(None);
        let candidates = self.candidates(TypeId::of::<T>());
        let selected = self.select(&request, &candidates)?;
        Ok(self.read::<T>(&selected.key))
    }

    /// Returns the `T` bound to the exact, case-sensitive `id`.
    ///
    /// Invalid IDs, missing bindings and ambiguous bindings return
    /// [`ResolveError`] with the original request and candidates.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object type used to identify the binding.
    /// It must be thread-safe and `'static` so the context can share it.
    ///
    /// # Returns
    ///
    /// The component bound to the validated exact identifier.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] for an invalid ID, missing binding, or
    /// ambiguous selection.
    #[must_use = "handle the component lookup result"]
    pub fn get_by_id<T: ?Sized + Send + Sync + 'static>(&self, id: &str) -> Result<Arc<T>, ResolveError> {
        let request = BindingKey::of::<T>(Some(BindingId::parse(id)?));
        if let Some(index) = self.query_index.by_key(&request) {
            return Ok(self.read::<T>(&self.bindings[index].key));
        }
        let candidates = self.candidates(TypeId::of::<T>());
        let selected = self.select(&request, &candidates)?;
        Ok(self.read::<T>(&selected.key))
    }

    /// Returns `Some` for a unique selected `T`, `None` when absent, or an
    /// ambiguity error when multiple non-primary candidates exist.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object type used to identify candidates.
    /// It must be thread-safe and `'static` so the context can share it.
    ///
    /// # Returns
    ///
    /// `Some` contains the uniquely selected component; `None` means no
    /// binding exists for `T`.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when multiple non-primary candidates remain.
    #[must_use = "handle the optional component lookup result"]
    pub fn try_get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Option<Arc<T>>, ResolveError> {
        let request = BindingKey::of::<T>(None);
        let candidates = self.candidates(TypeId::of::<T>());
        if candidates.is_empty() {
            return Ok(None);
        }
        self.select(&request, &candidates)
            .map(|binding| Some(self.read::<T>(&binding.key)))
    }

    /// Returns all built `T` values sorted by order, ID and source location.
    ///
    /// A type with no bindings yields an empty vector.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object type whose bindings are collected.
    /// It must be thread-safe and `'static` so the context can share them.
    ///
    /// # Returns
    ///
    /// All matching shared components, ordered by binding order, ID, and
    /// source location. An absent type produces an empty vector.
    #[must_use]
    pub fn get_all<T: ?Sized + Send + Sync + 'static>(&self) -> Vec<Arc<T>> {
        let mut candidates = self.candidates(TypeId::of::<T>());
        candidates.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.key.id().cmp(&right.key.id()))
                .then_with(|| left.source.cmp(&right.source))
        });
        candidates.iter().map(|binding| self.read::<T>(&binding.key)).collect()
    }

    /// Lists active bindings for one Rust type in registration order.
    fn candidates(&self, type_id: TypeId) -> Vec<&BuiltBinding> {
        self.query_index
            .by_type(type_id)
            .iter()
            .map(|&index| &self.bindings[index])
            .collect()
    }

    /// Resolves one named or unnamed query from candidates already in this
    /// context.
    fn select<'binding>(
        &self,
        request: &BindingKey,
        candidates: &[&'binding BuiltBinding],
    ) -> Result<&'binding BuiltBinding, ResolveError> {
        let matching: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|binding| request.id().is_none() || binding.key.id() == request.id())
            .collect();
        match matching.as_slice() {
            [] => Err(ResolveError::MissingComponent {
                request: request.clone(),
                available: candidates.iter().map(|binding| binding.key.clone()).collect(),
            }),
            [only] => Ok(only),
            many if request.id().is_none() => {
                let primaries: Vec<_> = many.iter().copied().filter(|binding| binding.primary).collect();
                match primaries.as_slice() {
                    [only] => Ok(only),
                    _ => Err(ResolveError::AmbiguousBinding {
                        request: request.clone(),
                        candidates: many.iter().map(|binding| binding.key.clone()).collect(),
                    }),
                }
            }
            many => Err(ResolveError::AmbiguousBinding {
                request: request.clone(),
                candidates: many.iter().map(|binding| binding.key.clone()).collect(),
            }),
        }
    }

    /// Clones an `Arc<T>` that must exist after successful construction.
    fn read<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Arc<T> {
        self.store
            .get::<T>(key)
            .expect("published binding must exist with its registered type")
    }
}
