// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Checks macro output against consumer-local names and crate aliases.

use std::sync::Arc;

use r#type::bean;
use r#type::Configuration;

type Result<T> = std::result::Result<T, std::io::Error>;

#[allow(non_upper_case_globals)]
const Some: () = ();
#[allow(non_upper_case_globals)]
const None: () = ();

/// Value produced by a factory with generated-name-like parameters.
pub struct CollisionResult(u64);

/// Produces an ordinary value while `Result` names a consumer alias.
#[bean]
pub fn ordinary() -> u32 {
    7
}

/// Accepts parameters that resemble implementation-local macro names.
#[bean]
pub fn collision(__qubit_context: Arc<u8>, __qubit_ioc_argument_0: Arc<u16>) -> CollisionResult {
    CollisionResult(u64::from(*__qubit_context) + u64::from(*__qubit_ioc_argument_0))
}

/// Installs a child bean from a module with a local result alias.
#[Configuration]
mod result_shadow {
    use r#type::bean;

    type Result<T> = std::result::Result<T, std::io::Error>;

    #[bean]
    pub(super) fn configured() -> u64 {
        11
    }

    #[cfg_attr(not(feature = "extra"), cfg(any()))]
    #[bean]
    pub(super) fn conditional() -> usize {
        13
    }
}

/// Confirms generated registration code ignores consumer Result aliases.
#[test]
fn test_shadowed_result_and_internal_parameter_names() {
    use r#type::ContainerBuilder;

    let _: Result<()> = std::result::Result::Ok(());
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(2_u8)).expect("register u8");
    builder.register_instance(Arc::new(3_u16)).expect("register u16");
    builder.install::<OrdinaryBean>().expect("install ordinary");
    builder.install::<CollisionBean>().expect("install collision");
    result_shadow::register_ioc(&mut builder).expect("install configuration group");
    let application = builder.build_all().expect("build definitions");
    let context = application.context();
    assert_eq!(*context.get::<u32>().expect("ordinary value"), 7);
    assert_eq!(context.get::<CollisionResult>().expect("collision value").0, 5);
    #[cfg(feature = "extra")]
    assert_eq!(*context.get::<usize>().expect("conditional bean"), 13);
    #[cfg(not(feature = "extra"))]
    assert!(context.try_get::<usize>().expect("conditional bean absent").is_none());
}
