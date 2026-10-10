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
use super::FactoryArgs;
use super::ValidationScope;
use super::internal::Construction;
use super::internal::replacement::Replacement;
use super::internal::validation::profile_source;
use super::internal::validation::source;
use super::internal::validation::valid_profile;
use super::internal::validation::validate_dependencies;
use crate::FactoryFuture;
use crate::application::Application;
use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::binding::profile_is_active;
use crate::build_context::BuildContext;
use crate::builder::BuildSession;
use crate::definition::Definition;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::error::BuildFailure;
use crate::error::FactoryError;
use crate::error::RegistrationError;
use crate::error::SettledBuildFailure;
use crate::graph::ValidatedGraph;
use crate::key::BindingKey;
use crate::managed::Managed;
use crate::managed::ManagedFactoryFuture;
use crate::managed::WaitPolicy;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;

// Implements configuration snapshot registration when integration is enabled.
#[cfg(feature = "config")]
mod config;

/// Active definitions after replacement, configured profiles, and build roots.
type PreparedDefinitions = (Vec<PendingDefinition>, Vec<String>, Vec<Dependency>);

/// Registers component definitions and constructs a validated application.
///
/// The builder collects explicit registrations, validates their dependency
/// graph, and creates only the components selected for a build.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ContainerBuilder;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance(Arc::new(7_u8))?;
/// builder.root::<u8>();
/// let application = builder.build()?;
/// let context = application.context();
/// assert_eq!(*context.get::<u8>()?, 7);
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
    /// Explicit lifecycle deadlines required for selected managed factories.
    wait_policy: Option<WaitPolicy>,
    /// Static validation scope for root-scoped builds.
    validation_scope: ValidationScope,
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
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures lifecycle deadlines for managed components.
    ///
    /// A selected managed factory requires an explicit policy before any
    /// factory can execute. Ordinary graphs do not require a policy.
    #[must_use]
    #[inline]
    pub fn wait_policy(mut self, policy: WaitPolicy) -> Self {
        self.wait_policy = Some(policy);
        self
    }

    /// Chooses the active definitions checked before a root-scoped build.
    ///
    /// The default is [`ValidationScope::Reachable`]. `AllActive` checks the
    /// entire active graph after profile filtering and replacement, then only
    /// constructs the requested root closure. It never runs factories outside
    /// that closure or requires their asynchronous build mode or wait policy.
    /// This setting does not change `build_all` or `build_all_async`.
    ///
    /// # Parameters
    ///
    /// `scope` selects the static validation range.
    ///
    /// # Returns
    ///
    /// The configured builder.
    #[must_use]
    #[inline]
    pub fn validation_scope(mut self, scope: ValidationScope) -> Self {
        self.validation_scope = scope;
        self
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
    /// Returns [`RegistrationError`] when `options` contains an invalid binding
    /// ID or profile. A duplicate binding in another active definition is
    /// reported by [`BuildError::DuplicateBinding`] during build validation.
    #[track_caller]
    pub fn register_instance_with<T: ?Sized + Send + Sync + 'static>(
        &mut self,
        value: Arc<T>,
        options: BindingOptions,
    ) -> Result<(), RegistrationError> {
        self.register_definition(Definition::<T>::builder().binding(options).instance(value).build()?)
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

    /// Stages a synchronous factory whose typed parameter tuple also declares
    /// its graph dependencies.
    ///
    /// Supported arguments are `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`;
    /// tuples support up to eight parameters. Repeated identical requests are
    /// declared once while every tuple position receives its own shared
    /// handle. Use [`Self::register_factory`] when the factory needs named
    /// requests or a custom dependency view.
    ///
    /// # Type Parameters
    ///
    /// * `T` - component type produced by the factory.
    /// * `A` - typed argument tuple implementing [`FactoryArgs`].
    /// * `F` - one-shot function from the resolved tuple to a shared component.
    ///
    /// # Parameters
    ///
    /// * `factory` - closure receiving exactly the arguments described by `A`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid generated requests or binding
    /// options. Missing and ambiguous requests are returned later by `build`.
    #[track_caller]
    pub fn register_injected_factory<T, A, F>(&mut self, factory: F) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        A: FactoryArgs,
        F: FnOnce(A) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        let dependencies = A::dependencies();
        self.register_factory::<T, _>(&dependencies, move |context| {
            let arguments = A::resolve(&context).map_err(FactoryError::new)?;
            factory(arguments)
        })
    }

    /// Stages an asynchronous factory whose typed parameter tuple also
    /// declares its graph dependencies.
    ///
    /// Supported arguments are `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`;
    /// tuples support up to eight parameters. Missing and ambiguous requests
    /// are reported during build validation, before this factory runs. Use
    /// [`Self::register_async_factory`] when the factory needs named IDs or a
    /// custom dependency view. Build with [`Self::build_async`] or
    /// [`Self::build_all_async`]. Dropping a build future stops later factories
    /// but cannot undo side effects performed by a running user factory.
    ///
    /// # Type Parameters
    ///
    /// * `T` - Component type produced by the factory.
    /// * `A` - Typed argument tuple implementing [`FactoryArgs`].
    /// * `F` - Sendable one-shot function creating the component future.
    ///
    /// # Parameters
    ///
    /// * `factory` - Closure receiving exactly the arguments described by `A`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid generated requests or binding
    /// options. Missing and ambiguous dependencies are reported later by build.
    #[track_caller]
    pub fn register_injected_async_factory<T, A, F>(&mut self, factory: F) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        A: FactoryArgs,
        F: FnOnce(A) -> FactoryFuture<T> + Send + 'static,
    {
        let dependencies = A::dependencies();
        self.register_async_factory::<T, _>(&dependencies, move |context| {
            match A::resolve(&context).map_err(FactoryError::new) {
                Ok(arguments) => factory(arguments),
                Err(error) => Box::pin(async move { Err(error) }),
            }
        })
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
        self.register_definition(
            Definition::<T>::builder()
                .binding(options)
                .dependencies(dependencies)
                .factory(factory)
                .build()?,
        )
    }

    /// Stages a one-shot managed synchronous factory with default options.
    ///
    /// Registering it does not create the managed resource.
    /// After graph validation, a selected factory may run during either
    /// synchronous or asynchronous construction. A later factory may still
    /// fail the build; completed managed resources then participate in
    /// rollback.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` is a sendable one-shot factory
    /// receiving a [`BuildContext`] and returning [`Managed<T>`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `factory` - One-shot closure returning the value and its shutdown
    ///   actions.
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

    /// Stages a managed synchronous factory whose typed parameter tuple also
    /// declares its graph dependencies.
    ///
    /// Argument types and tuple limits match
    /// [`Self::register_injected_factory`]. The resulting `Managed<T>`
    /// enters the same rollback and shutdown path as a factory registered
    /// through [`Self::register_managed_factory`].
    ///
    /// # Type Parameters
    ///
    /// * `T` - managed component type.
    /// * `A` - typed argument tuple implementing [`FactoryArgs`].
    /// * `F` - one-shot function from the resolved tuple to `Managed<T>`.
    ///
    /// # Parameters
    ///
    /// * `factory` - closure receiving the resolved typed arguments.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid generated requests or binding
    /// options. Graph resolution errors are returned later by `build`.
    #[track_caller]
    pub fn register_injected_managed_factory<T, A, F>(&mut self, factory: F) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        A: FactoryArgs,
        F: FnOnce(A) -> Result<Managed<T>, FactoryError> + Send + 'static,
    {
        let dependencies = A::dependencies();
        self.register_managed_factory::<T, _>(&dependencies, move |context| {
            let arguments = A::resolve(&context).map_err(FactoryError::new)?;
            factory(arguments)
        })
    }

    /// Stages a one-shot managed synchronous factory with binding options.
    ///
    /// Registering it does not create the managed resource.
    /// After graph validation, a selected factory may run during either
    /// synchronous or asynchronous construction. A later factory may still
    /// fail the build; completed managed resources then participate in
    /// rollback.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` is a sendable one-shot factory
    /// receiving a [`BuildContext`] and returning [`Managed<T>`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `options` - Binding ID, profile, and alias selection metadata.
    /// * `factory` - One-shot closure returning the value and its shutdown
    ///   actions.
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
        self.register_definition(
            Definition::<T>::builder()
                .binding(options)
                .dependencies(dependencies)
                .managed_factory(factory)
                .build()?,
        )
    }

    /// Stages a one-shot asynchronous factory with default binding options.
    ///
    /// Its returned future owns its data, is `Send`, and is driven only by
    /// [`Self::build_async`] or [`Self::build_all_async`]; no runtime is chosen
    /// by this crate.
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
        self.register_definition(
            Definition::<T>::builder()
                .binding(options)
                .dependencies(dependencies)
                .async_factory(factory)
                .build()?,
        )
    }

    /// Stages a managed asynchronous factory with default options.
    ///
    /// The factory is invoked by [`Self::build_async`] or
    /// [`Self::build_all_async`] after graph validation. Registration does not
    /// create the resource, and this crate does not select an async runtime.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` creates a sendable future
    /// yielding [`Managed<T>`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `factory` - One-shot closure creating a managed factory future.
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

    /// Stages a managed asynchronous factory whose typed parameter tuple also
    /// declares its graph dependencies.
    ///
    /// Supported arguments are `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`;
    /// tuples support up to eight parameters. Missing and ambiguous requests
    /// are reported during build validation, before this factory runs. Use
    /// [`Self::register_managed_async_factory`] when the factory needs named
    /// IDs or a custom dependency view. Build with [`Self::build_async`] or
    /// [`Self::build_all_async`]. Dropping a build future stops later factories
    /// but cannot undo side effects performed by a running user factory.
    /// Successfully returned managed values use the normal rollback and
    /// shutdown lifecycle.
    ///
    /// # Type Parameters
    ///
    /// * `T` - Managed component type produced by the factory.
    /// * `A` - Typed argument tuple implementing [`FactoryArgs`].
    /// * `F` - Sendable one-shot function creating the managed component
    ///   future.
    ///
    /// # Parameters
    ///
    /// * `factory` - Closure receiving exactly the arguments described by `A`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` after staging the definition.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] for invalid generated requests or binding
    /// options. Missing and ambiguous dependencies are reported later by build.
    #[track_caller]
    pub fn register_injected_managed_async_factory<T, A, F>(&mut self, factory: F) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
        A: FactoryArgs,
        F: FnOnce(A) -> ManagedFactoryFuture<T> + Send + 'static,
    {
        let dependencies = A::dependencies();
        self.register_managed_async_factory::<T, _>(&dependencies, move |context| {
            match A::resolve(&context).map_err(FactoryError::new) {
                Ok(arguments) => factory(arguments),
                Err(error) => Box::pin(async move { Err(error) }),
            }
        })
    }

    /// Stages a managed asynchronous factory with binding options.
    ///
    /// The factory is invoked by [`Self::build_async`] or
    /// [`Self::build_all_async`] after graph validation. Registration does not
    /// create the resource, and this crate does not select an async runtime.
    ///
    /// # Type Parameters
    ///
    /// `T` is the managed component type. `F` creates a sendable future
    /// yielding [`Managed<T>`].
    ///
    /// # Parameters
    ///
    /// * `dependencies` - Requests made available to the factory at build time.
    /// * `options` - Binding ID, profile, and alias selection metadata.
    /// * `factory` - One-shot closure creating a managed factory future.
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
        self.register_definition(
            Definition::<T>::builder()
                .binding(options)
                .dependencies(dependencies)
                .managed_async_factory(factory)
                .build()?,
        )
    }

    /// Atomically stages a complete validated definition of component `T`.
    ///
    /// Returns a structured registration error if the definition's internal
    /// keys are invalid. Factories and projectors are deferred until build.
    pub fn register_definition<T>(&mut self, value: Definition<T>) -> Result<(), RegistrationError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.stage_definition(value.into_pending())
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
    /// A selected active asynchronous factory yields `AsyncRequired` before any
    /// factory runs. Factory failures retain their source and no partial
    /// context escapes.
    ///
    /// # Returns
    ///
    /// The application owner containing the selected dependency closure.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] with its [`BuildError`] cause for missing
    /// roots, invalid graphs, asynchronous factories, missing managed wait
    /// policy, or construction failures.
    pub fn build(mut self) -> Result<Application, BuildFailure> {
        if self.roots.is_empty() {
            return Err(BuildError::NoRootsSelected.into());
        }
        let policy = self.wait_policy.take();
        let scope = self.validation_scope;
        let (definitions, profiles, roots) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots_with_scope(definitions, &profiles, Some(&roots), scope)?;
        let policy = Self::selected_wait_policy(&graph, policy)?;
        graph.require_sync()?;
        Construction::new(graph, policy).run_sync()
    }

    /// Builds the selected synchronous graph and waits for rollback on failure.
    ///
    /// On success, returns an [`Application`] that the caller must explicitly
    /// shut down. On failure, normal completion waits for managed cleanup; the
    /// returned report may still describe unsuccessful cleanup. Cancelling
    /// this future only provides best-effort abort and does not guarantee that
    /// rollback waiting completes.
    ///
    /// # Errors
    ///
    /// Returns the original build cause and any completed cleanup report.
    pub async fn build_settled(self) -> Result<Application, SettledBuildFailure> {
        match self.build() {
            Ok(application) => Ok(application),
            Err(failure) => Err(failure.settle().await),
        }
    }

    /// Validates and constructs every definition active under the configured
    /// profiles, whether or not a root was registered.
    ///
    /// Prefer [`Self::build`] when the application only needs a subset of
    /// discovered definitions.
    ///
    /// # Returns
    ///
    /// The application owner containing every active definition.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] with its [`BuildError`] cause for invalid
    /// graphs, asynchronous factories, missing managed wait policy, or
    /// construction failures.
    pub fn build_all(mut self) -> Result<Application, BuildFailure> {
        let policy = self.wait_policy.take();
        let (definitions, profiles, _) = self.prepare_definitions()?;
        let graph = ValidatedGraph::validate_roots(definitions, &profiles, None)?;
        let policy = Self::selected_wait_policy(&graph, policy)?;
        graph.require_sync()?;
        Construction::new(graph, policy).run_sync()
    }

    /// Builds every active synchronous definition and waits for rollback on
    /// failure.
    ///
    /// On success, returns an [`Application`] that the caller must explicitly
    /// shut down. On failure, normal completion waits for managed cleanup; the
    /// returned report may still describe unsuccessful cleanup. Cancelling
    /// this future only provides best-effort abort and does not guarantee that
    /// rollback waiting completes.
    ///
    /// # Errors
    ///
    /// Returns the original build cause and any completed cleanup report.
    pub async fn build_all_settled(self) -> Result<Application, SettledBuildFailure> {
        match self.build_all() {
            Ok(application) => Ok(application),
            Err(failure) => Err(failure.settle().await),
        }
    }

    /// Builds the components selected by one or more calls to [`Self::root`]
    /// or [`Self::root_by_id`], including their transitive dependencies.
    /// The future is `Send` and uses the caller's executor.
    ///
    /// Dropping it prevents remaining factories from starting and requests stop
    /// for managed resources already returned to the container, without
    /// waiting. Side effects created before a factory returns a managed
    /// value remain that factory's responsibility. A failure publishes no
    /// context.
    ///
    /// # Returns
    ///
    /// A future that resolves to the owner of the selected component graph.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] with its [`BuildError`] cause for missing
    /// roots, invalid graphs, missing managed wait policy, or factory
    /// failures. Already constructed managed resources are aborted before
    /// returning; the failure owns their deferred cleanup handle.
    pub async fn build_async(self) -> Result<Application, BuildFailure> {
        self.prepare_async(false)?.run_async().await
    }

    /// Creates a lazy, single-use asynchronous session for the selected roots.
    /// Validation starts on the first poll of [`BuildSession::run`]. Keep the
    /// session to observe cleanup if that borrowing future is cancelled.
    #[inline]
    pub fn build_async_session(self) -> BuildSession {
        BuildSession::new(self, false)
    }

    /// Creates a lazy asynchronous session that constructs all active
    /// definitions, regardless of the selected roots.
    #[inline]
    pub fn build_all_async_session(self) -> BuildSession {
        BuildSession::new(self, true)
    }

    /// Validates the selected roots or all active definitions and returns
    /// their private construction owner. Validation and policy errors occur
    /// before any factory runs; profile and replacement rules are shared.
    pub(super) fn prepare_async(mut self, all: bool) -> Result<Construction, BuildFailure> {
        if !all && self.roots.is_empty() {
            return Err(BuildError::NoRootsSelected.into());
        }
        let policy = self.wait_policy.take();
        let scope = self.validation_scope;
        let (definitions, profiles, roots) = self.prepare_definitions()?;
        let graph = if all {
            ValidatedGraph::validate_roots(definitions, &profiles, None)?
        } else {
            ValidatedGraph::validate_roots_with_scope(definitions, &profiles, Some(&roots), scope)?
        };
        let policy = Self::selected_wait_policy(&graph, policy)?;
        Ok(Construction::new(graph, policy))
    }

    /// Asynchronously builds the selected graph and waits for rollback on
    /// failure.
    ///
    /// On success, returns an [`Application`] that the caller must explicitly
    /// shut down. On failure, normal completion waits for managed cleanup; the
    /// returned report may still describe unsuccessful cleanup. Cancelling
    /// this future only provides best-effort abort and does not guarantee that
    /// rollback waiting completes.
    ///
    /// # Errors
    ///
    /// Returns the original build cause and any completed cleanup report.
    pub async fn build_async_settled(self) -> Result<Application, SettledBuildFailure> {
        match self.build_async().await {
            Ok(application) => Ok(application),
            Err(failure) => Err(failure.settle().await),
        }
    }

    /// Validates and asynchronously constructs every definition active under
    /// the configured profiles, whether or not a root was registered.
    ///
    /// # Returns
    ///
    /// A future that resolves to the owner of every active definition.
    ///
    /// # Errors
    ///
    /// Returns [`BuildFailure`] with its [`BuildError`] cause for invalid
    /// graphs, missing managed wait policy, or factory failures. The
    /// failure owns deferred managed cleanup; construction never waits for
    /// rollback.
    pub async fn build_all_async(self) -> Result<Application, BuildFailure> {
        self.prepare_async(true)?.run_async().await
    }

    /// Asynchronously builds every active definition and waits for rollback on
    /// failure.
    ///
    /// On success, returns an [`Application`] that the caller must explicitly
    /// shut down. On failure, normal completion waits for managed cleanup; the
    /// returned report may still describe unsuccessful cleanup. Cancelling
    /// this future only provides best-effort abort and does not guarantee that
    /// rollback waiting completes.
    ///
    /// # Errors
    ///
    /// Returns the original build cause and any completed cleanup report.
    pub async fn build_all_async_settled(self) -> Result<Application, SettledBuildFailure> {
        match self.build_all_async().await {
            Ok(application) => Ok(application),
            Err(failure) => Err(failure.settle().await),
        }
    }

    /// Atomically adds an already validated complete definition to the staging
    /// area.
    pub(crate) fn stage_definition(&mut self, definition: PendingDefinition) -> Result<(), RegistrationError> {
        definition.stage_into(&mut self.definitions)
    }

    /// Requires explicit deadlines only when the validated selection owns
    /// managed resources.
    fn selected_wait_policy(graph: &ValidatedGraph, policy: Option<WaitPolicy>) -> Result<WaitPolicy, BuildError> {
        let has_managed = graph.order.iter().any(|location| {
            matches!(
                &graph.definitions[location.definition].bindings[location.binding].kind,
                PendingBindingKind::ManagedSyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_)
            )
        });
        match policy {
            Some(policy) => Ok(policy),
            None if has_managed => Err(BuildError::MissingWaitPolicy),
            None => Ok(WaitPolicy::unbounded()),
        }
    }

    /// Collects the active indices and diagnostic sources of the definitions
    /// that declare a binding for `anchor`.
    ///
    /// Applying one replacement needs the replacement itself, the single
    /// original it overrides, and the ambiguity check across both; collecting
    /// them in one pass keeps those decisions consistent and avoids rescanning
    /// every active definition once per lookup.
    fn anchor_occurrences(
        active: &[(usize, PendingDefinition)],
        anchor: &BindingKey,
    ) -> Vec<(usize, DefinitionSource)> {
        active
            .iter()
            .filter(|(_, definition)| definition.bindings.iter().any(|binding| binding.key == *anchor))
            .map(|(index, definition)| (*index, definition.source))
            .collect()
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
            .filter(|(_, definition)| profile_is_active(definition.profile.as_deref(), &active_profiles))
            .collect();
        for replacement in replacements {
            let occurrences = Self::anchor_occurrences(&active, &replacement.anchor);
            // An inactive replacement does not affect bindings in active profiles.
            let Some(slot) = occurrences
                .iter()
                .position(|(index, _)| *index == replacement.definition_index)
            else {
                continue;
            };
            let replacement_source = occurrences[slot].1;
            let originals: Vec<DefinitionSource> = occurrences
                .iter()
                .filter(|(index, _)| *index < replacement.definition_index)
                .map(|(_, source)| *source)
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
            let Some((original_index, _)) = occurrences
                .iter()
                .find(|(index, _)| *index < replacement.definition_index)
                .copied()
            else {
                continue;
            };
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
}
