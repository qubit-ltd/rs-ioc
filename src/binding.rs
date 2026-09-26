// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal binding definitions and erased factories.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A sendable future that produces one shared component or a retained factory
/// error.
///
/// # Type Parameters
///
/// `T` is the concrete or trait-object value returned by the factory.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::FactoryFuture;
///
/// fn make_value() -> FactoryFuture<u32> {
///     Box::pin(async { Ok(Arc::new(42)) })
/// }
/// ```
pub type FactoryFuture<T> = Pin<Box<dyn Future<Output = Result<Arc<T>, FactoryError>> + Send + 'static>>;
use crate::build_context::BuildContext;
use crate::dependency::Dependency;
use crate::error::FactoryError;
use crate::error::RegistrationError;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::ErasedInstance;

/// A sendable factory future returning one erased complete `Arc<T>`.
pub(crate) type ErasedFactoryFuture =
    Pin<Box<dyn Future<Output = Result<ErasedInstance, FactoryError>> + Send + 'static>>;

/// The one-shot synchronous factory signature used during construction.
pub(crate) type SyncFactory = Box<dyn FnOnce(BuildContext) -> Result<ErasedInstance, FactoryError> + Send + 'static>;

/// The one-shot asynchronous factory signature used during construction.
pub(crate) type AsyncFactory = Box<dyn FnOnce(BuildContext) -> ErasedFactoryFuture + Send + 'static>;

/// Projects an already built concrete `Arc` to an interface `Arc`.
pub(crate) type AliasProjector = Box<dyn Fn(&ErasedInstance) -> Option<ErasedInstance> + Send + Sync + 'static>;

/// A single concrete binding or interface alias in a definition.
pub(crate) struct PendingBinding {
    /// Typed identity of this concrete binding or interface alias.
    pub(crate) key: BindingKey,
    /// Whether unnamed dependency requests may prefer this binding.
    pub(crate) primary: bool,
    /// Collection ordering value, compared before ID and source.
    pub(crate) order: i32,
    /// Sources removed by explicit replacement of this key.
    pub(crate) replaced_sources: Vec<DefinitionSource>,
    /// Deferred instance, factory, or interface projection operation.
    pub(crate) kind: PendingBindingKind,
}

/// Deferred construction or projection action for a binding.
pub(crate) enum PendingBindingKind {
    /// A shared instance supplied before graph construction.
    Instance(ErasedInstance),
    /// A one-shot synchronous factory.
    SyncFactory(SyncFactory),
    /// A one-shot asynchronous factory.
    AsyncFactory(AsyncFactory),
    /// A projection from a previously constructed concrete binding.
    Alias {
        /// Key of the concrete binding to project.
        target: BindingKey,
        /// Type checked projection to the alias type.
        project: AliasProjector,
    },
}

impl PendingBinding {
    /// Stages a complete shared instance of `T` under a matching typed `key`.
    pub(crate) fn instance<T: ?Sized + Send + Sync + 'static>(
        key: BindingKey,
        value: Arc<T>,
        primary: bool,
        order: i32,
    ) -> Self {
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::Instance(Box::new(value)),
        }
    }

    /// Erases the `Arc<T>` produced by a synchronous one-shot `factory`.
    ///
    /// Construction and any factory error occur only when the validated graph
    /// executes this binding.
    pub(crate) fn sync_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> Result<ErasedInstance, FactoryError> {
            factory(context).map(|value| Box::new(value) as ErasedInstance)
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::SyncFactory(Box::new(erased)),
        }
    }

    /// Erases the `Arc<T>` produced by an asynchronous one-shot `factory`.
    ///
    /// The returned future owns its data and is `Send`; construction and errors
    /// occur only when a caller polls it during asynchronous building.
    pub(crate) fn async_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> ErasedFactoryFuture {
            let future = factory(context);
            Box::pin(async move { future.await.map(|value| Box::new(value) as ErasedInstance) })
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::AsyncFactory(Box::new(erased)),
        }
    }

    /// Stages an alias from the concrete `target` to interface type `U`.
    ///
    /// `project` converts a cloned `Arc<T>` into an `Arc<U>` without rebuilding
    /// the component. A mismatched source type yields `None` during projection.
    pub(crate) fn alias<T, U, F>(key: BindingKey, target: BindingKey, primary: bool, order: i32, project: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        U: ?Sized + Send + Sync + 'static,
        F: Fn(Arc<T>) -> Arc<U> + Send + Sync + 'static,
    {
        let erased = move |value: &ErasedInstance| -> Option<ErasedInstance> {
            let concrete = value.downcast_ref::<Arc<T>>()?;
            Some(Box::new(project(Arc::clone(concrete))))
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::Alias {
                target,
                project: Box::new(erased),
            },
        }
    }

    /// Projects an alias from `value`, returning `None` for a non-alias or type
    /// mismatch.
    #[cfg(test)]
    pub(crate) fn project_alias(&self, value: &ErasedInstance) -> Option<ErasedInstance> {
        match &self.kind {
            PendingBindingKind::Alias { project, .. } => project(value),
            _ => None,
        }
    }
}

