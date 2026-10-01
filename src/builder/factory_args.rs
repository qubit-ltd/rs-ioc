// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A zero to eight argument tuple derived from a typed factory signature.

use crate::build_context::BuildContext;
use crate::builder::FactoryArg;
use crate::builder::factory_arg::sealed;
use crate::dependency::Dependency;
use crate::error::BuildAccessError;

/// A zero to eight argument tuple derived from a typed factory signature.
///
/// The trait is sealed so request declarations always match resolution.
pub trait FactoryArgs: sealed::Sealed + Send + 'static {
    /// Returns requests in parameter order, removing repeated identical
    /// requests while preserving first occurrence.
    fn dependencies() -> Vec<Dependency>;

    /// Resolves each declared request into the parameter tuple.
    #[allow(clippy::result_large_err)]
    fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError>
    where
        Self: Sized;
}

impl sealed::Sealed for () {}
impl FactoryArgs for () {
    fn dependencies() -> Vec<Dependency> {
        Vec::new()
    }

    fn resolve(_context: &BuildContext) -> Result<Self, BuildAccessError> {
        Ok(())
    }
}

macro_rules! impl_factory_args {
    ($($argument:ident),+ $(,)?) => {
        impl<$($argument: FactoryArg),+> sealed::Sealed for ($($argument,)+) {}

        impl<$($argument: FactoryArg),+> FactoryArgs for ($($argument,)+) {
            fn dependencies() -> Vec<Dependency> {
                let requests = vec![$($argument::dependency()),+];
                let mut unique = Vec::with_capacity(requests.len());
                for request in requests {
                    if !unique.contains(&request) {
                        unique.push(request);
                    }
                }
                unique
            }

            fn resolve(context: &BuildContext) -> Result<Self, BuildAccessError> {
                Ok(($($argument::resolve(context)?,)+))
            }
        }
    };
}

impl_factory_args!(A);
impl_factory_args!(A, B);
impl_factory_args!(A, B, C);
impl_factory_args!(A, B, C, D);
impl_factory_args!(A, B, C, D, E);
impl_factory_args!(A, B, C, D, E, F);
impl_factory_args!(A, B, C, D, E, F, G);
impl_factory_args!(A, B, C, D, E, F, G, H);
