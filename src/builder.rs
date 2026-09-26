// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit registration and validated construction of shared components.

use std::collections::HashMap;
use std::collections::HashSet;
use std::panic::Location;
use std::sync::Arc;
use std::sync::RwLock;

use crate::FactoryFuture;
use crate::application_context::ApplicationContext;
use crate::application_context::BuiltBinding;
use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::build_context::BuildContext;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::error::FactoryError;
use crate::error::RegistrationError;
use crate::graph::BindingLocation;
use crate::graph::ResolvedDependency;
use crate::graph::ValidatedGraph;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// A declaration that can install itself into a container builder.
pub trait ComponentDefinition {
    /// Returns this definition's diagnostic source location.
    ///
    /// Generated definitions use the same value in their discovery entry.
    fn source() -> DefinitionSource;

    /// Returns a stable, unique identity for this definition.
    ///
    /// Generated definitions use the same ID in their discovery entry. The ID
    /// must distinguish different definitions even if their source locations
    /// coincide; a package, module and item name combination is recommended.
    fn definition_id() -> &'static str;

    /// Registers the definition, returning a structured registration error on
    /// invalid input.
    // Keep the public error's full key and source fields in generated definitions.
    #[allow(clippy::result_large_err)]
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError>;
}

/// Collects component definitions without running their factories.
#[derive(Default)]
pub struct ContainerBuilder {
    pub(crate) definitions: Vec<PendingDefinition>,
    active_profiles: Vec<String>,
    pub(crate) excluded_definitions: Vec<&'static str>,
    replacements: Vec<Replacement>,
}

/// One explicit exact-key override and the definition that supplied it.
struct Replacement {
    key: BindingKey,
    definition_index: usize,
}

