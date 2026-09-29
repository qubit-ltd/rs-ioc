// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors returned by the manual fixture assembly path.

use qubit_ioc::BuildFailure;
use qubit_ioc::RegistrationError;

/// Failure during explicit registration or construction.
#[derive(Debug, thiserror::Error)]
pub enum ManualError {
    /// The binding declaration is invalid.
    #[error(transparent)]
    Registration(#[from] RegistrationError),
    /// The declared graph or factory failed.
    #[error(transparent)]
    Build(#[from] BuildFailure),
}
