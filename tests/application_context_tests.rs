// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::ApplicationContext;
use qubit_ioc::BindingOptions;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::ResolveError;

#[test]
fn test_get_reuses_shared_arc() {
    let instance = Arc::new(String::from("shared"));
    let mut builder = ApplicationContext::builder();
    builder
        .register_instance(Arc::clone(&instance))
        .expect("stage instance");
    let context = builder.build().expect("build context");
    let first = context.get::<String>().expect("first lookup");
    let second = context.get::<String>().expect("second lookup");
    assert!(Arc::ptr_eq(&instance, &first));
    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn test_get_by_id_and_get_all_share_instances() {
    let first = Arc::new(11_u32);
    let second = Arc::new(22_u32);
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::clone(&first),
            BindingOptions {
                id: Some("first".into()),
                order: 2,
                ..BindingOptions::default()
            },
        )
        .expect("stage first");
    builder
        .register_instance_with(
            Arc::clone(&second),
            BindingOptions {
                id: Some("second".into()),
                order: 1,
                ..BindingOptions::default()
            },
        )
        .expect("stage second");
    let context = builder.build().expect("build context");
    assert!(Arc::ptr_eq(
        &first,
        &context.get_by_id::<u32>("first").expect("named lookup")
    ));
    let all = context.get_all::<u32>();
    assert_eq!(all.iter().map(|value| **value).collect::<Vec<_>>(), vec![22, 11]);
    assert!(matches!(
        context.get::<u32>(),
        Err(ResolveError::AmbiguousBinding { .. })
    ));
}

#[test]
fn test_equal_order_sorts_by_id_and_missing_lookup_lists_available_keys() {
    let mut builder = ContainerBuilder::new();
    for (id, value) in [("zeta", 26_u32), ("alpha", 1_u32)] {
        builder
            .register_instance_with(
                Arc::new(value),
                BindingOptions {
                    id: Some(id.to_owned()),
                    order: 0,
                    ..BindingOptions::default()
                },
            )
            .expect("stage named value");
    }
    let context = builder.build().expect("build named values");
    assert_eq!(
        context.get_all::<u32>().iter().map(|value| **value).collect::<Vec<_>>(),
        [1, 26]
    );
    let error = context.get_by_id::<u32>("missing").expect_err("unknown ID must fail");
    match error {
        ResolveError::MissingComponent { request, available } => {
            assert_eq!(request.id().expect("requested ID").as_str(), "missing");
            assert_eq!(available.len(), 2);
            assert!(
                available
                    .iter()
                    .any(|key| key.id().is_some_and(|id| id.as_str() == "alpha"))
            );
            assert!(
                available
                    .iter()
                    .any(|key| key.id().is_some_and(|id| id.as_str() == "zeta"))
            );
        }
        other => panic!("expected missing component, got {other:?}"),
    }
    assert!(matches!(
        context.try_get::<u32>(),
        Err(ResolveError::AmbiguousBinding { .. })
    ));
}
