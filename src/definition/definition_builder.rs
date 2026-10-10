// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Consuming configuration and validation of complete component definitions.

use std::marker::PhantomData;
use std::sync::Arc;

use super::Definition;
use super::internal::AliasDraft;
use crate::binding::FactoryFuture;
use crate::binding::PendingBinding;
use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::build_context::BuildContext;
use crate::builder::validate_dependencies;
use crate::builder::validate_options;
use crate::dependency::Dependency;
use crate::error::FactoryError;
use crate::error::RegistrationError;
use crate::key::BindingKey;
use crate::managed::Managed;
use crate::managed::ManagedFactoryFuture;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;

/// Configures one concrete binding and any number of interface aliases.
///
/// Setters replace previous values; `bind` appends an alias. Validation never
/// invokes a factory or projector and produces an atomic [`Definition`].
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::Definition;
/// use qubit_ioc::Dependency;
/// use qubit_ioc::FactoryError;
///
/// let definition = Definition::<String>::builder()
///     .dependencies(&[Dependency::of::<u32>()])
///     .factory(|context| {
///         let value = *context.get::<u32>().map_err(FactoryError::new)?;
///         Ok(Arc::new(format!("value {value}")))
///     })
///     .build()?;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance(Arc::new(7_u32))?;
/// builder.register_definition(definition)?;
/// builder.root::<String>();
/// let application = builder.build()?;
/// assert_eq!(*application.context().get::<String>()?, "value 7");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct DefinitionBuilder<T: ?Sized + Send + Sync + 'static> {
    /// Diagnostic location of every declared binding.
    source: DefinitionSource,
    /// Final concrete binding options.
    options: BindingOptions,
    /// Requests made available to the concrete factory.
    dependencies: Vec<Dependency>,
    /// Last configured instance or factory.
    concrete: Option<PendingBinding>,
    /// Interface projections awaiting final options and target validation.
    aliases: Vec<AliasDraft>,
    /// Preserves the component type across erased storage.
    marker: PhantomData<Arc<T>>,
}

