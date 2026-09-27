// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::collections::HashMap;
use std::collections::HashSet;

use super::paths::paths_to_all;
use crate::application_context::ApplicationContext;
use crate::application_context::BuiltBinding;
use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::build_context::BuildContext;
use crate::error::BuildError;
use crate::error::FactoryError;
use crate::graph::BindingLocation;
use crate::graph::ResolvedDependency;
use crate::graph::ValidatedGraph;
use crate::key::BindingKey;
use crate::managed::CleanupJournal;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

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
    /// Stable root-to-binding paths used when wrapping factory failures.
    paths: HashMap<BindingKey, Vec<BindingKey>>,
    /// Lookup metadata published with the completed context.
    bindings: Vec<BuiltBinding>,
    /// Cleanup actions for managed components constructed so far.
    cleanup: CleanupJournal,
    /// Partially populated store, kept private until construction succeeds.
    store: InstanceStore,
}

// BuildError keeps full dependency paths and original factory sources.
#[allow(clippy::result_large_err)]
impl Construction {
    /// Retains lookup and diagnostic metadata before consuming one-shot
    /// bindings.
    pub(crate) fn new(graph: ValidatedGraph) -> Self {
        let selected: HashSet<_> = graph.order.iter().copied().collect();
        let paths = paths_to_all(&graph.definitions, &graph.resolved, &selected);
        let bindings = graph
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
            .collect();
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
            paths,
            bindings,
            cleanup: CleanupJournal::default(),
            store: InstanceStore::default(),
        }
    }

    /// Runs every sync binding after the graph and async preflight have
    /// succeeded.
    pub(crate) fn run_sync(mut self) -> Result<ApplicationContext, BuildError> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let product = match kind {
                PendingBindingKind::Instance(value) => Ok((value, None)),
                PendingBindingKind::ManagedInstance(value, cleanup) => Ok((value, Some(cleanup))),
                PendingBindingKind::SyncFactory(factory) => factory(self.build_context(location.definition))
                    .map(|value| (value, None))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::ManagedSyncFactory(factory) => factory(self.build_context(location.definition))
                    .map(|(value, cleanup)| (value, Some(cleanup)))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::AsyncFactory(_) => Err(BuildError::AsyncRequired {
                    definition: source,
                    key: key.clone(),
                }),
                PendingBindingKind::ManagedAsyncFactory(_) => Err(BuildError::AsyncRequired {
                    definition: source,
                    key: key.clone(),
                }),
                PendingBindingKind::Alias { target, project } => {
                    self.project(&key, source, &target, project).map(|value| (value, None))
                }
            };
            let (value, cleanup) = match product {
                Ok(product) => product,
                Err(error) => return Err(self.stop_and_wrap(error)),
            };
            if let Some(cleanup) = cleanup {
                self.cleanup.push(key.clone(), source, cleanup);
            }
            self.insert(key, value);
        }
        Ok(ApplicationContext::new(
            self.store,
            self.bindings,
            std::mem::take(&mut self.cleanup),
        ))
    }

    /// Drives each async factory to completion before starting the next
    /// binding.
    pub(crate) async fn run_async(mut self) -> Result<ApplicationContext, BuildError> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let product = match kind {
                PendingBindingKind::Instance(value) => Ok((value, None)),
                PendingBindingKind::ManagedInstance(value, cleanup) => Ok((value, Some(cleanup))),
                PendingBindingKind::SyncFactory(factory) => factory(self.build_context(location.definition))
                    .map(|value| (value, None))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::AsyncFactory(factory) => factory(self.build_context(location.definition))
                    .await
                    .map(|value| (value, None))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::ManagedSyncFactory(factory) => factory(self.build_context(location.definition))
                    .map(|(value, cleanup)| (value, Some(cleanup)))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::ManagedAsyncFactory(factory) => factory(self.build_context(location.definition))
                    .await
                    .map(|(value, cleanup)| (value, Some(cleanup)))
                    .map_err(|error| self.factory_error(&key, source, error)),
                PendingBindingKind::Alias { target, project } => {
                    self.project(&key, source, &target, project).map(|value| (value, None))
                }
            };
            let (value, cleanup) = match product {
                Ok(product) => product,
                Err(error) => return Err(self.cleanup_and_wrap(error).await),
            };
            if let Some(cleanup) = cleanup {
                self.cleanup.push(key.clone(), source, cleanup);
            }
            self.insert(key, value);
        }
        Ok(ApplicationContext::new(
            self.store,
            self.bindings,
            std::mem::take(&mut self.cleanup),
        ))
    }

    /// Stops built managed components after a synchronous construction failure.
    fn stop_and_wrap(&mut self, cause: BuildError) -> BuildError {
        let failures = self.cleanup.stop_reverse();
        if failures.is_empty() {
            cause
        } else {
            BuildError::CleanupFailed {
                cause: Box::new(cause),
                failures,
            }
        }
    }

    /// Stops and waits for built managed components after an asynchronous
    /// failure.
    async fn cleanup_and_wrap(&mut self, cause: BuildError) -> BuildError {
        let mut failures = self.cleanup.stop_reverse();
        failures.extend(self.cleanup.wait_reverse().await);
        if failures.is_empty() {
            cause
        } else {
            BuildError::CleanupFailed {
                cause: Box::new(cause),
                failures,
            }
        }
    }

    /// Takes the one-shot action for a graph location without changing its
    /// source.
    fn take_binding(&mut self, location: BindingLocation) -> (BindingKey, DefinitionSource, PendingBindingKind) {
        let binding = self.actions[location.definition][location.binding]
            .take()
            .expect("validated binding runs exactly once");
        (binding.key, self.sources[location.definition], binding.kind)
    }

    /// Gives a factory only its resolved requests, moving them into its
    /// context.
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

    /// Projects one alias from its already constructed concrete binding.
    fn project(
        &self,
        key: &BindingKey,
        source: DefinitionSource,
        target: &BindingKey,
        project: crate::binding::AliasProjector,
    ) -> Result<crate::store::ErasedInstance, BuildError> {
        let value = self.store.get_erased(target).and_then(project);
        value.ok_or_else(|| {
            self.factory_error(
                key,
                source,
                FactoryError::new(std::io::Error::other("alias projection target type mismatch")),
            )
        })
    }

    /// Inserts one successfully constructed complete erased `Arc`.
    fn insert(&mut self, key: BindingKey, value: crate::store::ErasedInstance) {
        self.store.insert_erased(key, value);
    }

    /// Wraps a factory failure with source and the complete first root path.
    fn factory_error(&self, key: &BindingKey, definition: DefinitionSource, error: FactoryError) -> BuildError {
        let path = self.paths.get(key).cloned().unwrap_or_else(|| vec![key.clone()]);
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
}
