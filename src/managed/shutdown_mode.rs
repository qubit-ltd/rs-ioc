// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Requested application shutdown behavior.

/// Selects dependency-preserving graceful shutdown or immediate cancellation.
///
/// The requested mode is retained by the resulting report even when a failing
/// graceful request upgrades the shutdown to immediate cancellation.
///
/// # Examples
///
/// ```
/// use qubit_ioc::Application;
/// use qubit_ioc::ShutdownMode;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = Application::builder();
/// builder.register_instance(std::sync::Arc::new(String::from("hello")))?;
/// builder.root::<String>();
///
/// let application = builder.build()?;
/// let report = application.begin_shutdown(ShutdownMode::Immediate).abandon();
/// assert_eq!(report.mode(), ShutdownMode::Immediate);
/// assert!(report.is_complete());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownMode {
    /// Stop and confirm each consumer before stopping its dependencies.
    Graceful,
    /// Request every abort before waiting for any component.
    Immediate,
}
