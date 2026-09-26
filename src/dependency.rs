// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dependency requests declared by factories.

use std::any::TypeId;
use std::any::type_name;
use std::hash::Hash;
use std::hash::Hasher;

/// Number of matching bindings requested by a factory.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DependencyCardinality {
    /// Exactly one matching binding must be selected.
    Required,
    /// Zero or one matching binding may be selected.
    Optional,
    /// Every matching binding is selected, possibly none.
    All,
}

/// A typed request whose optional ID remains raw until registration.
#[derive(Clone, Debug)]
pub struct Dependency {
    type_id: TypeId,
    type_name: &'static str,
    id: Option<String>,
    cardinality: DependencyCardinality,
}

impl PartialEq for Dependency {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.id == other.id && self.cardinality == other.cardinality
    }
}

impl Eq for Dependency {}

impl Hash for Dependency {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.id.hash(state);
        self.cardinality.hash(state);
    }
}

impl Dependency {
    /// Requests one binding of `T` by candidate selection.
    pub fn of<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::Required)
    }

    /// Requests the binding of `T` with the exact raw `id`.
    pub fn with_id<T: ?Sized + 'static>(id: &str) -> Self {
        Self::make::<T>(Some(id), DependencyCardinality::Required)
    }

    /// Requests zero or one binding of `T` by candidate selection.
    pub fn optional<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::Optional)
    }

    /// Requests zero or one binding of `T` with the exact raw `id`.
    pub fn optional_with_id<T: ?Sized + 'static>(id: &str) -> Self {
        Self::make::<T>(Some(id), DependencyCardinality::Optional)
    }

    /// Requests all bindings of `T` in collection order.
    pub fn all<T: ?Sized + 'static>() -> Self {
        Self::make::<T>(None, DependencyCardinality::All)
    }

    /// Returns the Rust type identity used to match bindings.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the type name used for diagnostics.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the original ID text, or `None` for candidate selection.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Returns the requested number of matching bindings.
    pub fn cardinality(&self) -> DependencyCardinality {
        self.cardinality
    }

    /// Builds a request while keeping its ID unvalidated until registration.
    fn make<T: ?Sized + 'static>(id: Option<&str>, cardinality: DependencyCardinality) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            id: id.map(str::to_owned),
            cardinality,
        }
    }
}
