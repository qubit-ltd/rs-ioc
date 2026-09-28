// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Versioned implementation protocol for generated code.

/// Gives generated config consumers an explicit diagnostic when the runtime
/// dependency was compiled without its `config` feature.
#[cfg(feature = "config")]
#[doc(hidden)]
#[macro_export]
macro_rules! require_config {
    ($($tokens:tt)*) => { $($tokens)* };
}

/// Reports the missing feature at the consuming declaration.
#[cfg(not(feature = "config"))]
#[doc(hidden)]
#[macro_export]
macro_rules! require_config {
    ($($tokens:tt)*) => {
        compile_error!("qubit-ioc: #[value] and #[ConfigurationProperties] require the `config` feature");
    };
}

#[doc(hidden)]
pub use crate::require_config;

/// Protocol version for macro-generated component definitions.
///
/// Generated code uses this module to stage definitions through the runtime.
/// Its items are versioned implementation details and may change with the
/// generated-code protocol.
pub mod codegen_v1 {
    use std::marker::PhantomData;
    use std::sync::Arc;

    #[cfg(feature = "config")]
    pub use qubit_config::Config;

    use crate::FactoryFuture;
    use crate::binding::PendingBinding;
    use crate::binding::PendingDefinition;
    use crate::build_context::BuildContext;
    use crate::builder::ContainerBuilder;
    use crate::builder::validate_dependencies;
    use crate::builder::validate_options;
    use crate::dependency::Dependency;
    use crate::error::FactoryError;
    use crate::error::RegistrationError;
    use crate::managed::Managed;
    use crate::managed::ManagedFactoryFuture;
    use crate::options::BindingOptions;
    use crate::options::DefinitionSource;

