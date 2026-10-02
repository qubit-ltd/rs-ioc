// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_ioc::BindingOptions;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::FactoryError;
use qubit_ioc::RegistrationError;
use qubit_ioc::ValidationScope;
use qubit_ioc::BindingKey;

struct Wanted;
struct Unused;
struct Leaf;
struct RootService;
struct MiddleService;
struct MissingService;
struct FailingDependency;
struct SelectedConsumer;
struct UnselectedConsumer;

#[test]
fn test_default_reachable_ignores_unselected_duplicate() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Wanted)).expect("root");
    builder.register_instance(Arc::new(Unused)).expect("first unused");
    builder.register_instance(Arc::new(Unused)).expect("second unused");
    builder.root::<Wanted>();
    assert!(builder.build().is_ok());
}

#[test]
fn test_all_active_rejects_unselected_duplicate_before_factory() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    let factory_calls = Arc::clone(&calls);
    builder.register_factory::<Wanted, _>(&[], move |_| {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Wanted))
    }).expect("root factory");
    builder.register_instance(Arc::new(Unused)).expect("first unused");
    builder.register_instance(Arc::new(Unused)).expect("second unused");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::DuplicateBinding { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_all_active_rejects_unselected_primary_conflict() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_instance(Arc::new(Wanted)).expect("root");
    for id in ["first", "second"] {
        builder.register_instance_with(Arc::new(Unused), BindingOptions {
            id: Some(id.to_owned()), primary: true, ..BindingOptions::default()
        }).expect("unused primary");
    }
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MultiplePrimaryBindings { .. })));
}

#[test]
fn test_all_active_rejects_unselected_missing_dependency_before_factory() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_instance(Arc::new(Wanted)).expect("root");
    builder.register_factory::<Unused, _>(&[Dependency::of::<MissingService>()], |_| {
        panic!("factory must not run before validation")
    }).expect("unused factory");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MissingDependency { .. })));
}

#[test]
fn test_all_active_rejects_unselected_ambiguous_dependency() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_instance(Arc::new(Wanted)).expect("root");
    for id in ["first", "second"] {
        builder.register_instance_with(Arc::new(Leaf), BindingOptions {
            id: Some(id.to_owned()), ..BindingOptions::default()
        }).expect("candidate");
    }
    builder.register_factory::<Unused, _>(&[Dependency::of::<Leaf>()], |_| Ok(Arc::new(Unused))).expect("unused consumer");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::AmbiguousBinding { .. })));
}

#[test]
fn test_all_active_rejects_unselected_dependency_cycle() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_instance(Arc::new(Wanted)).expect("root");
    builder.register_factory::<Unused, _>(&[Dependency::of::<Leaf>()], |_| Ok(Arc::new(Unused))).expect("unused consumer");
    builder.register_factory::<Leaf, _>(&[Dependency::of::<Unused>()], |_| Ok(Arc::new(Leaf))).expect("cycle member");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::DependencyCycle { .. })));
}

#[test]
fn test_all_active_filters_profiles_and_applies_replacement_before_preflight() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_instance(Arc::new(Wanted)).expect("root");
    builder.register_factory_with::<Unused, _>(&[Dependency::of::<MissingService>()], BindingOptions {
        profile: Some("prod".to_owned()), ..BindingOptions::default()
    }, |_| Ok(Arc::new(Unused))).expect("inactive invalid factory");
    builder.register_instance(Arc::new(Leaf)).expect("original");
    builder.replace_definition(BindingKey::of::<Leaf>(None), |draft| draft.register_instance(Arc::new(Leaf))).expect("replacement");
    builder.root::<Wanted>();
    let application = builder.build().expect("filtered and replaced graph validates");
    assert!(application.context().get::<Wanted>().is_ok());
    assert!(application.context().get::<Leaf>().is_err());
}

