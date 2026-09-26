// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Structured errors for registration, graph validation, construction, and
//! lookup.

use std::error::Error;

use thiserror::Error;

use crate::dependency::Dependency;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// An ID that does not match the binding identifier grammar.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BindingId;
///
/// let error = BindingId::parse("bad-id").expect_err("hyphens are not allowed");
/// assert_eq!(error.value(), "bad-id");
/// ```
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid binding ID `{value}`; expected dot-separated ASCII segments beginning with a letter")]
pub struct InvalidBindingId {
    /// Original text supplied by the caller.
    value: String,
}

impl InvalidBindingId {
    /// Records the original invalid text for diagnostics.
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    /// Returns the original invalid text.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A concrete wrapper retaining the original user factory error as its source.
///
/// # Examples
///
/// ```
/// use qubit_ioc::FactoryError;
///
/// let error = FactoryError::new(std::io::Error::other("connection refused"));
/// assert!(std::error::Error::source(&error).is_some());
/// ```
#[derive(Debug, Error)]
#[error("factory failed: {source}")]
pub struct FactoryError {
    /// Original user or configuration error retained as the source chain.
    #[source]
    source: Box<dyn Error + Send + Sync + 'static>,
    /// Extra target information for generated configuration reads.
    config_read: Option<ConfigReadContext>,
}

/// Path and destination retained for a generated configuration read.
#[derive(Debug)]
struct ConfigReadContext {
    /// Configuration key or subtree prefix that failed to read.
    path_key: String,
    /// Generated field, parameter, or properties type being built.
    target: String,
}

impl FactoryError {
    /// Wraps a user error without discarding its source chain.
    pub fn new<E: Error + Send + Sync + 'static>(source: E) -> Self {
        Self {
            source: Box::new(source),
            config_read: None,
        }
    }

    /// Marks a generated configuration failure while keeping `source` intact.
    ///
    /// The path is the Config lookup key; `target` names the field, parameter,
    /// or properties type being constructed. Construction uses both to report
    /// [`BuildError::ConfigReadFailed`].
    #[cfg(feature = "config")]
    pub(crate) fn config_read<E: Error + Send + Sync + 'static>(source: E, path_key: &str, target: &str) -> Self {
        Self {
            source: Box::new(source),
            config_read: Some(ConfigReadContext {
                path_key: path_key.to_owned(),
                target: target.to_owned(),
            }),
        }
    }

    /// Returns the generated read's path and destination, when one failed.
    ///
    /// `None` indicates an ordinary factory error; `Some` borrows the stored
    /// configuration key and target name.
    pub(crate) fn config_read_context(&self) -> Option<(&str, &str)> {
        self.config_read
            .as_ref()
            .map(|context| (context.path_key.as_str(), context.target.as_str()))
    }
}

