// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Construction of every selected binding from a validated graph.
//!
//! Owns the private store while components are built, attributes factory and
//! projection failures to their definition and root path, and hands either the
//! completed application or an immediate rollback owner back to the caller.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::application::Application;
use crate::application_context::ApplicationContext;
use crate::application_context::BuiltBinding;
use crate::binding::AliasProjector;
use crate::binding::ManagedProduct;
use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::build_context::BuildContext;
use crate::error::BuildError;
use crate::error::BuildFailure;
use crate::error::FactoryError;
use crate::graph::BindingLocation;
use crate::graph::DiagnosticPaths;
use crate::graph::ResolvedDependency;
use crate::graph::ValidatedGraph;
use crate::key::BindingKey;
use crate::managed::CleanupAction;
use crate::managed::CleanupJournal;
use crate::managed::ShutdownHandle;
use crate::managed::ShutdownMode;
use crate::managed::WaitPolicy;
use crate::options::DefinitionSource;
use crate::store::ErasedInstance;
use crate::store::InstanceStore;

/// One constructed erased value and the cleanup action its factory returned.
///
/// Unmanaged bindings carry `None`; managed bindings always carry the action
/// that must run when the partially built container is rolled back.
type Product = (ErasedInstance, Option<CleanupAction>);

/// Projects the selected bindings of a validated graph into publishable
/// lookup metadata, in registration order.
///
/// Only locations chosen by graph validation are kept, so the returned table
/// has exactly one entry per binding the construction loop will run.
///
/// # Parameters
///
/// `graph` supplies the definitions and their per-binding metadata, and
/// `selected` is the set of [`BindingLocation`] values graph validation ordered
/// for construction.
///
/// # Returns
///
/// One [`BuiltBinding`] per selected binding, in definition then binding
/// position order; an empty selection yields an empty vector.
fn published_bindings(graph: &ValidatedGraph, selected: &HashSet<BindingLocation>) -> Vec<BuiltBinding> {
    graph
        .definitions
        .iter()
        .enumerate()
        .flat_map(|(definition_index, definition)| {
            let selected = &selected;
            definition
                .bindings
                .iter()
                .enumerate()
                .filter(move |(binding_index, _)| {
                    selected.contains(&BindingLocation {
                        definition: definition_index,
                        binding: *binding_index,
                    })
                })
                .map(move |(_, binding)| BuiltBinding {
                    key: binding.key.clone(),
                    primary: binding.primary,
                    order: binding.order,
                    source: definition.source,
                    replaced_sources: binding.replaced_sources.clone(),
                })
        })
        .collect()
}

/// Owns a validated graph and its private, partially populated store.
pub(crate) struct Construction {
    /// Source location for each definition after graph validation.
    sources: Vec<DefinitionSource>,
    /// One-shot binding actions indexed by definition and binding position.
    actions: Vec<Vec<Option<PendingBinding>>>,
    /// Dependency-first order selected by graph validation.
    order: Vec<BindingLocation>,
    /// Exact dependency keys made available to each factory.
    resolved: Vec<Vec<ResolvedDependency>>,
    /// Compact graph provenance used to restore paths for failures.
    diagnostics: DiagnosticPaths,
    /// Lookup metadata published with the completed context.
    bindings: Vec<BuiltBinding>,
    /// Cleanup actions for managed components constructed so far.
    cleanup: CleanupJournal,
    /// Partially populated store, kept private until construction succeeds.
    store: InstanceStore,
    /// Validated lifecycle deadlines carried into the successful owner.
    policy: WaitPolicy,
}

