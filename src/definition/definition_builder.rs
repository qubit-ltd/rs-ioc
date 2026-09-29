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
    #[must_use]
    pub fn source(mut self, source: DefinitionSource) -> Self {
        self.source = source;
        self
    }

    /// Replaces the concrete ID, profile, primary flag, and ordering options.
    #[must_use]
    pub fn binding(mut self, options: BindingOptions) -> Self {
        self.options = options;
        self
    }

    /// Replaces the factory's declared requests with a copy of `requests`.
    #[must_use]
    pub fn dependencies(mut self, requests: &[Dependency]) -> Self {
        self.dependencies = requests.to_vec();
        self
    }

    /// Replaces the source with shared `value`, without constructing anything.
    #[must_use]
    pub fn instance(mut self, value: Arc<T>) -> Self {
        self.concrete = Some(PendingBinding::instance(BindingKey::of::<T>(None), value, false, 0));
        self
    }

    /// Replaces the source with a deferred one-shot factory.
    ///
    /// `factory` receives only declared dependencies and runs during build,
    /// never during definition validation. Its result supplies the component.
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
    /// never during definition validation. Its result supplies the component.
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
    /// never during definition validation. Its result supplies the component.
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
