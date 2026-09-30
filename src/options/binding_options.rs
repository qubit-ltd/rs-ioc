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
/// assert_eq!(*builder.build_all()?.context().get::<u32>()?, 7);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindingOptions {
    /// Original binding ID text, if supplied.
    ///
    /// `None` lets the container derive the key from the registered type. A
    /// supplied value must parse as a `BindingId`, and registration rejects a
    /// malformed one with the source of the definition that carried it.
    pub id: Option<String>,
    /// Select this binding from otherwise ambiguous unnamed requests.
    ///
    /// A single-binding request that matches several candidates resolves only
    /// when exactly one of them is primary; otherwise the dependency is
    /// reported as ambiguous.
    pub primary: bool,
    /// Sort order for collection requests.
    ///
    /// Collection requests receive candidates in ascending `order`, with equal
    /// values broken by binding ID and then by definition source so the result
    /// stays deterministic. Requests for a single binding ignore this value.
    pub order: i32,
    /// Optional activation profile.
    ///
    /// `None` keeps the binding in every build. A supplied value must satisfy
    /// the profile grammar and selects the binding only while that profile is
    /// active; an application that configures no active profile activates
    /// `default` only.
    pub profile: Option<String>,
}