/// Errors detected while adding one definition to a builder.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::BindingOptions;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::RegistrationError;
///
/// let mut builder = ContainerBuilder::new();
/// let result = builder.register_instance_with(
///     Arc::new(1_u32),
///     BindingOptions { id: Some("bad-id".to_owned()), ..BindingOptions::default() },
/// );
/// assert!(matches!(result, Err(RegistrationError::InvalidBindingId { .. })));
/// ```
#[derive(Debug, Error)]
pub enum RegistrationError {
    /// An interface alias attempted to use a different activation profile.
    #[error("{definition}: alias profile {alias:?} differs from concrete profile {concrete:?}")]
    AliasProfileMismatch {
        /// Definition containing the alias.
        definition: DefinitionSource,
        /// Concrete binding profile.
        concrete: Option<String>,
        /// Alias binding profile.
        alias: Option<String>,
    },
    /// A replacement registration did not declare its requested exact key.
    #[error("replacement does not declare exact key {key:?}")]
    ReplacementTargetMissing {
        /// The requested replacement key.
        key: BindingKey,
    },
    /// A replacement registration declared its requested key more than once.
    #[error("replacement declares exact key {key:?} at multiple sources: {sources:?}")]
    ReplacementTargetAmbiguous {
        /// The requested replacement key.
        key: BindingKey,
        /// Definitions that declared it.
        sources: Vec<DefinitionSource>,
    },
    /// A pending definition has no concrete binding.
    #[error("{definition}: definition has no concrete binding")]
    EmptyDefinition {
        /// Definition missing its required concrete binding.
        definition: DefinitionSource,
    },
    /// The first binding is an alias rather than an instance or factory.
    #[error("{definition}: first binding {key:?} must be concrete")]
    InvalidConcreteBinding {
        /// Binding in the concrete position.
        key: BindingKey,
        /// Definition containing the binding.
        definition: DefinitionSource,
    },
    /// A binding after the concrete binding is not an alias.
    #[error("{definition}: binding {key:?} after the concrete binding must be an alias")]
    InvalidAliasBinding {
        /// Binding in an alias position.
        key: BindingKey,
        /// Definition containing the binding.
        definition: DefinitionSource,
    },
    /// An interface alias points outside its own definition.
    #[error("{definition}: alias {alias:?} targets {target:?}, expected concrete key {expected:?}")]
    InvalidAliasTarget {
        /// Alias being declared.
        alias: BindingKey,
        /// Supplied target key.
        target: BindingKey,
        /// This definition's concrete binding key.
        expected: BindingKey,
        /// Definition containing the alias.
        definition: DefinitionSource,
    },
    /// One definition declares the same exact binding key more than once.
    #[error("{definition}: duplicate key {key:?} within one definition")]
    DuplicateDefinitionKey {
        /// Repeated key within the definition.
        key: BindingKey,
        /// Definition containing both conflicting declarations.
        definition: DefinitionSource,
    },
    /// A binding or dependency ID is invalid.
    #[error("{definition}: {error}")]
    InvalidBindingId {
        /// Original parse error.
        #[source]
        error: InvalidBindingId,
        /// Definition that supplied the ID.
        definition: DefinitionSource,
    },
    /// A profile name does not match `[A-Za-z][A-Za-z0-9_-]*`.
    #[error("{definition}: invalid profile `{value}`")]
    InvalidProfile {
        /// Original profile text.
        value: String,
        /// Definition that supplied the profile.
        definition: DefinitionSource,
    },
    /// A factory declared the same type, ID, and cardinality twice.
    #[error("{definition}: duplicate dependency {dependency:?}")]
    DuplicateDependency {
        /// Repeated request.
        dependency: Dependency,
        /// Definition that declared it.
        definition: DefinitionSource,
    },
}