#[test]
fn test_all_active_only_constructs_root_and_ignores_unselected_async_and_managed_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    let root_calls = Arc::clone(&calls);
    builder.register_factory::<Wanted, _>(&[], move |_| {
        root_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Wanted))
    }).expect("root factory");
    builder.register_async_factory::<Unused, _>(&[], |_| panic!("unselected async factory"))
        .expect("unused async factory");
    builder.register_managed_factory::<Leaf, _>(&[], |_| panic!("unselected managed factory"))
        .expect("unused managed factory");
    builder.root::<Wanted>();
    let application = builder.build().expect("only root closure requires synchronous construction or wait policy");
    assert!(application.context().get::<Wanted>().is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_all_active_async_build_validates_before_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    let root_calls = Arc::clone(&calls);
    builder.register_factory::<Wanted, _>(&[], move |_| {
        root_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Wanted))
    }).expect("root factory");
    builder.register_factory::<Unused, _>(&[Dependency::of::<MissingService>()], |_| {
        panic!("unselected factory must not run")
    }).expect("unused invalid factory");
    builder.root::<Wanted>();
    let runtime = tokio::runtime::Builder::new_current_thread().build().expect("test runtime");
    assert!(matches!(runtime.block_on(builder.build_async()), Err(failure) if matches!(failure.cause(), BuildError::MissingDependency { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_all_active_async_build_only_constructs_root_closure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    let root_calls = Arc::clone(&calls);
    builder.register_factory::<Wanted, _>(&[], move |_| {
        root_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(Wanted))
    }).expect("root factory");
    builder.register_managed_async_factory::<Unused, _>(&[], |_| panic!("unselected managed async factory"))
        .expect("unused factory");
    builder.root::<Wanted>();
    let runtime = tokio::runtime::Builder::new_current_thread().build().expect("test runtime");
    let application = runtime.block_on(builder.build_async()).expect("unselected managed factory needs no wait policy");
    assert!(application.context().get::<Wanted>().is_ok());
    assert!(application.context().get::<Unused>().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_all_active_reports_graph_error_before_missing_root() {
    let mut builder = ContainerBuilder::new().validation_scope(ValidationScope::AllActive);
    builder.register_factory::<Unused, _>(&[Dependency::of::<MissingService>()], |_| Ok(Arc::new(Unused)))
        .expect("invalid active factory");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MissingDependency { .. })));
}

#[test]
fn test_build_all_runs_every_staged_factory_even_when_a_root_is_registered() {
    let wanted_calls = Arc::new(AtomicUsize::new(0));
    let unused_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();

    let calls = Arc::clone(&wanted_calls);
    builder
        .register_factory::<Wanted, _>(&[], move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Wanted))
        })
        .expect("stage wanted factory");
    let calls = Arc::clone(&unused_calls);
    builder
        .register_factory::<Unused, _>(&[], move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Unused))
        })
        .expect("stage unused factory");
    builder.root::<Wanted>();

    let application = builder.build_all().expect("full graph builds");

    let context = application.context();
    assert_eq!(wanted_calls.load(Ordering::SeqCst), 1);
    assert_eq!(unused_calls.load(Ordering::SeqCst), 1);
    assert!(context.get::<Unused>().is_ok());
}

#[test]
fn test_build_requires_at_least_one_root_before_running_factories() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    let factory_calls = Arc::clone(&calls);
    builder
        .register_factory::<Wanted, _>(&[], move |_| {
            factory_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Wanted))
        })
        .expect("stage wanted factory");

    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::NoRootsSelected)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_root_by_id_rejects_invalid_id_at_registration() {
    let mut builder = ContainerBuilder::new();
    assert!(matches!(
        builder.root_by_id::<Wanted>("invalid-id"),
        Err(RegistrationError::InvalidBindingId { error, .. }) if error.value() == "invalid-id"
    ));
}

#[test]
fn test_build_all_accepts_an_empty_graph() {
    let application = ContainerBuilder::new().build_all().expect("empty full graph builds");
    let context = application.context();
    assert!(context.get_all::<Wanted>().is_empty());
}

#[test]
fn test_root_build_constructs_only_the_selected_definition_closure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Wanted)).expect("wanted instance");
    let calls_in_factory = Arc::clone(&calls);
    builder
        .register_factory::<Unused, _>(&[], move |_| {
            calls_in_factory.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(Unused))
        })
        .expect("unused factory");
    builder.root::<Wanted>();

    let application = builder.build().expect("root closure builds");

    let context = application.context();
    assert!(context.get::<Wanted>().is_ok());
    assert!(context.get::<Unused>().is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn test_missing_root_reports_available_bindings() {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Unused)).expect("unused instance");
    builder.root::<Wanted>();
    assert!(matches!(builder.build(), Err(failure) if matches!(failure.cause(), BuildError::MissingRoot { .. })));
}