// BuildError keeps full dependency paths and original factory sources.
#[allow(clippy::result_large_err)]
impl Construction {
    /// Retains lookup and diagnostic metadata before consuming one-shot
    /// bindings.
    ///
    /// The validated graph is split into three parallel views: the per
    /// definition source table used for error attribution, the still-callable
    /// one-shot binding actions, and the selected dependency-first order. The
    /// store and the cleanup journal start empty so a failed build can be
    /// rolled back without touching components owned by the caller.
    ///
    /// # Returns
    ///
    /// A constructor that owns every pending action; nothing is inserted into
    /// the store until [`Self::run_sync`] or [`Self::run_async`] runs.
    ///
    /// # Parameters
    ///
    /// `graph` is consumed for its definitions, selected order, resolved
    /// dependency lists, and diagnostic provenance, and `policy` carries the
    /// validated lifecycle deadlines into the owner this constructor later
    /// produces.
    #[must_use]
    pub(crate) fn new(graph: ValidatedGraph, policy: WaitPolicy) -> Self {
        let selected: HashSet<_> = graph.order.iter().copied().collect();
        let bindings = published_bindings(&graph, &selected);
        let diagnostics = graph.diagnostics;
        let sources = graph.definitions.iter().map(|definition| definition.source).collect();
        let actions = graph
            .definitions
            .into_iter()
            .map(|definition| definition.bindings.into_iter().map(Some).collect())
            .collect();
        Self {
            sources,
            actions,
            order: graph.order,
            resolved: graph.resolved,
            diagnostics,
            bindings,
            cleanup: CleanupJournal::default(),
            store: InstanceStore::default(),
            policy,
        }
    }