// Public structured registration/build errors retain complete paths and
// sources.
#[allow(clippy::result_large_err)]
impl ContainerBuilder {
    /// Creates an empty container builder with the `default` profile active.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stages a complete shared instance with default binding options.
    #[track_caller]
    pub fn register_instance<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        value: Arc<T>,
    ) -> Result<(), RegistrationError> {
        self.register_instance_with(value, BindingOptions::default())
    }

    /// Stages a complete shared instance of `T` with binding options.
    ///
    /// Invalid IDs or profiles return a registration error with caller
    /// location. Cross-definition collisions are checked after profile
    /// filtering at build.
    #[track_caller]
    pub fn register_instance_with<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        value: Arc<T>,
        options: BindingOptions,
    ) -> Result<(), RegistrationError> {
        let source = source::<T>();
        let (key, profile) = validate_options::<T>(&options, source)?;
        let binding = PendingBinding::instance(key, value, options.primary, options.order);
        self.stage_definition(PendingDefinition::new(source, profile, Vec::new(), binding))
    }

    /// Stages a one-shot synchronous factory with default binding options.
    ///
    /// The factory receives only its declared requests during construction.
    #[track_caller]
    pub fn register_factory<T, F>(&mut self, dependencies: &[Dependency], factory: F) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        self.register_factory_with(dependencies, BindingOptions::default(), factory)
    }

    /// Stages a one-shot synchronous factory of `T` with binding options.
    ///
    /// Duplicate requests or invalid IDs and profiles fail at registration;
    /// the closure itself runs only after the complete graph validates.
    #[track_caller]
    pub fn register_factory_with<T, F>(
        &mut self,
        dependencies: &[Dependency],
        options: BindingOptions,
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        let source = source::<T>();
        let (key, profile) = validate_options::<T>(&options, source)?;
        validate_dependencies(dependencies, source)?;
        let binding = PendingBinding::sync_factory(key, options.primary, options.order, factory);
        self.stage_definition(PendingDefinition::new(source, profile, dependencies.to_vec(), binding))
    }

    /// Stages a one-shot asynchronous factory with default binding options.
    ///
    /// Its returned future owns its data, is `Send`, and is driven only by
    /// [`Self::build_async`]; no runtime is chosen by this crate.
    #[track_caller]
    pub fn register_async_factory<T, F>(
        &mut self,
        dependencies: &[Dependency],
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
    {
        self.register_async_factory_with(dependencies, BindingOptions::default(), factory)
    }

    /// Stages an asynchronous factory of `T` with binding options.
    ///
    /// Invalid options and duplicate requests fail before any factory executes.
    #[track_caller]
    pub fn register_async_factory_with<T, F>(
        &mut self,
        dependencies: &[Dependency],
        options: BindingOptions,
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
    {
        let source = source::<T>();
        let (key, profile) = validate_options::<T>(&options, source)?;
        validate_dependencies(dependencies, source)?;
        let binding = PendingBinding::async_factory(key, options.primary, options.order, factory);
        self.stage_definition(PendingDefinition::new(source, profile, dependencies.to_vec(), binding))
    }

    /// Invokes a generated or handwritten definition registration entry.
    ///
    /// The definition's own registration errors are returned unchanged.
    pub fn install<D: ComponentDefinition>(&mut self) -> Result<(), RegistrationError> {
        D::register(self)
    }

    /// Skips the linked registration entry for `D` during later discovery.
    ///
    /// Every binding declared by that entry, including interface aliases, is
    /// skipped. Explicit registrations already staged in this builder remain.
    pub fn exclude_definition<D: ComponentDefinition>(&mut self) {
        self.excluded_definitions.push(D::definition_id());
    }

    /// Stages `definition` as an explicit replacement for exactly `key`.
    ///
    /// The callback registers into a temporary builder, so errors leave this
    /// builder unchanged. It must declare `key` exactly once. At build time,
    /// the matching active binding from earlier definitions is removed after
    /// profile filtering; their other keys remain. Later duplicates still fail.
    pub fn replace_binding(
        &mut self,
        key: BindingKey,
        definition: fn(&mut Self) -> Result<(), RegistrationError>,
    ) -> Result<(), RegistrationError> {
        let mut draft = Self::new();
        definition(&mut draft)?;
        let matches: Vec<_> = draft
            .definitions
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                item.bindings
                    .iter()
                    .any(|binding| binding.key == key)
                    .then_some((index, item.source))
            })
            .collect();
        let relative_index = match matches.as_slice() {
            [(index, _)] => *index,
            [] => return Err(RegistrationError::ReplacementTargetMissing { key }),
            _ => {
                return Err(RegistrationError::ReplacementTargetAmbiguous {
                    key,
                    sources: matches.into_iter().map(|(_, source)| source).collect(),
                });
            }
        };
        let definition_index = self.definitions.len() + relative_index;
        self.definitions.extend(draft.definitions);
        self.replacements.push(Replacement { key, definition_index });
        Ok(())
    }

    /// Replaces the active profile set for this builder.
    ///
    /// Each profile must match `[A-Za-z][A-Za-z0-9_-]*`; an empty slice
    /// restores the default profile. Invalid input leaves the consumed
    /// builder unavailable.
    #[track_caller]
    pub fn active_profiles(mut self, profiles: &[&str]) -> Result<Self, RegistrationError> {
        for profile in profiles {
            if !valid_profile(profile) {
                return Err(RegistrationError::InvalidProfile {
                    value: (*profile).to_owned(),
                    definition: profile_source(),
                });
            }
        }
        self.active_profiles = profiles.iter().map(|profile| (*profile).to_owned()).collect();
        Ok(self)
    }

    /// Validates the full graph, then runs only synchronous factories in order.
    ///
    /// An active asynchronous factory yields `AsyncRequired` before any factory
    /// runs. Factory failures retain their source and no partial context
    /// escapes.
    pub fn build(self) -> Result<ApplicationContext, BuildError> {
        let (definitions, profiles) = self.prepare_definitions();
        let graph = ValidatedGraph::validate(definitions, &profiles)?;
        for definition in &graph.definitions {
            for binding in &definition.bindings {
                if matches!(binding.kind, PendingBindingKind::AsyncFactory(_)) {
                    return Err(BuildError::AsyncRequired {
                        definition: definition.source,
                        key: binding.key.clone(),
                    });
                }
            }
        }
        Construction::new(graph).run_sync()
    }

    /// Validates the full graph, then serially runs synchronous and
    /// asynchronous factories. The future is `Send` and uses the caller's
    /// executor.
    ///
    /// Dropping it stops unstarted factories; completed external side effects
    /// remain the factory's responsibility. A failure publishes no context.
    pub async fn build_async(self) -> Result<ApplicationContext, BuildError> {
        let (definitions, profiles) = self.prepare_definitions();
        let graph = ValidatedGraph::validate(definitions, &profiles)?;
        Construction::new(graph).run_async().await
    }

    /// Filters profiles, then applies each exact-key override before graph
    /// validation.
    fn prepare_definitions(self) -> (Vec<PendingDefinition>, Vec<String>) {
        let Self {
            definitions,
            active_profiles,
            replacements,
            ..
        } = self;
        let mut active: Vec<_> = definitions
            .into_iter()
            .enumerate()
            .filter(|(_, definition)| {
                definition.profile.as_deref().is_none_or(|profile| {
                    if active_profiles.is_empty() {
                        profile == "default"
                    } else {
                        active_profiles.iter().any(|active| active == profile)
                    }
                })
            })
            .collect();
        for replacement in replacements {
            // An inactive replacement does not affect bindings in active profiles.
            if !active.iter().any(|(index, _)| *index == replacement.definition_index) {
                continue;
            }
            let mut replaced_sources = Vec::new();
            for (index, definition) in &mut active {
                if *index < replacement.definition_index {
                    definition.bindings.retain(|binding| {
                        if binding.key == replacement.key {
                            replaced_sources.extend(&binding.replaced_sources);
                            replaced_sources.push(definition.source);
                            false
                        } else {
                            true
                        }
                    });
                }
            }
            if let Some((_, definition)) = active
                .iter_mut()
                .find(|(index, _)| *index == replacement.definition_index)
                && let Some(binding) = definition
                    .bindings
                    .iter_mut()
                    .find(|binding| binding.key == replacement.key)
            {
                binding.replaced_sources.extend(replaced_sources);
            }
        }
        (
            active
                .into_iter()
                .filter_map(|(_, definition)| (!definition.bindings.is_empty()).then_some(definition))
                .collect(),
            active_profiles,
        )
    }

    /// Atomically adds an already validated complete definition to the staging
    /// area.
    pub(crate) fn stage_definition(&mut self, definition: PendingDefinition) -> Result<(), RegistrationError> {
        definition.stage_into(&mut self.definitions)
    }
}

