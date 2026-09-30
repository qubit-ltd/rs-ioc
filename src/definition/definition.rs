// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! An atomically validated concrete component and its interface aliases.

use std::marker::PhantomData;
use std::panic::Location;
use std::sync::Arc;

use super::DefinitionBuilder;
use crate::binding::PendingDefinition;
use crate::options::DefinitionSource;

/// A complete validated definition of `T`, ready for atomic registration.
///
/// `T` may be a concrete type or a thread-safe trait object. Factories and
/// interface projectors execute only when the container constructs the graph.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
///
/// use qubit_ioc::Definition;
///
/// struct Repository;
///
/// let definition = Definition::<Repository>::builder()
///     .instance(Arc::new(Repository))
///     .build()
///     .expect("a concrete instance validates");
/// ```
pub struct Definition<T: ?Sized + Send + Sync + 'static> {
    /// Validated concrete binding and all its aliases.
    pub(super) pending: PendingDefinition,
    /// Preserves the concrete type across erased storage.
    pub(super) marker: PhantomData<Arc<T>>,
}

impl<T: ?Sized + Send + Sync + 'static> Definition<T> {
    /// Creates a builder whose default diagnostic source is this caller.
    #[track_caller]
    #[must_use]
    pub fn builder() -> DefinitionBuilder<T> {
        let location = Location::caller();
        DefinitionBuilder::new(DefinitionSource::new(
            "<explicit>",
            "<explicit>",
            location.file(),
            location.line(),
            location.column(),
            std::any::type_name::<T>(),
        ))
    }

    /// Transfers the validated bindings to the container's staging area.
    pub(crate) fn into_pending(self) -> PendingDefinition {
        self.pending
    }
}