/// Errors detected before or during container construction.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BuildError;
/// use qubit_ioc::ContainerBuilder;
///
/// let error = match ContainerBuilder::new().build() {
///     Ok(_) => panic!("a root is required"),
///     Err(error) => error,
/// };
/// assert!(matches!(error, BuildError::NoRootsSelected));
/// ```
#[derive(Debug, Error)]
pub enum BuildError {
    /// A root-scoped build was requested without selecting a root component.
    #[error("no build roots were selected; register at least one root or call build_all")]
    NoRootsSelected,
    /// No active binding satisfies a required root request.
    #[error("missing root {request:?}; available bindings: {available:?}")]
    MissingRoot {
        /// Requested root binding.
        request: Dependency,
        /// Active bindings of the requested type.
        available: Vec<BindingKey>,
    },
    /// An alias still points to a concrete key replaced by another definition.
    #[error("alias {alias:?} targets replaced key {target:?}: {original} was replaced by {replacement}")]
    AliasTargetReplaced {
        /// Alias whose projection was invalidated.
        alias: BindingKey,
        /// Replaced concrete key.
        target: BindingKey,
        /// Source definition that originally owned the key.
        original: DefinitionSource,
        /// Replacement definition now owning the key.
        replacement: DefinitionSource,
    },
    /// Several active bindings satisfy a root request without a unique primary.
    #[error("ambiguous root {request:?}; candidates: {candidates:?}")]
    AmbiguousRoot {
        /// Requested root binding.
        request: Dependency,
        /// Active matching bindings.
        candidates: Vec<BindingKey>,
    },
    /// Two active definitions own the same exact key.
    #[error("duplicate binding {key:?} from {first} and {second}")]
    DuplicateBinding {
        /// Conflicting key.
        key: BindingKey,
        /// First definition.
        first: DefinitionSource,
        /// Second definition.
        second: DefinitionSource,
    },
    /// An alias's concrete target was removed or is otherwise absent after
    /// filtering.
    #[error("alias {alias:?} from {definition} has missing target {target:?}; path: {path:?}")]
    MissingAliasTarget {
        /// Alias whose target cannot be resolved.
        alias: BindingKey,
        /// Missing concrete target key.
        target: BindingKey,
        /// Definition that declared the alias.
        definition: DefinitionSource,
        /// Complete binding path to the alias.
        path: Vec<BindingKey>,
    },
    /// No active binding satisfies a required request.
    #[error("missing dependency {dependency:?} for {definition}; path: {path:?}")]
    MissingDependency {
        /// Unsatisfied request.
        dependency: Dependency,
        /// Requesting definition.
        definition: DefinitionSource,
        /// Complete binding path to the request.
        path: Vec<BindingKey>,
    },
    /// Several bindings satisfy a single request without a unique primary.
    #[error("ambiguous dependency {dependency:?} for {definition}; candidates: {candidates:?}; path: {path:?}")]
    AmbiguousBinding {
        /// Ambiguous request.
        dependency: Dependency,
        /// Requesting definition.
        definition: DefinitionSource,
        /// Matching keys.
        candidates: Vec<BindingKey>,
        /// Complete binding path to the request.
        path: Vec<BindingKey>,
    },
    /// Several active primary bindings exist for one Rust type.
    #[error("multiple primary bindings: {candidates:?}")]
    MultiplePrimaryBindings {
        /// Conflicting primary keys and their sources.
        candidates: Vec<(BindingKey, DefinitionSource)>,
    },
    /// The dependency graph contains a cycle.
    #[error("dependency cycle: {path:?}")]
    DependencyCycle {
        /// Complete cycle path, including the repeated first key.
        path: Vec<BindingKey>,
    },
    /// A synchronous build encountered an active asynchronous factory.
    #[error("asynchronous factory at {definition} requires build_async")]
    AsyncRequired {
        /// Factory definition.
        definition: DefinitionSource,
        /// Factory binding key.
        key: BindingKey,
    },
    /// A user factory failed during construction.
    #[error("factory at {definition} failed for {key:?}; path: {path:?}: {error}")]
    FactoryFailed {
        /// Factory definition.
        definition: DefinitionSource,
        /// Factory binding key.
        key: BindingKey,
        /// Complete dependency path.
        path: Vec<BindingKey>,
        /// Original user factory failure.
        #[source]
        error: FactoryError,
    },
    /// A configuration read failed during construction.
    #[error("configuration read at {definition} for {target} from `{path_key}`; path: {path:?}: {error}")]
    ConfigReadFailed {
        /// Definition containing the read.
        definition: DefinitionSource,
        /// Configuration path.
        path_key: String,
        /// Field or parameter being populated.
        target: String,
        /// Complete dependency path.
        path: Vec<BindingKey>,
        /// Original configuration failure.
        #[source]
        error: FactoryError,
    },
}

/// Errors from a factory's access to its declared dependencies.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::BuildAccessError;
/// use qubit_ioc::ContainerBuilder;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_factory::<u64, _>(&[], |context| {
///     assert!(matches!(context.get::<u32>(), Err(BuildAccessError::UndeclaredDependency { .. })));
///     Ok(Arc::new(1))
/// })?;
/// builder.build_all()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Error)]
pub enum BuildAccessError {
    /// The factory requested a dependency absent from its declaration.
    #[error("factory at {definition} did not declare dependency {dependency:?}")]
    UndeclaredDependency {
        /// Factory definition.
        definition: DefinitionSource,
        /// Requested dependency.
        dependency: Dependency,
    },
}

/// Errors from looking up built components.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::ResolveError;
///
/// let context = ContainerBuilder::new().build_all()?;
/// assert!(matches!(context.get::<String>(), Err(ResolveError::MissingComponent { .. })));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Error)]
pub enum ResolveError {
    /// No built binding satisfies the requested type and ID.
    #[error("missing component {request:?}; available: {available:?}")]
    MissingComponent {
        /// Requested type and optional ID.
        request: BindingKey,
        /// Available keys for that type.
        available: Vec<BindingKey>,
    },
    /// Several built bindings satisfy an unnamed request.
    #[error("ambiguous component {request:?}; candidates: {candidates:?}")]
    AmbiguousBinding {
        /// Requested type.
        request: BindingKey,
        /// Matching keys.
        candidates: Vec<BindingKey>,
    },
    /// The caller supplied an invalid ID at lookup time.
    #[error(transparent)]
    InvalidBindingId(#[from] InvalidBindingId),
}
