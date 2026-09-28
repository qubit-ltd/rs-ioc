// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A typed dependency request.

use std::any::TypeId;
use std::any::type_name;
use std::hash::Hash;
use std::hash::Hasher;

use super::DependencyCardinality;

/// A typed request whose optional ID remains raw until registration.
///
/// # Examples
///
/// ```
/// use qubit_ioc::Dependency;
/// use qubit_ioc::DependencyCardinality;
///
/// let request = Dependency::with_id::<String>("primary");
/// assert_eq!(request.id(), Some("primary"));
/// assert_eq!(request.cardinality(), DependencyCardinality::Required);
/// ```
#[derive(Clone, Debug)]
pub struct Dependency {
    /// Runtime type identity used to select candidate bindings.
    type_id: TypeId,
    /// Readable type name included in diagnostics.
    type_name: &'static str,
    /// Raw optional ID, validated when the request is registered.
    id: Option<String>,
    /// Number of matching bindings the factory requests.
    cardinality: DependencyCardinality,
}

impl PartialEq for Dependency {
    /// Compares type, optional ID, and request cardinality.
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.id == other.id && self.cardinality == other.cardinality
    }
}

impl Eq for Dependency {}

impl Hash for Dependency {
    /// Hashes the same request identity components used by equality.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.id.hash(state);
        self.cardinality.hash(state);
    }
}

impl Dependency {
    /// Requests one binding of `T` by candidate selection.
    ///
    /// The request is required and does not constrain an ID.
    ///
    /// # Type Parameters
    ///
    /// `T` is the requested Rust type and must be `'static` for runtime type
    /// identity tracking.
    ///
    /// # Returns
    ///
    /// A required request for any binding in `T`'s type namespace.
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::Required)
    }

    /// Requests the binding of `T` with the exact raw `id`.
    ///
    /// The ID is validated when this request is registered with a builder.
    ///
    /// # Parameters
    ///
    /// `id` is the exact identifier text retained until registration.
    ///
    /// # Type Parameters
    ///
    /// `T` is the requested Rust type and must be `'static` for runtime type
    /// identity tracking.
    ///
    /// # Returns
    ///
    /// A required request for a binding with the exact raw identifier.
    pub fn with_id<T: ?Sized + 'static>(id: &str) -> Self {
        Self::make::<T>(Some(id), DependencyCardinality::Required)
    }

    /// Requests zero or one binding of `T` by candidate selection.
    ///
    /// Absence is permitted; ambiguity still fails graph validation.
    ///
    /// # Type Parameters
    ///
    /// `T` is the requested Rust type and must be `'static` for runtime type
    /// identity tracking.
    ///
    /// # Returns
    ///
    /// An optional request for any binding in `T`'s type namespace.
    pub fn optional<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::Optional)
    }

    /// Requests zero or one binding of `T` with the exact raw `id`.
    ///
    /// Absence is permitted and the ID is validated during registration.
    ///
    /// # Type Parameters
    ///
    /// `T` is the requested Rust type and must be `'static` for runtime type
    /// identity tracking.
    ///
    /// # Parameters
    ///
    /// `id` is the exact identifier text retained until registration.
    ///
    /// # Returns
    ///
    /// An optional request for a binding with the exact raw identifier.
    pub fn optional_with_id<T: ?Sized + 'static>(id: &str) -> Self {
        Self::make::<T>(Some(id), DependencyCardinality::Optional)
    }

    /// Requests all bindings of `T` in collection order.
    ///
    /// No candidates produce an empty collection.
    ///
    /// # Type Parameters
    ///
    /// `T` is the requested Rust type and must be `'static` for runtime type
    /// identity tracking.
    ///
    /// # Returns
    ///
    /// A collection request for every matching binding in collection order.
    pub fn all<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::All)
    }

    /// Returns the Rust type identity used to match bindings.
    ///
    /// # Returns
    ///
    /// The process-local `TypeId` captured for the requested type.
    #[must_use]
    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the type name used for diagnostics.
    #[must_use]
    #[inline]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the original ID text, or `None` for candidate selection.
    #[must_use]
    #[inline]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Returns the requested number of matching bindings.
    #[must_use]
    #[inline]
    pub fn cardinality(&self) -> DependencyCardinality {
        self.cardinality
    }

    /// Creates a normalized request while keeping its ID unvalidated until
    /// registration.
    fn make<T: ?Sized + 'static>(id: Option<&str>, cardinality: DependencyCardinality) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            id: id.map(str::to_owned),
            cardinality,
        }
    }
}
