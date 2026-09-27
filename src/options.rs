// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Registration options and definition provenance.

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

/// Stable source location for a registered definition.
///
/// # Examples
///
/// ```
/// use qubit_ioc::DefinitionSource;
///
/// let source = DefinitionSource::new("app", "app::services", "src/services.rs", 12, 3, "Database");
/// assert_eq!(source.to_string(), "src/services.rs:12:3 (Database)");
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DefinitionSource {
    /// Cargo package containing the definition.
    pub package: &'static str,
    /// Rust module containing the definition.
    pub module_path: &'static str,
    /// Source file path.
    pub file: &'static str,
    /// One-based source line.
    pub line: u32,
    /// One-based source column.
    pub column: u32,
    /// Name of the declared item.
    pub item: &'static str,
}

impl DefinitionSource {
    /// Creates a source value from static compiler metadata.
    ///
    /// # Parameters
    ///
    /// `package` and `module_path` identify the defining crate and module;
    /// `file`, `line`, and `column` identify the source location; `item` names
    /// the declared component.
    ///
    /// # Returns
    ///
    /// A source location suitable for diagnostics and stable ordering.
    pub const fn new(
        package: &'static str,
        module_path: &'static str,
        file: &'static str,
        line: u32,
        column: u32,
        item: &'static str,
    ) -> Self {
        Self {
            package,
            module_path,
            file,
            line,
            column,
            item,
        }
    }
}

impl std::fmt::Display for DefinitionSource {
    /// Formats the source file, line, column, and declared item name.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}:{} ({})", self.file, self.line, self.column, self.item)
    }
}