    /// Runs every sync binding after the graph and async preflight have
    /// succeeded.
    ///
    /// Each binding is constructed in dependency order and inserted into the
    /// private store before the next one starts. Managed products register
    /// their cleanup action before insertion, so an early return can roll back
    /// every component constructed so far.
    ///
    /// # Returns
    ///
    /// The completed [`Application`] owning the store, the lookup metadata,
    /// and the cleanup journal.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] carrying the originating [`BuildError`] and,
    /// whenever at least one managed component was already constructed, an
    /// immediate [`ShutdownHandle`] that aborts those components without
    /// waiting.
    ///
    /// This consumes the constructor: the pending order is taken, and on
    /// failure the partial store, lookup metadata, cleanup journal, and policy
    /// all move into the returned rollback owner.
    pub(crate) fn run_sync(mut self) -> Result<Application, BuildFailure> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let product = match kind {
                PendingBindingKind::AsyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_) => {
                    Err(Self::async_required(&key, source))
                }
                immediate => self.immediate_product(location, &key, source, immediate),
            };
            match product {
                Ok(product) => self.accept(key, source, product),
                Err(error) => return Err(self.cleanup_and_wrap(error)),
            }
        }
        Ok(self.finish())
    }

    /// Drives each async factory to completion before starting the next
    /// binding.
    ///
    /// The ordering, insertion, and rollback rules match [`Self::run_sync`];
    /// only the two asynchronous arms differ, because each one awaits its
    /// future before the next binding is started.
    ///
    /// # Returns
    ///
    /// The completed [`Application`] owning the store, the lookup metadata,
    /// and the cleanup journal.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] carrying the originating [`BuildError`] and,
    /// whenever at least one managed component was already constructed, an
    /// immediate [`ShutdownHandle`] that aborts those components without
    /// waiting.
    ///
    /// Like [`Self::run_sync`], this consumes the constructor and moves the
    /// partial store, lookup metadata, cleanup journal, and policy into the
    /// returned rollback owner on failure.
    pub(crate) async fn run_async(mut self) -> Result<Application, BuildFailure> {
        match self.construct_async().await {
            Ok(()) => Ok(self.finish()),
            Err(error) => Err(self.cleanup_and_wrap(error)),
        }
    }

    /// Runs the remaining selected factories while retaining storage in this
    /// owner. Returns the attributed factory error without moving cleanup.
    /// Cancelling the borrowing future drops the current factory first.
    pub(crate) async fn construct_async(&mut self) -> Result<(), BuildError> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let product = match kind {
                PendingBindingKind::AsyncFactory(factory) => {
                    let context = self.build_context(location.definition);
                    let result = factory(context).await;
                    self.product(&key, source, location, result)
                }
                PendingBindingKind::ManagedAsyncFactory(factory) => {
                    let context = self.build_context(location.definition);
                    let result = factory(context).await;
                    self.managed_product(&key, source, location, result)
                }
                immediate => self.immediate_product(location, &key, source, immediate),
            };
            match product {
                Ok(product) => self.accept(key, source, product),
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Builds one product for every binding kind that needs no `await`.
    ///
    /// Both execution paths share this dispatch so the ordering, insertion, and
    /// rollback contract cannot drift between the sync and async loops.
    ///
    /// # Parameters
    ///
    /// `location` is the graph position whose dependency context factory kinds
    /// need, `key` and `source` attribute the product, and `kind` is the taken
    /// binding action. Asynchronous kinds never reach this helper; the two
    /// execution paths dispatch them before calling it.
    ///
    /// # Returns
    ///
    /// The ready-to-store [`Product`], or the failure already attributed to its
    /// definition and first root path.
    fn immediate_product(
        &mut self,
        location: BindingLocation,
        key: &BindingKey,
        source: DefinitionSource,
        kind: PendingBindingKind,
    ) -> Result<Product, BuildError> {
        match kind {
            PendingBindingKind::Instance(value) => Ok((value, None)),
            PendingBindingKind::SyncFactory(factory) => {
                let context = self.build_context(location.definition);
                self.product(key, source, location, factory(context))
            }
            PendingBindingKind::ManagedSyncFactory(factory) => {
                let context = self.build_context(location.definition);
                self.managed_product(key, source, location, factory(context))
            }
            PendingBindingKind::Alias { target, project } => self
                .project(key, source, location, &target, project)
                .map(|value| (value, None)),
            PendingBindingKind::AsyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_) => {
                Err(Self::async_required(key, source))
            }
        }
    }

    /// Transfers partial storage and the journal into an Immediate rollback
    /// owner, requesting every abort before returning without starting waits.
    ///
    /// A build that failed before any managed component was constructed owns
    /// nothing to roll back, so the failure is returned without a handle.
    ///
    /// # Parameters
    ///
    /// `cause` is the failure that stopped construction; it is preserved as the
    /// cause of the returned failure.
    ///
    /// # Returns
    ///
    /// A [`BuildFailure`] that owns an immediate [`ShutdownHandle`] aborting
    /// the already constructed managed components, or a handle-free failure
    /// when the cleanup journal is empty.
    pub(crate) fn cleanup_and_wrap(self, cause: BuildError) -> BuildFailure {
        BuildFailure::new(cause, self.into_cleanup())
    }

    /// Transfers completed managed resources into an Immediate handle and
    /// requests abort synchronously. Returns `None` when no resource completed.
    pub(crate) fn into_cleanup(self) -> Option<ShutdownHandle> {
        if self.cleanup.is_empty() {
            return None;
        }
        Some(ShutdownHandle::new(
            ApplicationContext::new(self.store, self.bindings),
            self.cleanup,
            self.policy,
            ShutdownMode::Immediate,
        ))
    }

    /// Takes the one-shot action for a graph location without changing its
    /// source.
    ///
    /// # Parameters
    ///
    /// `location` is the definition and binding position whose one-shot action
    /// is taken out of the action table.
    ///
    /// # Returns
    ///
    /// The exact key, the definition source, and the remaining binding action
    /// with its slot emptied so the binding cannot run twice.
    ///
    /// # Panics
    ///
    /// Panics when the location has no pending action, which graph validation
    /// prevents by ordering every selected binding exactly once.
    fn take_binding(&mut self, location: BindingLocation) -> (BindingKey, DefinitionSource, PendingBindingKind) {
        let binding = self.actions[location.definition][location.binding]
            .take()
            .expect("validated binding runs exactly once");
        (binding.key, self.sources[location.definition], binding.kind)
    }

    /// Gives a factory only its resolved requests, moving them into its
    /// context.
    ///
    /// The resolved dependency list is taken out of `self`, so a factory can
    /// never observe or repeat the requests of a sibling binding.
    ///
    /// # Parameters
    ///
    /// `definition_index` selects which definition's resolved request list is
    /// moved into the factory context.
    ///
    /// # Returns
    ///
    /// A [`BuildContext`] holding only this definition's already constructed
    /// dependency values plus its source; the request list is left empty
    /// afterwards so sibling factories cannot observe or repeat it.
    ///
    /// # Panics
    ///
    /// Panics when a resolved dependency is missing from the store, which
    /// dependency-first ordering prevents.
    fn build_context(&mut self, definition_index: usize) -> BuildContext {
        let source = self.sources[definition_index];
        let resolved = std::mem::take(&mut self.resolved[definition_index]);
        let mut values = HashMap::new();
        for dependency in &resolved {
            for key in &dependency.keys {
                values.entry(key.clone()).or_insert_with(|| {
                    self.store
                        .get_erased_cloned(key)
                        .expect("validated dependency must be constructed before its factory")
                });
            }
        }
        BuildContext::new(values, source, resolved)
    }

    /// Attributes an unmanaged factory result to its source and root path.
    ///
    /// A produced value carries no cleanup action, and a failure is wrapped
    /// with the failing key, definition source, and first root path.
    ///
    /// # Parameters
    ///
    /// `key` and `source` identify the binding under construction, `location`
    /// locates it in the diagnostic graph, and `result` is the factory outcome
    /// to attribute.
    ///
    /// # Returns
    ///
    /// The factory value paired with no cleanup action, or the attributed
    /// [`BuildError`].
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::FactoryFailed`], or
    /// [`BuildError::ConfigReadFailed`] when the failure came from reading
    /// a configuration snapshot.
    fn product(
        &self,
        key: &BindingKey,
        source: DefinitionSource,
        location: BindingLocation,
        result: Result<ErasedInstance, FactoryError>,
    ) -> Result<Product, BuildError> {
        result
            .map(|value| (value, None))
            .map_err(|error| self.factory_error(key, source, location, error))
    }

    /// Attributes a managed factory result and keeps its cleanup action.
    ///
    /// The value and its cleanup action stay paired so the journal can be
    /// updated before the value is stored, and failures are attributed exactly
    /// like an unmanaged factory result.
    ///
    /// # Parameters
    ///
    /// `key` and `source` identify the binding under construction, `location`
    /// locates it in the diagnostic graph, and `result` is the managed factory
    /// outcome to attribute.
    ///
    /// # Returns
    ///
    /// The factory value paired with its cleanup action, or the attributed
    /// [`BuildError`].
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::FactoryFailed`], or
    /// [`BuildError::ConfigReadFailed`] when the failure came from reading
    /// a configuration snapshot.
    fn managed_product(
        &self,
        key: &BindingKey,
        source: DefinitionSource,
        location: BindingLocation,
        result: Result<ManagedProduct, FactoryError>,
    ) -> Result<Product, BuildError> {
        result
            .map(|(value, cleanup)| (value, Some(cleanup)))
            .map_err(|error| self.factory_error(key, source, location, error))
    }

    /// Rejects a factory that can only complete asynchronously.
    ///
    /// The key is cloned because the caller keeps using it to attribute the
    /// failure to the definition that requested the binding.
    ///
    /// # Parameters
    ///
    /// `key` is the binding the synchronous path cannot construct, and `source`
    /// is its definition location.
    ///
    /// # Returns
    ///
    /// [`BuildError::AsyncRequired`] naming the definition and the requested
    /// key.
    fn async_required(key: &BindingKey, source: DefinitionSource) -> BuildError {
        BuildError::AsyncRequired {
            definition: source,
            key: key.clone(),
        }
    }

    /// Projects one alias from its already constructed concrete binding.
    ///
    /// A missing target value and a projection that rejects the stored type are
    /// both reported as one factory failure, because from the alias position
    /// the two cases are indistinguishable.
    ///
    /// # Parameters
    ///
    /// `key` and `source` identify the alias being projected, `location`
    /// locates it in the diagnostic graph, `target` is the concrete binding
    /// the projection reads, and `project` is the caller-supplied
    /// projection.
    ///
    /// # Returns
    ///
    /// The projected erased value sharing the concrete component's allocation.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::FactoryFailed`], or
    /// [`BuildError::ConfigReadFailed`] when the alias itself reads a
    /// configuration snapshot.
    fn project(
        &self,
        key: &BindingKey,
        source: DefinitionSource,
        location: BindingLocation,
        target: &BindingKey,
        project: AliasProjector,
    ) -> Result<ErasedInstance, BuildError> {
        let value = self.store.get_erased(target).and_then(project);
        value.ok_or_else(|| {
            self.factory_error(
                key,
                source,
                location,
                FactoryError::new(std::io::Error::other("alias projection target type mismatch")),
            )
        })
    }

    /// Journals the cleanup action, when present, and stores one product.
    ///
    /// The cleanup action is journalled before the value is inserted, so a
    /// later failure can never roll back past a component it does not know
    /// about. An unmanaged product only inserts its value.
    ///
    /// # Parameters
    ///
    /// `key` is the exact key the product is stored under, `source` is the
    /// definition location recorded with any cleanup action, and `product` is
    /// the value plus its optional cleanup action.
    fn accept(&mut self, key: BindingKey, source: DefinitionSource, product: Product) {
        let (value, cleanup) = product;
        if let Some(cleanup) = cleanup {
            self.cleanup.push(key.clone(), source, cleanup);
        }
        self.store.insert_erased(key, value);
    }

    /// Wraps a factory failure with source and the complete first root path.
    ///
    /// A failure raised while reading a configuration snapshot is reported as
    /// [`BuildError::ConfigReadFailed`], which names the requested key and
    /// target type; every other factory failure keeps the failing binding key
    /// in [`BuildError::FactoryFailed`].
    ///
    /// # Parameters
    ///
    /// `key` is the binding whose factory failed, `definition` is its reported
    /// source location, `location` selects the first root path to report, and
    /// `error` is the factory failure to wrap.
    ///
    /// # Returns
    ///
    /// [`BuildError::ConfigReadFailed`] when the failure carries configuration
    /// read context, otherwise [`BuildError::FactoryFailed`]; both carry the
    /// definition and the restored root path.
    fn factory_error(
        &self,
        key: &BindingKey,
        definition: DefinitionSource,
        location: BindingLocation,
        error: FactoryError,
    ) -> BuildError {
        let path = self.diagnostics.path_to(location);
        if let Some((path_key, target)) = error.config_read_context() {
            return BuildError::ConfigReadFailed {
                definition,
                path_key: path_key.to_owned(),
                target: target.to_owned(),
                path,
                error,
            };
        }
        BuildError::FactoryFailed {
            definition,
            key: key.clone(),
            path,
            error,
        }
    }

    /// Hands the completed store, lookup metadata, and journal to the new
    /// owner, leaving this constructor empty.
    ///
    /// The journal is moved out rather than copied, so the new owner is the
    /// only holder of the cleanup actions and this constructor can be dropped
    /// without running anything.
    ///
    /// # Returns
    ///
    /// The [`Application`] that owns the private store, the lookup metadata,
    /// the cleanup journal, and the validated lifecycle policy.
    pub(crate) fn finish(mut self) -> Application {
        Application::new(
            ApplicationContext::new(self.store, self.bindings),
            std::mem::take(&mut self.cleanup),
            self.policy,
        )
    }
}
