// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! One typed factory argument and its graph request.

use std::sync::Arc;

use crate::build_context::BuildContext;
use crate::dependency::Dependency;
use crate::error::BuildAccessError;

pub(super) mod sealed {
    /// Prevents external implementations from separating a type's request
    /// declaration from its resolution behavior.
    pub trait Sealed {}
}

/// One typed factory argument and the dependency request it represents.
///
/// Implementations are provided for `Arc<T>`, `Option<Arc<T>>`, and
/// `Vec<Arc<T>>`. Use [`crate::builder::FactoryArgs`] to combine arguments
/// into a factory parameter tuple.
pub trait FactoryArg: sealed::Sealed + Send + 'static {
    /// Returns the graph request required to resolve this argument.
    fn dependency() -> Dependency;

    /// Resolves this argument from the factory's validated dependency view.
    #[allow(clippy::result_large_err)]
    fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError>
    where
        Self: Sized;
}

impl<T: ?Sized + Send + Sync + 'static> sealed::Sealed for Arc<T> {}
impl<T: ?Sized + Send + Sync + 'static> FactoryArg for Arc<T> {
    fn dependency() -> Dependency {
        Dependency::of::<T>()
    }

    fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError> {
        context.get::<T>()
    }
}

impl<T: ?Sized + Send + Sync + 'static> sealed::Sealed for Option<Arc<T>> {}
impl<T: ?Sized + Send + Sync + 'static> FactoryArg for Option<Arc<T>> {
    fn dependency() -> Dependency {
        Dependency::optional::<T>()
    }

    fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError> {
        context.try_get::<T>()
    }
}

impl<T: ?Sized + Send + Sync + 'static> sealed::Sealed for Vec<Arc<T>> {}
impl<T: ?Sized + Send + Sync + 'static> FactoryArg for Vec<Arc<T>> {
    fn dependency() -> Dependency {
        Dependency::all::<T>()
    }

    fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError> {
        context.get_all::<T>()
    }
}