    /// Collects one concrete `T` and its interface projections before
    /// registration.
    ///
    /// This type is a generated-code protocol and can change in a later
    /// version.
    ///
    /// # Type Parameters
    ///
    /// `T` is the concrete component type produced by the staged definition.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use qubit_ioc::__private::codegen_v1::DefinitionDraft;
    /// use qubit_ioc::BindingOptions;
    /// use qubit_ioc::ContainerBuilder;
    /// use qubit_ioc::DefinitionSource;
    ///
    /// struct Message;
    /// let draft = DefinitionDraft::<Message>::from_instance(
    ///     DefinitionSource::new("app", "app", "src/main.rs", 1, 1, "Message"),
    ///     BindingOptions::default(),
    ///     Arc::new(Message),
    /// )?;
    /// let mut builder = ContainerBuilder::new();
    /// draft.register(&mut builder)?;
    /// assert!(builder.build_all()?.get::<Message>().is_ok());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub struct DefinitionDraft<T: ?Sized + Send + Sync + 'static> {
        /// Concrete binding and its staged interface projections.
        definition: PendingDefinition,
        /// Retains the component type for compile-time protocol checking.
        marker: PhantomData<Arc<T>>,
    }

    // Generated registrations return the public structured RegistrationError.
    #[allow(clippy::result_large_err)]
    impl<T: ?Sized + Send + Sync + 'static> DefinitionDraft<T> {
        /// Stages a managed synchronous concrete factory and its requests.
        ///
        /// # Parameters
        ///
        /// * `source` - Source location retained for diagnostics.
        /// * `dependencies` - Requests available to the factory.
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `factory` - One-shot closure that constructs the managed value.
        ///
        /// # Returns
        ///
        /// A draft containing the factory and validated requests.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for invalid options or requests.
        pub fn new_managed_sync<F>(
            source: DefinitionSource,
            dependencies: &[Dependency],
            options: BindingOptions,
            factory: F,
        ) -> Result<Self, RegistrationError>
        where
            F: FnOnce(BuildContext) -> Result<Managed<T>, FactoryError> + Send + 'static,
        {
            let (key, profile) = validate_options::<T>(&options, source)?;
            validate_dependencies(dependencies, source)?;
            let concrete = PendingBinding::managed_sync_factory(key, options.primary, options.order, factory);
            Ok(Self {
                definition: PendingDefinition::new(source, profile, dependencies.to_vec(), concrete),
                marker: PhantomData,
            })
        }

        /// Stages a managed asynchronous concrete factory without polling it.
        ///
        /// # Parameters
        ///
        /// * `source` - Source location retained for diagnostics.
        /// * `dependencies` - Requests available to the factory.
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `factory` - One-shot closure that creates the managed future.
        ///
        /// # Returns
        ///
        /// A draft containing the factory and validated requests.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for invalid options or requests.
        pub fn new_managed_async<F>(
            source: DefinitionSource,
            dependencies: &[Dependency],
            options: BindingOptions,
            factory: F,
        ) -> Result<Self, RegistrationError>
        where
            F: FnOnce(BuildContext) -> ManagedFactoryFuture<T> + Send + 'static,
        {
            let (key, profile) = validate_options::<T>(&options, source)?;
            validate_dependencies(dependencies, source)?;
            let concrete = PendingBinding::managed_async_factory(key, options.primary, options.order, factory);
            Ok(Self {
                definition: PendingDefinition::new(source, profile, dependencies.to_vec(), concrete),
                marker: PhantomData,
            })
        }

        /// Stages a synchronous concrete factory and its declared requests.
        ///
        /// IDs, profile and dependencies are checked before the factory is
        /// stored; the factory itself runs only during a validated
        /// build.
        ///
        /// # Type Parameters
        ///
        /// `F` is a sendable one-shot factory that returns a shared `T` or a
        /// [`FactoryError`].
        ///
        /// # Parameters
        ///
        /// * `source` - Source location retained for diagnostics.
        /// * `dependencies` - Requests available to the factory.
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `factory` - One-shot closure that creates the shared value.
        ///
        /// # Returns
        ///
        /// A draft containing the factory and validated requests.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for invalid options or requests.
        pub fn new_sync<F>(
            source: DefinitionSource,
            dependencies: &[Dependency],
            options: BindingOptions,
            factory: F,
        ) -> Result<Self, RegistrationError>
        where
            F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
        {
            let (key, profile) = validate_options::<T>(&options, source)?;
            validate_dependencies(dependencies, source)?;
            let concrete = PendingBinding::sync_factory(key, options.primary, options.order, factory);
            Ok(Self {
                definition: PendingDefinition::new(source, profile, dependencies.to_vec(), concrete),
                marker: PhantomData,
            })
        }

        /// Stages an asynchronous concrete factory without polling its future.
        ///
        /// Invalid IDs, profile or duplicate requests return a registration
        /// error.
        ///
        /// # Type Parameters
        ///
        /// `F` is a sendable one-shot factory that creates a future returning
        /// a shared `T` or a [`FactoryError`].
        ///
        /// # Parameters
        ///
        /// * `source` - Source location retained for diagnostics.
        /// * `dependencies` - Requests available to the factory.
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `factory` - One-shot closure that creates the component future.
        ///
        /// # Returns
        ///
        /// A draft containing the factory and validated requests.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for invalid options or requests.
        pub fn new_async<F>(
            source: DefinitionSource,
            dependencies: &[Dependency],
            options: BindingOptions,
            factory: F,
        ) -> Result<Self, RegistrationError>
        where
            F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
        {
            let (key, profile) = validate_options::<T>(&options, source)?;
            validate_dependencies(dependencies, source)?;
            let concrete = PendingBinding::async_factory(key, options.primary, options.order, factory);
            Ok(Self {
                definition: PendingDefinition::new(source, profile, dependencies.to_vec(), concrete),
                marker: PhantomData,
            })
        }

        /// Stages an already constructed concrete instance.
        ///
        /// Invalid IDs or profile return a registration error.
        ///
        /// # Type Parameters
        ///
        /// `T` is the concrete component type held by `value`.
        ///
        /// # Parameters
        ///
        /// * `source` - Source location retained for diagnostics.
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `value` - Shared concrete value to stage.
        ///
        /// # Returns
        ///
        /// A draft containing the instance binding.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for invalid options.
        pub fn from_instance(
            source: DefinitionSource,
            options: BindingOptions,
            value: Arc<T>,
        ) -> Result<Self, RegistrationError> {
            let (key, profile) = validate_options::<T>(&options, source)?;
            let concrete = PendingBinding::instance(key, value, options.primary, options.order);
            Ok(Self {
                definition: PendingDefinition::new(source, profile, Vec::new(), concrete),
                marker: PhantomData,
            })
        }

        /// Adds an interface key projected from the same built `Arc<T>`.
        ///
        /// `project` must be a compiler-checked `Arc<T>` to `Arc<U>`
        /// conversion. The alias shares this definition's activation
        /// profile; a conflicting alias profile or invalid ID returns a
        /// registration error.
        ///
        /// # Type Parameters
        ///
        /// `U` is the interface type added as an alias. `F` projects a shared
        /// `T` into a shared `U` and must be safe to call across threads.
        ///
        /// # Parameters
        ///
        /// * `options` - ID, profile, primary selection, and collection order.
        /// * `project` - Conversion from the concrete value to the alias type.
        ///
        /// # Returns
        ///
        /// Returns `Ok(())` after adding the alias.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] for an invalid ID or profile mismatch.
        pub fn bind<U, F>(&mut self, options: BindingOptions, project: F) -> Result<(), RegistrationError>
        where
            U: ?Sized + Send + Sync + 'static,
            F: Fn(Arc<T>) -> Arc<U> + Send + Sync + 'static,
        {
            let (key, profile) = validate_options::<U>(&options, self.definition.source)?;
            if profile != self.definition.profile {
                return Err(RegistrationError::AliasProfileMismatch {
                    definition: self.definition.source,
                    concrete: self.definition.profile.clone(),
                    alias: profile,
                });
            }
            let concrete_key = self.definition.bindings[0].key.clone();
            self.definition.add_alias(PendingBinding::alias::<T, U, F>(
                key,
                concrete_key,
                options.primary,
                options.order,
                project,
            ));
            Ok(())
        }

        /// Atomically validates and stages the concrete binding and all
        /// aliases.
        ///
        /// A duplicate key within this draft returns a registration error and
        /// leaves `builder` unchanged. Other definitions are checked at build.
        ///
        /// # Parameters
        ///
        /// `builder` receives the complete concrete binding and all aliases.
        ///
        /// # Returns
        ///
        /// Returns `Ok(())` after staging the definition.
        ///
        /// # Errors
        ///
        /// Returns [`RegistrationError`] if any binding in the draft conflicts.
        pub fn register(self, builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
            builder.stage_definition(self.definition)
        }
    }
}
