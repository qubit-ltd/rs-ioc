// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildAccessError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;

#[test]
fn test_get_rejects_undeclared_dependency() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance(Arc::new(7_u32))
        .expect("stage available value");
    builder
        .register_factory::<u64, _>(&[], |context| {
            let error = context.get::<u32>().expect_err("undeclared read must fail");
            assert!(matches!(error, BuildAccessError::UndeclaredDependency { .. }));
            Ok(Arc::new(9))
        })
        .expect("stage consumer");
    builder.build().expect("factory must observe access error");
}

#[test]
fn test_get_all_reads_only_declared_collection() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(7_u32)).expect("stage value");
    builder
        .register_factory::<usize, _>(&[Dependency::all::<u32>()], |context| {
            let values = context.get_all::<u32>().expect("declared collection");
            Ok(Arc::new(values.len()))
        })
        .expect("stage collection consumer");
    let context = builder.build().expect("valid graph");
    assert_eq!(*context.get::<usize>().expect("built count"), 1);
}

#[test]
fn test_try_get_by_id_distinguishes_missing_and_selected() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<usize, _>(&[Dependency::optional_with_id::<u32>("missing")], |context| {
            assert!(
                context
                    .try_get_by_id::<u32>("missing")
                    .expect("declared optional")
                    .is_none()
            );
            Ok(Arc::new(1))
        })
        .expect("stage optional consumer");
    let context = builder.build().expect("optional absence is valid");
    assert_eq!(*context.get::<usize>().expect("built value"), 1);
}

#[test]
fn test_try_get_by_id_returns_the_declared_named_component() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_instance_with(
            Arc::new(37_u32),
            BindingOptions {
                id: Some("present".to_owned()),
                ..BindingOptions::default()
            },
        )
        .expect("stage named value");
    builder
        .register_factory::<u64, _>(&[Dependency::optional_with_id::<u32>("present")], |context| {
            let value = context
                .try_get_by_id::<u32>("present")
                .expect("declared optional lookup")
                .expect("named value exists");
            Ok(Arc::new(u64::from(*value)))
        })
        .expect("stage consumer");
    assert_eq!(
        *builder.build().expect("valid graph").get::<u64>().expect("consumer"),
        37
    );
}
