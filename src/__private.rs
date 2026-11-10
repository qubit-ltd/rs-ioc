// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Config feature diagnostics and type glue for generated code.

/// Gives generated config consumers an explicit diagnostic when the runtime
/// dependency was compiled without its `config` feature.
#[cfg(feature = "config")]
#[doc(hidden)]
#[macro_export]
macro_rules! require_config {
    ($($tokens:tt)*) => { $($tokens)* };
}

/// Reports the missing feature at the consuming declaration.
#[cfg(not(feature = "config"))]
#[doc(hidden)]
#[macro_export]
macro_rules! require_config {
    ($($tokens:tt)*) => {
        compile_error!("qubit-ioc: #[value] and #[ConfigurationProperties] require the `config` feature");
    };
}

#[doc(hidden)]
pub use crate::require_config;

/// Config type bridge for consumers without a direct config dependency.
pub mod codegen_v1 {
    #[cfg(feature = "config")]
    pub use qubit_config::Config;
}
