// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Declaration macro intermediate representation.

use crate::ir::BeanIr;
use crate::ir::ComponentIr;
use crate::ir::ConfigurationIr;
use crate::ir::ConfigurationPropertiesIr;

/// A declaration after syntax and local semantic validation.
pub(crate) enum Declaration {
    /// Validated component, service, or repository struct.
    Component(ComponentIr),
    /// Validated factory function.
    Bean(Box<BeanIr>),
    /// Validated inline configuration module.
    Configuration(ConfigurationIr),
    /// Validated configuration-backed properties struct.
    ConfigurationProperties(ConfigurationPropertiesIr),
}

impl std::fmt::Debug for Declaration {
    /// Displays only the variant because `syn` syntax trees do not enable extra
    /// debug traits.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Component(_) => "Component",
            Self::Bean(_) => "Bean",
            Self::Configuration(_) => "Configuration",
            Self::ConfigurationProperties(_) => "ConfigurationProperties",
        };
        formatter.write_str(kind)
    }
}
