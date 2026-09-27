// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Explicit registration and validated construction of shared components.

use std::collections::HashSet;
use std::panic::Location;
use std::sync::Arc;

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
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::managed::Managed;
use crate::managed::ManagedFactoryFuture;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;
mod internal;

use internal::Construction;

type PreparedDefinitions = (Vec<PendingDefinition>, Vec<String>, Vec<Dependency>);

/// A declaration that can install itself into a container builder.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ComponentDefinition;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::DefinitionSource;
/// use qubit_ioc::RegistrationError;
///
/// struct Message;
/// impl ComponentDefinition for Message {
///     fn source() -> DefinitionSource {
///         DefinitionSource::new("app", "app", "src/main.rs", 1, 1, "Message")
///     }
///     fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
///         builder.register_instance(Arc::new(Message))
///     }
/// }
///
/// let mut builder = ContainerBuilder::new();
/// builder.install::<Message>()?;
/// assert!(builder.build_all()?.get::<Message>().is_ok());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub trait ComponentDefinition {
    /// Returns this definition's diagnostic source location.
    fn source() -> DefinitionSource;

    /// Registers this definition into `builder`.
    ///
    /// # Errors
    ///
    /// Returns the registration error produced when one of the definition's
    /// keys, options, or declared requests is invalid.
    // Keep the public error's full key and source fields in generated definitions.
    #[allow(clippy::result_large_err)]
    fn register(builder: &mut ContainerBuilder) -> Result<(), RegistrationError>;
}

/// Collects component definitions without running their factories.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ContainerBuilder;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance(Arc::new(42_u32))?;
/// let context = builder.build_all()?;
/// assert_eq!(*context.get::<u32>()?, 42);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

/// One explicit definition override and the definition that supplied it.
struct Replacement {
    /// Exact typed key identifying the earlier definition to remove.
    anchor: BindingKey,
    /// Index of the staged definition that supplies the replacement.
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

/// Reports the caller's file location and marks unavailable package/module
/// data.
///
/// The type name becomes the diagnostic item name; package and module are
/// placeholders because explicit registrations have no generated source.
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
///
/// The caller location points to the public profile-selection call.
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
