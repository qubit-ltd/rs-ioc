// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Parsed option and declaration records.

pub(crate) mod raw_declaration;
pub(crate) mod raw_option;
pub(crate) mod raw_value;

pub(crate) use raw_declaration::RawDeclaration;
pub(crate) use raw_option::RawOption;
pub(crate) use raw_value::RawValue;
