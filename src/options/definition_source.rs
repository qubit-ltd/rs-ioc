// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Source location retained for diagnostics.

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
    #[must_use]
    #[inline]
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
