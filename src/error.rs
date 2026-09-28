// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Structured errors for registration, graph validation, construction, and
//! lookup.

mod build_access_error;
mod build_error;
mod factory_error;
mod invalid_binding_id;
mod registration_error;
mod resolve_error;

pub use build_access_error::BuildAccessError;
pub use build_error::BuildError;
pub use factory_error::FactoryError;
pub use invalid_binding_id::InvalidBindingId;
pub use registration_error::RegistrationError;
pub use resolve_error::ResolveError;