/// Reports the caller's file location and marks unavailable package/module
/// data.
#[track_caller]
fn source<T: ?Sized + 'static>() -> DefinitionSource {
    let location = Location::caller();
    DefinitionSource::new(
        "<explicit>",
        "<explicit>",
        location.file(),
        location.line(),
        location.column(),
        std::any::type_name::<T>(),
    )
}

/// Records source location for builder-level profile input.
#[track_caller]
fn profile_source() -> DefinitionSource {
    let location = Location::caller();
    DefinitionSource::new(
        "<explicit>",
        "<explicit>",
        location.file(),
        location.line(),
        location.column(),
        "active_profiles",
    )
}

/// Validates options and returns a typed key plus the optional profile.
// Keep the public RegistrationError's source and key fields intact.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_options<T: ?Sized + 'static>(
    options: &BindingOptions,
    definition: DefinitionSource,
) -> Result<(BindingKey, Option<String>), RegistrationError> {
    let id = options
        .id
        .as_deref()
        .map(BindingId::parse)
        .transpose()
        .map_err(|error| RegistrationError::InvalidBindingId { error, definition })?;
    if let Some(profile) = &options.profile
        && !valid_profile(profile)
    {
        return Err(RegistrationError::InvalidProfile {
            value: profile.clone(),
            definition,
        });
    }
    Ok((BindingKey::of::<T>(id), options.profile.clone()))
}

/// Checks one profile identifier against the documented ASCII grammar.
fn valid_profile(profile: &str) -> bool {
    let mut bytes = profile.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Rejects malformed IDs and exact duplicate requests before staging a factory.
// Keep the public RegistrationError's source and request fields intact.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_dependencies(
    dependencies: &[Dependency],
    definition: DefinitionSource,
) -> Result<(), RegistrationError> {
    let mut seen = HashSet::with_capacity(dependencies.len());
    for dependency in dependencies {
        if let Some(id) = dependency.id() {
            BindingId::parse(id).map_err(|error| RegistrationError::InvalidBindingId { error, definition })?;
        }
        if !seen.insert(dependency) {
            return Err(RegistrationError::DuplicateDependency {
                dependency: dependency.clone(),
                definition,
            });
        }
    }
    Ok(())
}

/// Owns a validated graph and its private, partially populated store.
struct Construction {
    sources: Vec<DefinitionSource>,
    actions: Vec<Vec<Option<PendingBinding>>>,
    order: Vec<BindingLocation>,
    resolved: Vec<Vec<ResolvedDependency>>,
    paths: HashMap<BindingKey, Vec<BindingKey>>,
    bindings: Vec<BuiltBinding>,
    store: Arc<RwLock<InstanceStore>>,
}

