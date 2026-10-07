// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured errors for registration, graph validation, construction, and
//! lookup.

// Owns the `BuildAccessError` family raised when a factory resolves an
// undeclared dependency.
mod build_access_error;
// Owns the `BuildError` family raised while validating and constructing the
// component graph.
mod build_error;
// Owns `BuildFailure`, the immediate construction failure that also owns the
// deferred rollback handle.
mod build_failure;
// Owns `FactoryError`, wrapping a user factory failure or a configuration read
// failure.
mod factory_error;
// Owns the private error metadata shared by the error owners above.
mod internal;
// Owns `InvalidBindingId`, raised for identifiers outside the binding grammar.
mod invalid_binding_id;
// Owns the `RegistrationError` family raised while registering definitions.
mod registration_error;
// Owns the `ResolveError` family raised when querying built component
// instances.
mod resolve_error;
// Owns a construction failure after its rollback wait has completed.
mod settled_build_failure;

pub use build_access_error::BuildAccessError;
pub use build_error::BuildError;
pub use build_failure::BuildFailure;
pub use factory_error::FactoryError;
pub use invalid_binding_id::InvalidBindingId;
pub use registration_error::RegistrationError;
pub use resolve_error::ResolveError;
pub use settled_build_failure::SettledBuildFailure;

mod build_session_error;
pub use build_session_error::BuildSessionError;
