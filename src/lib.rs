// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Application-level component assembly and sharing for the Qubit ecosystem.

pub use crate as qubit_ioc;

pub mod application;
pub mod application_context;
pub mod application_state;
mod binding;
pub mod build_context;
pub mod builder;
#[cfg(feature = "config")]
pub mod config;
pub mod definition;
pub mod dependency;
pub mod error;
mod graph;
pub mod key;
pub mod managed;
pub mod options;
mod store;

#[doc(hidden)]
pub mod __private;

#[cfg(test)]
mod tests;

pub use application::Application;
pub use application_context::ApplicationContext;
pub use application_state::ApplicationState;
pub use binding::FactoryFuture;
pub use build_context::BuildContext;
pub use builder::ComponentDefinition;
pub use builder::ContainerBuilder;
pub use definition::Definition;
pub use definition::DefinitionBuilder;
pub use dependency::Dependency;
pub use dependency::DependencyCardinality;
pub use error::BuildAccessError;
pub use error::BuildError;
pub use error::BuildFailure;
pub use error::FactoryError;
pub use error::InvalidBindingId;
pub use error::RegistrationError;
pub use error::ResolveError;
pub use key::BindingId;
pub use key::BindingKey;
pub use managed::CleanupError;
pub use managed::DeadlineFuture;
pub use managed::Managed;
pub use managed::ManagedFactoryFuture;
pub use managed::ShutdownError;
pub use managed::ShutdownFailure;
pub use managed::ShutdownHandle;
pub use managed::ShutdownMode;
pub use managed::ShutdownPhase;
pub use managed::ShutdownReport;
pub use managed::WaitPolicy;
pub use options::BindingOptions;
pub use options::DefinitionSource;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::Component;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::Configuration;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::ConfigurationProperties;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::Repository;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::Service;
#[cfg(feature = "macros")]
pub use qubit_ioc_macros::bean;
