// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Application-level component assembly and sharing for the Qubit ecosystem.
//!
//! A container describes every shared component once, validates the whole
//! dependency graph before any factory runs, and builds the reachable set into
//! an [`Application`]. A missing or ambiguous component therefore fails while
//! the application starts instead of on a later request.
//!
//! [`Definition`]s bind component types to an identifier, a scope and a
//! factory. They are staged on a [`ContainerBuilder`], roots are selected, and
//! [`ContainerBuilder::build`] returns the [`Application`] that owns the built
//! instances. [`Application::context`] hands out a cloneable, read-only
//! [`ApplicationContext`] that resolves a component by its type or by an
//! explicit [`BindingKey`]; the clones stay valid until the application is
//! dropped, and a clone may outlive a completed shutdown.
//!
//! A component that owns an external resource is produced by a [`Managed`]
//! factory, so the application can shut it down. A managed graph requires an
//! explicit [`WaitPolicy`], and a normal exit calls
//! [`Application::begin_shutdown`] and awaits the returned
//! [`ShutdownHandle`].
//!
//! Registration order does not matter. A component is only reachable from
//! outside after it is selected as a root, so [`ContainerBuilder::build`]
//! requires at least one selected root and constructs nothing else.
//!
//! # Examples
//!
//! ```
//! use std::sync::Arc;
//!
//! use qubit_ioc::ContainerBuilder;
//! use qubit_ioc::Dependency;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut builder = ContainerBuilder::new();
//! builder.register_instance(Arc::new(String::from("hello")))?;
//! builder.register_factory::<usize, _>(&[Dependency::of::<String>()], |context| {
//!     let message = context.get::<String>().expect("declared dependency");
//!     Ok(Arc::new(message.len()))
//! })?;
//! builder.root::<usize>();
//!
//! let application = builder.build()?;
//! let context = application.context();
//! assert_eq!(*context.get::<usize>()?, 5);
//! # Ok(())
//! # }
//! ```

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
pub use builder::FactoryArg;
pub use builder::FactoryArgs;
pub use builder::ValidationScope;
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
pub use error::SettledBuildFailure;
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
