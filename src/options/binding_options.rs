// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Options supplied when registering a binding.

/// Caller-provided registration options.
///
/// `id` stays unvalidated until the definition is registered, so that
/// registration errors can include its source.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::BindingOptions;
/// use qubit_ioc::ContainerBuilder;
///
/// let options = BindingOptions { primary: true, order: 2, ..BindingOptions::default() };
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance_with(Arc::new(7_u32), options)?;
/// assert_eq!(*builder.build_all()?.get::<u32>()?, 7);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindingOptions {
    /// Original binding ID text, if supplied.
    pub id: Option<String>,
    /// Select this binding from otherwise ambiguous unnamed requests.
    pub primary: bool,
    /// Sort order for collection requests.
    pub order: i32,
    /// Optional activation profile.
    pub profile: Option<String>,
}
