// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use r#type::bean;

#[bean(marker = InvalidBean)]
fn invalid(#[cfg_attr(all(), cfg(all()))] value: u8) -> u8 {
    value
}

fn main() {}