/// A concrete binding and all its interface aliases, staged atomically.
pub(crate) struct PendingDefinition {
    /// Source attached to every binding declared by this definition.
    pub(crate) source: DefinitionSource,
    /// Optional profile required for activation.
    pub(crate) profile: Option<String>,
    /// Requests that must be selected before executing the concrete factory.
    pub(crate) dependencies: Vec<Dependency>,
    /// Concrete binding followed by its interface aliases.
    pub(crate) bindings: Vec<PendingBinding>,
}

// RegistrationError keeps complete public diagnostic keys and source locations.
#[allow(clippy::result_large_err)]
impl PendingDefinition {
    /// Creates one definition with its concrete binding and declared requests.
    ///
    /// `profile` and IDs must already be validated by the registration entry.
    /// Call [`Self::validate_self_keys`] before adding the definition to a
    /// builder.
    pub(crate) fn new(
        source: DefinitionSource,
        profile: Option<String>,
        dependencies: Vec<Dependency>,
        concrete: PendingBinding,
    ) -> Self {
        Self {
            source,
            profile,
            dependencies,
            bindings: vec![concrete],
        }
    }

    /// Adds an interface alias to the pending definition before validation.
    pub(crate) fn add_alias(&mut self, alias: PendingBinding) {
        self.bindings.push(alias);
    }

    /// Rejects a duplicate exact key inside this definition before staging.
    ///
    /// Cross-definition duplicates are checked later after profile filtering.
    pub(crate) fn validate_self_keys(&self) -> Result<(), RegistrationError> {
        let mut keys = HashSet::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            if !keys.insert(&binding.key) {
                return Err(RegistrationError::DuplicateDefinitionKey {
                    key: binding.key.clone(),
                    definition: self.source,
                });
            }
        }
        let concrete = self.bindings.first().ok_or(RegistrationError::EmptyDefinition {
            definition: self.source,
        })?;
        if matches!(concrete.kind, PendingBindingKind::Alias { .. }) {
            return Err(RegistrationError::InvalidConcreteBinding {
                key: concrete.key.clone(),
                definition: self.source,
            });
        }
        for binding in self.bindings.iter().skip(1) {
            match &binding.kind {
                PendingBindingKind::Alias { target, .. } if target != &concrete.key => {
                    return Err(RegistrationError::InvalidAliasTarget {
                        alias: binding.key.clone(),
                        target: target.clone(),
                        expected: concrete.key.clone(),
                        definition: self.source,
                    });
                }
                PendingBindingKind::Alias { .. } => {}
                _ => {
                    return Err(RegistrationError::InvalidAliasBinding {
                        key: binding.key.clone(),
                        definition: self.source,
                    });
                }
            }
        }
        Ok(())
    }

    /// Validates every key before appending this complete definition to
    /// `definitions`.
    ///
    /// On a duplicate exact key, returns its source and leaves the staging area
    /// untouched; different definitions may share keys until graph validation.
    pub(crate) fn stage_into(self, definitions: &mut Vec<Self>) -> Result<(), RegistrationError> {
        self.validate_self_keys()?;
        definitions.push(self);
        Ok(())
    }
}
