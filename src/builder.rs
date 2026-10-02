// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit builder registration and validated construction.

// Owns the `ComponentDefinition` registration contract and its inherent
// implementation.
mod component_definition;
// Owns the `ContainerBuilder` state and its inherent implementation.
mod container_builder;
// Maps one typed argument to its declared dependency request.
mod factory_arg;
// Derives dependency declarations and values from typed manual factory
// arguments.
mod factory_args;
// Owns the private construction, validation, replacement, and path helpers.
mod internal;
mod validation_scope;

pub use component_definition::ComponentDefinition;
pub use container_builder::ContainerBuilder;
pub use factory_arg::FactoryArg;
pub use factory_args::FactoryArgs;
pub use validation_scope::ValidationScope;
pub(crate) use internal::validation::validate_dependencies;
pub(crate) use internal::validation::validate_options;
