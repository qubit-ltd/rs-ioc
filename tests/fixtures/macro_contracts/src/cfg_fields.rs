// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Exercises conditional component fields at an external crate boundary.

mod conditional;
mod conditional_beans;
mod conditional_struct;
mod derived_conditional_struct;
mod disabled_struct;
mod disabled_unknown_type;
mod disabled_value;
#[cfg(feature = "config")]
mod enabled_value;
mod nested_condition;

pub use conditional::Conditional;
pub use conditional_beans::ConditionalConfigFactory;
pub use conditional_beans::ConditionalFactory;
pub use conditional_beans::DisabledCfgAttrUnsupportedBean;
pub use conditional_beans::DisabledUnsupportedBean;
pub use conditional_beans::cfg_attr_disabled_unsupported;
pub use conditional_beans::conditional_bean;
pub use conditional_beans::conditional_config_bean;
pub use conditional_beans::disabled_unsupported;
pub use conditional_beans::ordinary_cfg_attr_disabled;
pub use conditional_beans::ordinary_disabled;
#[cfg(feature = "extra")]
pub use conditional_struct::ConditionalStruct;
pub use derived_conditional_struct::DerivedConditionalStruct;
pub use disabled_unknown_type::DisabledUnknownType;
pub use disabled_value::DisabledValue;
#[cfg(feature = "config")]
pub use enabled_value::EnabledValue;
pub use nested_condition::NestedCondition;
