// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Hosts the value produced by a factory with generated-name-like parameters.
//!
//! The consumer-local `Result` alias and the `Option` variant shadows live
//! here because this module is the one whose generated registration code must
//! fail to resolve them.

use std::sync::Arc;

use r#type::bean;

/// Consumer-local alias that shadows the `Result` name that generated
/// registration code would otherwise resolve in this module.
type Result<T> = std::result::Result<T, std::io::Error>;

/// Shadows the `Option::Some` variant name for generated code in this module.
#[allow(non_upper_case_globals)]
const Some: () = ();
/// Shadows the `Option::None` variant name for generated code in this module.
#[allow(non_upper_case_globals)]
const None: () = ();

/// Value produced by a factory with generated-name-like parameters.
///
/// The wrapped count stays module-private on purpose: it is a consumer-local
/// detail that no outer test boundary is allowed to read.
pub struct CollisionResult(
    /// Sum of the two injected dependency values.
    u64,
);

/// Produces an ordinary value while `Result` names a consumer alias.
#[must_use]
#[bean]
pub fn ordinary() -> u32 {
    7
}

/// Accepts parameters that resemble implementation-local macro names.
#[must_use]
#[bean]
pub fn collision(__qubit_context: Arc<u8>, __qubit_ioc_argument_0: Arc<u16>) -> CollisionResult {
    CollisionResult(u64::from(*__qubit_context) + u64::from(*__qubit_ioc_argument_0))
}

/// Confirms the generated factories install past every shadowed local name.
///
/// This stays inline because it needs the module-private `Result` alias, the
/// `pub(super)` `register_ioc` seam, and the private `CollisionResult` field
/// that no outer test boundary can name.
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use r#type::ContainerBuilder;

    use super::super::result_shadow;
    use super::CollisionBean;
    use super::CollisionResult;
    use super::OrdinaryBean;
    use super::Result;

    #[test]
    fn test_configuration_module_ignores_consumer_result_alias() {
        let _: Result<()> = std::result::Result::Ok(());
        let mut builder = ContainerBuilder::new();
        builder.register_instance(Arc::new(2_u8)).expect("register u8");
        builder.register_instance(Arc::new(3_u16)).expect("register u16");
        builder.install::<OrdinaryBean>().expect("install ordinary");
        result_shadow::register_ioc(&mut builder).expect("install configuration group");
        let application = builder.build_all().expect("build definitions");
        let context = application.context();
        assert_eq!(*context.get::<u64>().expect("configured value"), 11);
        #[cfg(feature = "extra")]
        assert_eq!(*context.get::<usize>().expect("conditional bean"), 13);
        #[cfg(not(feature = "extra"))]
        assert!(context.try_get::<usize>().expect("conditional bean absent").is_none());
    }

    #[test]
    fn test_collision_value_comes_from_the_injected_dependencies() {
        let mut builder = ContainerBuilder::new();
        builder.register_instance(Arc::new(2_u8)).expect("register u8");
        builder.register_instance(Arc::new(3_u16)).expect("register u16");
        builder.install::<CollisionBean>().expect("install collision");
        let application = builder.build_all().expect("build definitions");
        let context = application.context();
        assert_eq!(context.get::<CollisionResult>().expect("collision value").0, 5);
    }
}