#[allow(clippy::result_large_err)]
impl<T: ?Sized + Send + Sync + 'static> DefinitionBuilder<T> {
    /// Creates an empty builder with the supplied diagnostic source.
    ///
    /// # Parameters
    ///
    /// `source` is the diagnostic location attached to every binding this
    /// builder eventually validates.
    ///
    /// # Returns
    ///
    /// A builder with default options, no declared requests, no construction
    /// source, and no aliases.
    #[inline]
    pub(super) fn new(source: DefinitionSource) -> Self {
        Self {
            source,
            options: BindingOptions::default(),
            dependencies: Vec::new(),
            concrete: None,
            aliases: Vec::new(),
            marker: PhantomData,
        }
    }

    /// Replaces the diagnostic source attached to this complete definition.
    ///
    /// # Parameters
    ///
    /// `source` is the diagnostic location reported by later registration
    /// errors; it replaces any previously configured source.
    ///
    /// # Returns
    ///
    /// The same builder with the new diagnostic source.
    #[must_use]
    #[inline]
    pub fn source(mut self, source: DefinitionSource) -> Self {
        self.source = source;
        self
    }

    /// Replaces the concrete ID, profile, primary flag, and ordering options.
    ///
    /// # Parameters
    ///
    /// `options` are the caller-supplied registration options for the concrete
    /// binding; they replace any previously configured options.
    ///
    /// # Returns
    ///
    /// The same builder with the new concrete options.
    #[must_use]
    #[inline]
    pub fn binding(mut self, options: BindingOptions) -> Self {
        self.options = options;
        self
    }

    /// Replaces the factory's declared requests with a copy of `requests`.
    ///
    /// # Parameters
    ///
    /// `requests` is the exact set of dependency requests the concrete factory
    /// will be allowed to resolve; it is copied, so later mutation of the
    /// caller's slice cannot change this definition.
    ///
    /// # Returns
    ///
    /// The same builder declaring exactly `requests`.
    #[must_use]
    #[inline]
    pub fn dependencies(mut self, requests: &[Dependency]) -> Self {
        self.dependencies = requests.to_vec();
        self
    }

    /// Replaces the source with shared `value`, without constructing anything.
    ///
    /// # Parameters
    ///
    /// `value` is the already-constructed component handle stored in the
    /// context; the container never calls a factory for this definition.
    ///
    /// # Returns
    ///
    /// The same builder staged as a shared instance.
    #[must_use]
    pub fn instance(mut self, value: Arc<T>) -> Self {
        self.concrete = Some(PendingBinding::instance(BindingKey::of::<T>(None), value, false, 0));
        self
    }

    /// Replaces the source with a deferred one-shot factory.
    ///
    /// `factory` receives only declared dependencies and runs during build,
    /// never during definition validation. Its result supplies the component.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type this builder defines.
    ///
    /// # Parameters
    ///
    /// `factory` is deferred until graph construction and may resolve only the
    /// requests declared through [`Self::dependencies`].
    ///
    /// # Returns
    ///
    /// The same builder staged as a deferred one-shot synchronous factory.
    #[must_use]
    pub fn factory<F>(mut self, factory: F) -> Self
    where
        F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        self.concrete = Some(PendingBinding::sync_factory(
            BindingKey::of::<T>(None),
            false,
            0,
            factory,
        ));
        self
    }

    /// Replaces the source with a deferred one-shot async factory.
    ///
    /// `factory` receives only declared dependencies and runs during build,
    /// never during definition validation. Its result supplies the component.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type this builder defines.
    ///
    /// # Parameters
    ///
    /// `factory` is deferred until graph construction and may resolve only the
    /// requests declared through [`Self::dependencies`].
    ///
    /// # Returns
    ///
    /// The same builder staged as a deferred one-shot asynchronous factory.
    #[must_use]
    pub fn async_factory<F>(mut self, factory: F) -> Self
    where
        F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
    {
        self.concrete = Some(PendingBinding::async_factory(
            BindingKey::of::<T>(None),
            false,
            0,
            factory,
        ));
        self
    }

    /// Replaces the source with a deferred one-shot managed factory.
    ///
    /// `factory` receives only declared dependencies and runs during build,
    /// never during definition validation. Its result supplies the component
    /// and its shutdown policy, so the application registers a managed
    /// shutdown handle for it.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type this builder defines.
    /// `F` is the deferred synchronous managed factory that produces it.
    ///
    /// # Parameters
    ///
    /// `factory` is deferred until graph construction and may resolve only the
    /// requests declared through [`Self::dependencies`].
    ///
    /// # Returns
    ///
    /// The same builder staged as a deferred one-shot synchronous managed
    /// factory.
    #[must_use]
    pub fn managed_factory<F>(mut self, factory: F) -> Self
    where
        F: FnOnce(BuildContext) -> Result<Managed<T>, FactoryError> + Send + 'static,
    {
        self.concrete = Some(PendingBinding::managed_sync_factory(
            BindingKey::of::<T>(None),
            false,
            0,
            factory,
        ));
        self
    }

    /// Replaces the source with a deferred one-shot managed async factory.
    ///
    /// `factory` receives only declared dependencies and runs during build,
    /// never during definition validation. Its result supplies the component
    /// and its shutdown policy, so the application registers a managed
    /// shutdown handle for it.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete or trait-object component type this builder defines.
    /// `F` is the deferred asynchronous managed factory that produces it.
    ///
    /// # Parameters
    ///
    /// `factory` is deferred until graph construction and may resolve only the
    /// requests declared through [`Self::dependencies`].
    ///
    /// # Returns
    ///
    /// The same builder staged as a deferred one-shot asynchronous managed
    /// factory.
    #[must_use]
    pub fn managed_async_factory<F>(mut self, factory: F) -> Self
    where
        F: FnOnce(BuildContext) -> ManagedFactoryFuture<T> + Send + 'static,
    {
        self.concrete = Some(PendingBinding::managed_async_factory(
            BindingKey::of::<T>(None),
            false,
            0,
            factory,
        ));
        self
    }

    /// Appends an interface alias using `options` and deferred `project`.
    ///
    /// `U` is the thread-safe interface type. `project` should preserve the
    /// concrete allocation when converting `Arc<T>` to `Arc<U>`; it executes
    /// during graph construction. A missing alias profile inherits the final
    /// concrete profile; an explicit differing profile is rejected by `build`.
    /// Aliases append, so repeated calls accumulate instead of replacing.
    ///
    /// # Type Parameters
    ///
    /// `U` is the interface type the alias exposes, and `F` is the projection
    /// applied to the concrete component. Both must be thread-safe and outlive
    /// the built application.
    ///
    /// # Parameters
    ///
    /// `options` supplies the alias profile, primary flag, and ordering, and
    /// `project` is deferred until graph construction and must convert the
    /// concrete `Arc<T>` into an `Arc<U>` of the same component.
    ///
    /// # Returns
    ///
    /// The same builder with one additional alias draft.
    #[must_use]
    pub fn bind<U, F>(mut self, options: BindingOptions, project: F) -> Self
    where
        U: ?Sized + Send + Sync + 'static,
        F: Fn(Arc<T>) -> Arc<U> + Send + Sync + 'static,
    {
        let binding = PendingBinding::alias::<T, U, F>(
            BindingKey::of::<U>(None),
            BindingKey::of::<T>(None),
            options.primary,
            options.order,
            project,
        );
        self.aliases.push(AliasDraft {
            options,
            binding,
            validate: validate_options::<U>,
        });
        self
    }

    /// Validates and consumes this builder into a complete definition.
    ///
    /// Returns `RegistrationError` with this definition's source for missing
    /// construction sources, malformed options, duplicate requests or keys,
    /// and alias profile mismatches. No factory or projector executes.
    ///
    /// # Returns
    ///
    /// The complete definition, carrying the validated concrete binding, its
    /// resolved profile, and every appended alias already bound to the
    /// concrete key.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] with this definition's source when
    /// `build` is called before any `instance` or factory method, when
    /// [`Self::binding`] or [`Self::bind`] options are malformed, when
    /// [`Self::dependencies`] contains duplicate requests, when a concrete or
    /// alias key duplicates an already declared one, or when an alias declares
    /// a profile that differs from the resolved concrete profile.
    pub fn build(self) -> Result<Definition<T>, RegistrationError> {
        let (key, profile) = validate_options::<T>(&self.options, self.source)?;
        validate_dependencies(&self.dependencies, self.source)?;
        let mut concrete = self.concrete.ok_or(RegistrationError::MissingDefinitionFactory {
            definition: self.source,
        })?;
        concrete.key = key.clone();
        concrete.primary = self.options.primary;
        concrete.order = self.options.order;
        let mut pending = PendingDefinition::new(self.source, profile.clone(), self.dependencies, concrete);
        for alias in self.aliases {
            let mut binding = alias.binding;
            // The interface type is already encoded in the staged alias key.
            let (validated, alias_profile) = (alias.validate)(&alias.options, self.source)?;
            if alias_profile.is_some() && alias_profile != profile {
                return Err(RegistrationError::AliasProfileMismatch {
                    definition: self.source,
                    concrete: profile,
                    alias: alias_profile,
                });
            }
            binding.key = validated;
            if let PendingBindingKind::Alias { target, .. } = &mut binding.kind {
                *target = key.clone();
            }
            pending.add_alias(binding);
        }
        pending.validate_self_keys()?;
        Ok(Definition {
            pending,
            marker: PhantomData,
        })
    }
}
