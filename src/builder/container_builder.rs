// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Builder state and operations for explicit component registration.

use std::sync::Arc;

use super::ComponentDefinition;
use super::internal::Construction;
use super::internal::replacement::Replacement;
use super::internal::validation::profile_source;
use super::internal::validation::source;
use super::internal::validation::valid_profile;
use super::internal::validation::validate_dependencies;
use super::internal::validation::validate_options;
use crate::FactoryFuture;
use crate::application_context::ApplicationContext;
use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::build_context::BuildContext;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::error::FactoryError;
use crate::error::RegistrationError;
use crate::graph::ValidatedGraph;
use crate::key::BindingKey;
use crate::managed::Managed;
use crate::managed::ManagedFactoryFuture;
use crate::options::BindingOptions;

#[cfg(feature = "config")]
mod config;

type PreparedDefinitions = (Vec<PendingDefinition>, Vec<String>, Vec<Dependency>);

/// Registers component definitions and constructs a validated application.
///
/// The builder collects explicit registrations, validates their dependency
/// graph, and creates only the components selected for a build.
#[derive(Default)]
pub struct ContainerBuilder {
    /// Definitions staged by generated and explicit registration calls.
    pub(crate) definitions: Vec<PendingDefinition>,
    /// Profiles whose definitions are active during graph validation.
    active_profiles: Vec<String>,
    /// Required requests that select the subgraph for a root-scoped build.
    roots: Vec<Dependency>,
    /// Whole-definition replacements applied after profile filtering.
    replacements: Vec<Replacement>,
}

