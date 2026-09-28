// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Expands validated declarations into runtime registration tokens.

mod bean;
mod component;
mod config_properties;
mod configuration;
mod context;
mod dispatch;
mod symbols;
mod value;

pub(crate) use context::ExpansionContext;
pub(crate) use dispatch::dispatch;
