// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_ioc::Component;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::bean;

#[Component]
struct Example {
    #[doc = "retained"]
    #[cfg(any())]
    #[inject(unknown = "x")]
    unknown: Undefined,
    #[cfg(any())]
    #[inject(id = "bad-id")]
    invalid_id: Undefined,
    #[cfg(any())]
    #[inject]
    #[inject]
    duplicate: Undefined,
    #[cfg(any())]
    #[inject(id = 1)]
    non_string: Undefined,
    #[cfg(any())]
    #[value(1)]
    invalid_value: Undefined,
    #[cfg(any())]
    #[inject]
    #[value("ignored")]
    conflict: Undefined,
    #[cfg_attr(all(), cfg(any()))]
    #[inject(unknown = "x")]
    nested: Undefined,
}

#[bean]
fn value(
    #[doc = "retained"]
    #[cfg(any())]
    #[inject(unknown = "x")]
    unknown: Undefined,
    #[cfg(any())]
    #[inject(id = "bad-id")]
    invalid_id: Undefined,
    #[cfg(any())]
    #[inject]
    #[inject]
    duplicate: Undefined,
    #[cfg(any())]
    #[inject(id = 1)]
    non_string: Undefined,
    #[cfg(any())]
    #[value(1)]
    invalid_value: Undefined,
    #[cfg(any())]
    #[inject]
    #[value("ignored")]
    conflict: Undefined,
    #[cfg_attr(all(), cfg(any()))]
    #[inject(unknown = "x")]
    nested: Undefined,
) -> u32 {
    1
}

fn main() {
    let mut builder = ContainerBuilder::new();
    builder.install::<Example>().unwrap();
    builder.install::<ValueBean>().unwrap();
    let application = builder.build_all().unwrap();
    assert_eq!(*application.context().get::<u32>().unwrap(), 1);
}