// Public structured registration/build errors retain complete paths and
// sources.
#[allow(clippy::result_large_err)]
impl ContainerBuilder {
    /// Creates an empty container builder with the `default` profile active.
    ///
    /// # Returns
    ///
    /// An empty builder ready to receive component definitions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects `T` as a required root for [`Self::build`] or
    /// [`Self::build_async`].
    ///
    /// Roots are resolved against every active binding before the dependency
    /// graph is reduced to the selected components. Repeating the same request
    /// keeps its first registration position.
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type whose dependency closure should be built.
    pub fn root<T: ?Sized + 'static>(&mut self) {
        let request = Dependency::of::<T>();
        if !self.roots.contains(&request) {
            self.roots.push(request);
        }
    }

    /// Selects the `T` binding with the exact `id` as a required build root.
    ///
    /// The ID is validated at registration time. Repeating the same request
    /// keeps its first registration position.
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type whose exact-ID binding should be built.
    ///
    /// # Parameters
    ///
    /// `id` is the exact identifier selected as a required root.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after the request is added, or when it was already
    /// present.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError::InvalidBindingId`] when `id` does not
    /// match the binding ID grammar.
    #[track_caller]
    pub fn root_by_id<T: ?Sized + 'static>(&mut self, id: &str) -> Result<(), RegistrationError> {
        let request = Dependency::with_id::<T>(id);
        validate_dependencies(std::slice::from_ref(&request), source::<T>())?;
        if !self.roots.contains(&request) {
            self.roots.push(request);
        }
        Ok(())
    }

    /// Stages a complete shared instance with default binding options.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type and must be
    /// thread-safe and `'static` for storage in the context.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component to stage under default options.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the instance.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] if the generated definition is invalid.
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
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type and must be
    /// thread-safe and `'static` for storage in the context.
    ///
    /// # Parameters
    ///
    /// * `value` - Shared component to stage.
    /// * `options` - ID, profile, primary selection, and collection order.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the instance.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid options or a duplicate key.
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
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type returned by the factory. `F` is a sendable,
    /// one-shot factory that returns a shared `T` or [`FactoryError`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests the factory may read during construction.
    /// * `factory` - One-shot closure that creates the shared component.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the factory.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid requests or default options.
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
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type returned by the factory. `F` is a sendable,
    /// one-shot factory that returns a shared `T` or [`FactoryError`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests the factory may read during construction.
    /// * `options` - ID, profile, primary selection, and collection order.
    /// * `factory` - One-shot closure that creates the shared component.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the factory.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid options or duplicate requests.
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

    /// Stages a one-shot managed synchronous factory with default options.
    ///
    /// The factory is invoked only during a successful synchronous build, after
    /// graph validation. Registering it does not create the managed resource.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `factory` - One-shot closure returning the value and its shutdown
    ///   actions.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` is a sendable one-shot factory
    /// receiving a [`BuildContext`] and returning [`Managed<T>`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] when a request or binding option is
    /// invalid.
    #[track_caller]
    pub fn register_managed_factory<T, F>(
        &mut self,
        dependencies: &[Dependency],
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Managed<T>, FactoryError> + Send + 'static,
    {
        self.register_managed_factory_with(dependencies, BindingOptions::default(), factory)
    }

    /// Stages a one-shot managed synchronous factory with binding options.
    ///
    /// The factory is invoked only during a successful synchronous build, after
    /// graph validation. Registering it does not create the managed resource.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `options` - Binding ID, profile, and alias selection metadata.
    /// * `factory` - One-shot closure returning the value and its shutdown
    ///   actions.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` is a sendable one-shot factory
    /// receiving a [`BuildContext`] and returning [`Managed<T>`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] when a request or binding option is
    /// invalid.
    #[track_caller]
    pub fn register_managed_factory_with<T, F>(
        &mut self,
        dependencies: &[Dependency],
        options: BindingOptions,
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Managed<T>, FactoryError> + Send + 'static,
    {
        let source = source::<T>();
        let (key, profile) = validate_options::<T>(&options, source)?;
        validate_dependencies(dependencies, source)?;
        let binding = PendingBinding::managed_sync_factory(key, options.primary, options.order, factory);
        self.stage_definition(PendingDefinition::new(source, profile, dependencies.to_vec(), binding))
    }

    /// Stages a one-shot asynchronous factory with default binding options.
    ///
    /// Its returned future owns its data, is `Send`, and is driven only by
    /// [`Self::build_async`]; no runtime is chosen by this crate.
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type returned by the future. `F` is a sendable,
    /// one-shot factory that creates that future.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests the factory may read during construction.
    /// * `factory` - One-shot closure that creates the component future.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the factory.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid requests or default options.
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
    ///
    /// # Type Parameters
    ///
    /// `T` is the component type returned by the future. `F` is a sendable,
    /// one-shot factory that creates that future.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests the factory may read during construction.
    /// * `options` - ID, profile, primary selection, and collection order.
    /// * `factory` - One-shot closure that creates the component future.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the factory.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid options or duplicate requests.
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

    /// Stages a managed asynchronous factory with default options.
    ///
    /// The factory is invoked only by [`Self::build_async`] after graph
    /// validation. Registration does not create the resource, and this crate
    /// does not select an async runtime.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `factory` - One-shot closure creating a managed factory future.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` creates a sendable future
    /// yielding [`Managed<T>`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] when a request or binding option is
    /// invalid.
    #[track_caller]
    pub fn register_managed_async_factory<T, F>(
        &mut self,
        dependencies: &[Dependency],
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> ManagedFactoryFuture<T> + Send + 'static,
    {
        self.register_managed_async_factory_with(dependencies, BindingOptions::default(), factory)
    }

    /// Stages a managed asynchronous factory with binding options.
    ///
    /// The factory is invoked only by [`Self::build_async`] after graph
    /// validation. Registration does not create the resource, and this crate
    /// does not select an async runtime.
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `options` - Binding ID, profile, and alias selection metadata.
    /// * `factory` - One-shot closure creating a managed factory future.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` creates a sendable future
    /// yielding [`Managed<T>`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] when a request or binding option is
    /// invalid.
    #[track_caller]
    pub fn register_managed_async_factory_with<T, F>(
        &mut self,
        dependencies: &[Dependency],
        options: BindingOptions,
        factory: F,
    ) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> ManagedFactoryFuture<T> + Send + 'static,
    {
        let source = source::<T>();
        let (key, profile) = validate_options::<T>(&options, source)?;
        validate_dependencies(dependencies, source)?;
        let binding = PendingBinding::managed_async_factory(key, options.primary, options.order, factory);
        self.stage_definition(PendingDefinition::new(source, profile, dependencies.to_vec(), binding))
    }

    /// Invokes a generated or handwritten definition registration entry.
    ///
    /// The definition's own registration errors are returned unchanged.
    ///
    /// # Type Parameters
    ///
    /// `D` is a generated or handwritten definition implementing
    /// [`ComponentDefinition`].
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after the definition is staged.
    ///
    /// # Errors
    ///
    /// Returns the registration error produced by the definition.
    pub fn install<D: ComponentDefinition>(&mut self) -> Result<(), RegistrationError> {
        D::register(self)
    }

    /// Replaces the complete active definition identified by `anchor`.
    ///
    /// The callback registers into a temporary builder, so errors leave this
    /// builder unchanged. It must stage exactly one definition containing
    /// `anchor` exactly once. At build time, the complete matching active
    /// definition is removed after profile filtering. Later duplicates still
    /// fail.
    ///
    /// # Parameters
    ///
    /// * `anchor` - Exact key identifying the active definition to replace.
    /// * `definition` - Callback that stages the replacement definition.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the complete replacement.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] if the callback fails, stages other than
    /// one definition, or does not declare `anchor` exactly once.
    pub fn replace_definition<F>(&mut self, anchor: BindingKey, definition: F) -> Result<(), RegistrationError>
    where
        F: FnOnce(&mut Self) -> Result<(), RegistrationError>,
    {
        let mut draft = Self::new();
        definition(&mut draft)?;
        if draft.definitions.len() != 1 {
            return Err(RegistrationError::ReplacementDefinitionCount {
                count: draft.definitions.len(),
            });
        }
        let definition = &draft.definitions[0];
        let anchor_count = definition
            .bindings
            .iter()
            .filter(|binding| binding.key == anchor)
            .count();
        if anchor_count != 1 {
            return Err(RegistrationError::ReplacementAnchorCount {
                anchor,
                count: anchor_count,
            });
        }
        let relative_index = 0;
        let definition_index = self.definitions.len() + relative_index;
        self.definitions.extend(draft.definitions);
        self.replacements.push(Replacement {
            anchor,
            definition_index,
        });
        Ok(())
    }

    /// Replaces the active profile set for this builder.
    ///
    /// Each profile must match `[A-Za-z][A-Za-z0-9_-]*`; an empty slice
    /// restores the default profile. Invalid input leaves the consumed
    /// builder unavailable.
    ///
    /// # Parameters
    ///
    /// `profiles` lists the profiles to activate for subsequent builds.
    ///
    /// # Returns
    ///
    /// The builder with the new active profile set.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError::InvalidProfile`] if any profile is invalid.
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

    /// Builds the components selected by one or more calls to [`Self::root`]
    /// or [`Self::root_by_id`].
    ///
    /// An active asynchronous factory yields `AsyncRequired` before any factory
    /// runs. Factory failures retain their source and no partial context
    /// escapes.
    ///
    /// # Returns
    ///
    /// The context containing the selected dependency closure.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] for missing roots, invalid graphs, asynchronous
    /// factories, or construction failures.
    pub fn build(self) -> Result<ApplicationContext, BuildError> {
        if self.roots.is_empty() {
            return Err(BuildError::NoRootsSelected);
        }
        let (definitions, profiles, roots) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots(definitions, &profiles, Some(&roots))?;
        for location in &graph.order {
            let definition = &graph.definitions[location.definition];
            let binding = &definition.bindings[location.binding];
            if matches!(
                binding.kind,
                PendingBindingKind::AsyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_)
            ) {
                return Err(BuildError::AsyncRequired {
                    definition: definition.source,
                    key: binding.key.clone(),
                });
            }
        }
        Construction::new(graph).run_sync()
    }

    /// Validates and constructs every definition active under the configured
    /// profiles, whether or not a root was registered.
    ///
    /// Prefer [`Self::build`] when the application only needs a subset of
    /// discovered definitions.
    ///
    /// # Returns
    ///
    /// The context containing every active definition.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] for invalid graphs, asynchronous factories, or
    /// construction failures.
    pub fn build_all(self) -> Result<ApplicationContext, BuildError> {
        let (definitions, profiles, _) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots(definitions, &profiles, None)?;
        for location in &graph.order {
            let definition = &graph.definitions[location.definition];
            let binding = &definition.bindings[location.binding];
            if matches!(
                binding.kind,
                PendingBindingKind::AsyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_)
            ) {
                return Err(BuildError::AsyncRequired {
                    definition: definition.source,
                    key: binding.key.clone(),
                });
            }
        }
        Construction::new(graph).run_sync()
    }

    /// Builds the components selected by one or more calls to [`Self::root`]
    /// or [`Self::root_by_id`], including their transitive dependencies.
    /// The future is `Send` and uses the caller's executor.
    ///
    /// Dropping it stops unstarted factories; completed external side effects
    /// remain the factory's responsibility. A failure publishes no context.
    ///
    /// # Returns
    ///
    /// A future that resolves to the context containing the selected closure.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] for missing roots, invalid graphs, or factory
    /// failures. Managed cleanup failures are retained with the build error.
    pub async fn build_async(self) -> Result<ApplicationContext, BuildError> {
        if self.roots.is_empty() {
            return Err(BuildError::NoRootsSelected);
        }
        let (definitions, profiles, roots) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots(definitions, &profiles, Some(&roots))?;
        Construction::new(graph).run_async().await
    }

    /// Validates and asynchronously constructs every definition active under
    /// the configured profiles, whether or not a root was registered.
    ///
    /// # Returns
    ///
    /// A future that resolves to the context containing every active
    /// definition.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError`] for invalid graphs or factory failures. Managed
    /// cleanup failures are retained with the build error.
    pub async fn build_all_async(self) -> Result<ApplicationContext, BuildError> {
        let (definitions, profiles, _) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots(definitions, &profiles, None)?;
        Construction::new(graph).run_async().await
    }

    /// Filters profiles, then applies each exact-key override before graph
    /// validation.
    fn prepare_definitions(self) -> Result<PreparedDefinitions, BuildError> {
        let Self {
            definitions,
            active_profiles,
            replacements,
            roots,
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
            let Some((_, replacement_definition)) = active.iter().find(|(index, definition)| {
                *index == replacement.definition_index
                    && definition
                        .bindings
                        .iter()
                        .any(|binding| binding.key == replacement.anchor)
            }) else {
                continue;
            };
            let replacement_source = replacement_definition.source;
            let originals: Vec<_> = active
                .iter()
                .filter(|(index, _)| *index < replacement.definition_index)
                .flat_map(|(_, definition)| {
                    definition
                        .bindings
                        .iter()
                        .filter(|binding| binding.key == replacement.anchor)
                        .map(|_| definition.source)
                })
                .collect();
            match originals.as_slice() {
                [] => {
                    return Err(BuildError::ReplacementOriginalMissing {
                        key: replacement.anchor,
                        replacement: replacement_source,
                    });
                }
                [_] => {}
                _ => {
                    return Err(BuildError::ReplacementOriginalAmbiguous {
                        key: replacement.anchor,
                        originals,
                        replacement: replacement_source,
                    });
                }
            }
            let original_index = active.iter().find_map(|(index, definition)| {
                (*index < replacement.definition_index
                    && definition
                        .bindings
                        .iter()
                        .any(|binding| binding.key == replacement.anchor))
                .then_some(*index)
            });
            let Some(original_index) = original_index else { continue };
            let original_sources = active
                .iter()
                .find(|(index, _)| *index == original_index)
                .and_then(|(_, item)| {
                    item.bindings
                        .iter()
                        .find(|binding| binding.key == replacement.anchor)
                        .map(|binding| {
                            let mut sources = binding.replaced_sources.clone();
                            sources.push(item.source);
                            sources
                        })
                })
                .unwrap_or_default();
            active.retain(|(index, _)| *index != original_index);
            if let Some((_, definition)) = active
                .iter_mut()
                .find(|(index, _)| *index == replacement.definition_index)
                && let Some(binding) = definition
                    .bindings
                    .iter_mut()
                    .find(|binding| binding.key == replacement.anchor)
            {
                binding.replaced_sources.extend(original_sources);
            }
        }
        Ok((
            active
                .into_iter()
                .filter_map(|(_, definition)| (!definition.bindings.is_empty()).then_some(definition))
                .collect(),
            active_profiles,
            roots,
        ))
    }

    /// Atomically adds an already validated complete definition to the staging
    /// area.
    pub(crate) fn stage_definition(&mut self, definition: PendingDefinition) -> Result<(), RegistrationError> {
        definition.stage_into(&mut self.definitions)
    }
}