// BuildError keeps full dependency paths and original factory sources.
#[allow(clippy::result_large_err)]
impl Construction {
    /// Retains lookup and diagnostic metadata before consuming one-shot
    /// bindings.
    fn new(graph: ValidatedGraph) -> Self {
        let paths = paths_to_all(&graph.definitions, &graph.resolved);
        let bindings = graph
            .definitions
            .iter()
            .flat_map(|definition| {
                definition.bindings.iter().map(|binding| BuiltBinding {
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
            store: Arc::new(RwLock::new(InstanceStore::default())),
        }
    }

    /// Runs every sync binding after the graph and async preflight have
    /// succeeded.
    fn run_sync(mut self) -> Result<ApplicationContext, BuildError> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let value = match kind {
                PendingBindingKind::Instance(value) => value,
                PendingBindingKind::SyncFactory(factory) => factory(self.build_context(location.definition))
                    .map_err(|error| self.factory_error(&key, source, error))?,
                PendingBindingKind::AsyncFactory(_) => {
                    return Err(BuildError::AsyncRequired {
                        definition: source,
                        key,
                    });
                }
                PendingBindingKind::Alias { target, project } => self.project(&key, source, &target, project)?,
            };
            self.insert(key, value);
        }
        Ok(ApplicationContext::new(self.store, self.bindings))
    }

    /// Drives each async factory to completion before starting the next
    /// binding.
    async fn run_async(mut self) -> Result<ApplicationContext, BuildError> {
        for location in std::mem::take(&mut self.order) {
            let (key, source, kind) = self.take_binding(location);
            let value = match kind {
                PendingBindingKind::Instance(value) => value,
                PendingBindingKind::SyncFactory(factory) => factory(self.build_context(location.definition))
                    .map_err(|error| self.factory_error(&key, source, error))?,
                PendingBindingKind::AsyncFactory(factory) => factory(self.build_context(location.definition))
                    .await
                    .map_err(|error| self.factory_error(&key, source, error))?,
                PendingBindingKind::Alias { target, project } => self.project(&key, source, &target, project)?,
            };
            self.insert(key, value);
        }
        Ok(ApplicationContext::new(self.store, self.bindings))
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
        BuildContext::new(Arc::clone(&self.store), source, resolved)
    }

    /// Projects one alias from its already constructed concrete binding.
    fn project(
        &self,
        key: &BindingKey,
        source: DefinitionSource,
        target: &BindingKey,
        project: crate::binding::AliasProjector,
    ) -> Result<crate::store::ErasedInstance, BuildError> {
        let store = self.store.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        let value = store.get_erased(target).and_then(project);
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
        self.store
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert_erased(key, value);
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

/// Precomputes a stable root-to-binding path for every active binding.
fn paths_to_all(
    definitions: &[PendingDefinition],
    resolved: &[Vec<ResolvedDependency>],
) -> HashMap<BindingKey, Vec<BindingKey>> {
    let mut targets = HashSet::new();
    for (definition_index, definition) in definitions.iter().enumerate() {
        for binding in &definition.bindings {
            if let PendingBindingKind::Alias { target, .. } = &binding.kind {
                targets.insert(target.clone());
            } else {
                for dependency in &resolved[definition_index] {
                    targets.extend(dependency.keys.iter().cloned());
                }
            }
        }
    }
    let mut paths = HashMap::new();
    for definition in definitions {
        for binding in &definition.bindings {
            if !targets.contains(&binding.key) {
                trace_paths(&binding.key, definitions, resolved, &mut Vec::new(), &mut paths);
            }
        }
    }
    for definition in definitions {
        for binding in &definition.bindings {
            if !paths.contains_key(&binding.key) {
                trace_paths(&binding.key, definitions, resolved, &mut Vec::new(), &mut paths);
            }
        }
    }
    paths
}

/// Visits a validated acyclic dependency path, preserving its first root.
fn trace_paths(
    key: &BindingKey,
    definitions: &[PendingDefinition],
    resolved: &[Vec<ResolvedDependency>],
    path: &mut Vec<BindingKey>,
    paths: &mut HashMap<BindingKey, Vec<BindingKey>>,
) {
    if paths.contains_key(key) {
        return;
    }
    path.push(key.clone());
    paths.insert(key.clone(), path.clone());
    for (definition_index, definition) in definitions.iter().enumerate() {
        if let Some(binding) = definition.bindings.iter().find(|binding| &binding.key == key) {
            match &binding.kind {
                PendingBindingKind::Alias { target, .. } => {
                    trace_paths(target, definitions, resolved, path, paths);
                }
                _ => {
                    for dependency in &resolved[definition_index] {
                        for target in &dependency.keys {
                            trace_paths(target, definitions, resolved, path, paths);
                        }
                    }
                }
            }
            break;
        }
    }
    path.pop();
}
