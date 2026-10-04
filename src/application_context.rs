// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read-only queries over a successfully built component graph.

// Owns the private shared query data, binding metadata, and lookup indexes
// backing every clone of this context.
mod internal;

use std::any::TypeId;
use std::sync::Arc;
use std::sync::atomic::Ordering;

pub(crate) use internal::BuiltBinding;
use internal::ContextInner;

use crate::application_state::ApplicationState;
use crate::error::ResolveError;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// Cloneable, read-only queries over an application's constructed components.
///
/// Clones share immutable values and lifecycle state. Keeping a clone alive
/// does not prevent the application owner from shutting down resources.
/// Queries remain available after shutdown; they do not guarantee that a
/// component still accepts work. [`state()`](Self::state) is an observation
/// signal; it does not gate or reject component queries.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::Application;
///
/// let mut builder = Application::builder();
/// builder.register_instance(Arc::new(String::from("hello")))?;
/// let application = builder.build_all()?;
/// let context = application.context().clone();
/// assert_eq!(context.get::<String>()?.as_str(), "hello");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct ApplicationContext {
    /// Immutable query data and shared lifecycle state.
    inner: Arc<ContextInner>,
}

impl ApplicationContext {
    /// Publishes a successfully constructed store and its exact binding
    /// metadata.
    ///
    /// # Parameters
    ///
    /// `store` is the instance store holding every constructed component, and
    /// `bindings` lists the exact binding metadata in registration order; both
    /// are moved into the shared context and never mutated afterwards.
    pub(crate) fn new(store: InstanceStore, bindings: Vec<BuiltBinding>) -> Self {
        Self {
            inner: Arc::new(ContextInner::new(store, bindings)),
        }
    }

    /// Returns the latest lifecycle state published by the unique owner.
    ///
    /// The value is loaded with acquire ordering, so a clone that observes a
    /// new state also observes the components already published for it. This
    /// is an observation API; query methods do not use the state as an access
    /// gate, and a lookup can still succeed after shutdown.
    ///
    /// # Returns
    ///
    /// The most recently published [`ApplicationState`], or
    /// [`ApplicationState::Incomplete`] if the stored discriminant is not one
    /// this version recognises.
    #[inline]
    #[must_use]
    pub fn state(&self) -> ApplicationState {
        match self.inner.state.load(Ordering::Acquire) {
            0 => ApplicationState::Running,
            1 => ApplicationState::ShuttingDown,
            2 => ApplicationState::Closed,
            _ => ApplicationState::Incomplete,
        }
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
    #[inline]
    #[must_use]
    pub fn binding_sources(&self, key: &BindingKey) -> Option<(DefinitionSource, &[DefinitionSource])> {
        self.inner
            .query_index
            .by_key(key)
            .map(|index| &self.inner.bindings[index])
            .map(|binding| (binding.source, binding.replaced_sources.as_slice()))
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
        let candidates = self.inner.query_index.by_type(TypeId::of::<T>());
        let selected = self.select(&request, candidates)?;
        Ok(self.read::<T>(&self.inner.bindings[selected].key))
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
    /// # Parameters
    ///
    /// `id` is the exact, case-sensitive identifier of the requested binding.
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
        if let Some(index) = self.inner.query_index.by_key(&request) {
            return Ok(self.read::<T>(&self.inner.bindings[index].key));
        }
        let candidates = self.inner.query_index.by_type(TypeId::of::<T>());
        let selected = self.select(&request, candidates)?;
        Ok(self.read::<T>(&self.inner.bindings[selected].key))
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
        let candidates = self.inner.query_index.by_type(TypeId::of::<T>());
        if candidates.is_empty() {
            return Ok(None);
        }
        self.select(&request, candidates)
            .map(|index| Some(self.read::<T>(&self.inner.bindings[index].key)))
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
        self.inner
            .query_index
            .by_type_collection(TypeId::of::<T>())
            .iter()
            .map(|&index| self.read::<T>(&self.inner.bindings[index].key))
            .collect()
    }

    /// Publishes a lifecycle transition to all query clones.
    ///
    /// Only the application owner calls this after construction, so query
    /// clones observe the transition without any locking.
    ///
    /// # Parameters
    ///
    /// `state` is the lifecycle transition to publish to every clone sharing
    /// this context; it is stored with release ordering.
    pub(crate) fn publish_state(&self, state: ApplicationState) {
        self.inner.state.store(state as u8, Ordering::Release);
    }

    /// Resolves one named or unnamed query from candidates already in this
    /// context.
    ///
    /// A request that carries an exact ID matches only bindings with that ID.
    /// An unnamed request prefers the unique primary binding when exactly one
    /// primary candidate matches; every other selection reports a structured
    /// [`ResolveError`] instead of guessing.
    ///
    /// # Parameters
    ///
    /// `request` is the typed key whose ID filter and type filter are applied,
    /// and `candidates` are the binding indexes already filtered by the
    /// requested type.
    ///
    /// # Returns
    ///
    /// The index into the context binding table of the selected binding.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::MissingComponent`] when no candidate matches and
    /// [`ResolveError::AmbiguousBinding`] when several match without a unique
    /// primary selection.
    ///
    /// # Panics
    ///
    /// Panics if the counted match or primary match has no recorded index,
    /// which the single-assignment loop above cannot produce.
    fn select(&self, request: &BindingKey, candidates: &[usize]) -> Result<usize, ResolveError> {
        let mut matching_count = 0;
        let mut selected = None;
        let mut primary_count = 0;
        let mut primary = None;
        for &index in candidates {
            let binding = &self.inner.bindings[index];
            if request.id().is_some_and(|id| binding.key.id() != Some(id)) {
                continue;
            }
            matching_count += 1;
            selected = Some(index);
            if binding.primary {
                primary_count += 1;
                primary = Some(index);
            }
        }

        match matching_count {
            0 => Err(ResolveError::MissingComponent {
                request: request.clone(),
                available: candidates
                    .iter()
                    .map(|&index| self.inner.bindings[index].key.clone())
                    .collect(),
            }),
            1 => Ok(selected.expect("one match must have an index")),
            _ if request.id().is_none() && primary_count == 1 => {
                Ok(primary.expect("one primary match must have an index"))
            }
            _ => Err(ResolveError::AmbiguousBinding {
                request: request.clone(),
                candidates: candidates
                    .iter()
                    .filter_map(|&index| {
                        let binding = &self.inner.bindings[index];
                        (request.id().is_none() || binding.key.id() == request.id()).then(|| binding.key.clone())
                    })
                    .collect(),
            }),
        }
    }

    /// Clones an `Arc<T>` that must exist after successful construction.
    ///
    /// The store is keyed by the registered binding key, so the value is
    /// already shared and this lookup only clones the existing handle. `T` must
    /// match the type the binding was registered with.
    ///
    /// # Parameters
    ///
    /// `key` is the key of a binding published by the application owner.
    ///
    /// # Returns
    ///
    /// A new handle to the shared component value.
    ///
    /// # Panics
    ///
    /// Panics if `key` is absent from the store or its stored value is not a
    /// `T`; only a violated construction invariant can trigger this.
    fn read<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Arc<T> {
        self.inner
            .store
            .get::<T>(key)
            .expect("published binding must exist with its registered type")
    }
}