#[test]
fn test_root_build_missing_dependency_reports_complete_path() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<RootService, _>(&[Dependency::of::<MiddleService>()], |context| {
            let _ = context.get::<MiddleService>().expect("declared dependency");
            Ok(Arc::new(RootService))
        })
        .expect("stage root factory");
    builder
        .register_factory::<MiddleService, _>(&[Dependency::of::<MissingService>()], |_| Ok(Arc::new(MiddleService)))
        .expect("stage middle factory");
    builder.root::<RootService>();

    let error = match builder.build() {
        Ok(_) => panic!("missing nested dependency must fail"),
        Err(error) => error,
    };
    match error.cause() {
        BuildError::MissingDependency { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<RootService>());
            assert_eq!(path[1].type_name(), std::any::type_name::<MiddleService>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_root_build_factory_failure_ignores_unselected_consumers() {
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<UnselectedConsumer, _>(&[Dependency::of::<FailingDependency>()], |context| {
            let _ = context.get::<FailingDependency>().expect("declared dependency");
            Ok(Arc::new(UnselectedConsumer))
        })
        .expect("stage unselected consumer");
    builder
        .register_factory::<SelectedConsumer, _>(&[Dependency::of::<FailingDependency>()], |context| {
            let _ = context.get::<FailingDependency>().expect("declared dependency");
            Ok(Arc::new(SelectedConsumer))
        })
        .expect("stage selected consumer");
    builder
        .register_factory::<FailingDependency, _>(&[], |_| {
            Err(FactoryError::new(std::io::Error::other("expected failure")))
        })
        .expect("stage failing dependency");
    builder.root::<SelectedConsumer>();

    let error = match builder.build() {
        Ok(_) => panic!("dependency factory must fail"),
        Err(error) => error,
    };
    match error.cause() {
        BuildError::FactoryFailed { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0].type_name(), std::any::type_name::<SelectedConsumer>());
            assert_eq!(path[1].type_name(), std::any::type_name::<FailingDependency>());
        }
        other => panic!("unexpected build error: {other}"),
    }
}

#[test]
fn test_root_build_duplicate_binding_reports_both_definition_sources() {
    let mut builder = ContainerBuilder::new();
    let first_line = line!() + 1;
    builder.register_instance(Arc::new(Leaf)).expect("stage first leaf");
    let second_line = line!() + 1;
    builder.register_instance(Arc::new(Leaf)).expect("stage second leaf");
    builder
        .register_factory::<RootService, _>(&[Dependency::all::<Leaf>()], |_| Ok(Arc::new(RootService)))
        .expect("stage root service");
    builder.root::<RootService>();

    let error = match builder.build() {
        Ok(_) => panic!("selected duplicate leaves must fail graph validation"),
        Err(error) => error,
    };
    assert!(matches!(
        error.cause(),
        BuildError::DuplicateBinding { first, second, .. }
            if first.file == file!()
                && second.file == file!()
                && first.line == first_line
                && second.line == second_line
                && first != second
    ));
}

#[test]
fn test_root_build_skips_unrelated_definitions_in_wide_graph() {
    const UNSELECTED_COUNT: usize = 1_000;
    const CHAIN_COUNT: usize = 100;
    let unselected_calls = Arc::new(AtomicUsize::new(0));
    let selected_calls = Arc::new(AtomicUsize::new(0));
    let mut builder = ContainerBuilder::new();

    for index in 0..UNSELECTED_COUNT {
        let calls = Arc::clone(&unselected_calls);
        builder
            .register_factory_with::<u32, _>(
                &[],
                BindingOptions {
                    id: Some(format!("unused.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::new(index as u32))
                },
            )
            .expect("stage unrelated definition");
    }
    for index in 0..CHAIN_COUNT {
        let calls = Arc::clone(&selected_calls);
        let dependencies = if index + 1 < CHAIN_COUNT {
            vec![Dependency::with_id::<u32>(&format!("chain.n{}", index + 1))]
        } else {
            Vec::new()
        };
        builder
            .register_factory_with::<u32, _>(
                &dependencies,
                BindingOptions {
                    id: Some(format!("chain.n{index}")),
                    ..BindingOptions::default()
                },
                move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(Arc::new(index as u32))
                },
            )
            .expect("stage selected chain node");
    }
    builder.root_by_id::<u32>("chain.n0").expect("select chain root");

    let _context = builder.build().expect("selected root closure builds");
    assert_eq!(selected_calls.load(Ordering::SeqCst), CHAIN_COUNT);
    assert_eq!(unselected_calls.load(Ordering::SeqCst), 0);
}
