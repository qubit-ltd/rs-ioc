// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::collections::HashMap;
use std::collections::HashSet;

use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::graph::BindingLocation;
use crate::graph::ResolvedDependency;
use crate::key::BindingKey;

/// Precomputes a stable root-to-binding path for every active binding.
pub(super) fn paths_to_all(
    definitions: &[PendingDefinition],
    resolved: &[Vec<ResolvedDependency>],
    selected: &HashSet<BindingLocation>,
) -> HashMap<BindingKey, Vec<BindingKey>> {
    let locations: HashMap<BindingKey, BindingLocation> = definitions
        .iter()
        .enumerate()
        .flat_map(|(definition, item)| {
            item.bindings.iter().enumerate().filter_map(move |(binding, value)| {
                let location = BindingLocation { definition, binding };
                selected.contains(&location).then_some((value.key.clone(), location))
            })
        })
        .collect();
    let mut targets = HashSet::new();
    for (definition_index, definition) in definitions.iter().enumerate() {
        for (binding_index, binding) in definition.bindings.iter().enumerate() {
            if !selected.contains(&BindingLocation {
                definition: definition_index,
                binding: binding_index,
            }) {
                continue;
            }
            if let PendingBindingKind::Alias { target, .. } = &binding.kind {
                targets.insert(target.clone());
            } else {
                for dependency in &resolved[definition_index] {
                    targets.extend(dependency.keys.iter().cloned());
                }
            }
        }
    }
    let mut paths = HashMap::new();
    for (definition_index, definition) in definitions.iter().enumerate() {
        for (binding_index, binding) in definition.bindings.iter().enumerate() {
            if !selected.contains(&BindingLocation {
                definition: definition_index,
                binding: binding_index,
            }) {
                continue;
            }
            if !targets.contains(&binding.key) {
                trace_paths(&binding.key, definitions, resolved, &locations, &mut paths);
            }
        }
    }
    for (definition_index, definition) in definitions.iter().enumerate() {
        for (binding_index, binding) in definition.bindings.iter().enumerate() {
            if !selected.contains(&BindingLocation {
                definition: definition_index,
                binding: binding_index,
            }) {
                continue;
            }
            if !paths.contains_key(&binding.key) {
                trace_paths(&binding.key, definitions, resolved, &locations, &mut paths);
            }
        }
    }
    paths
}

/// Visits a validated acyclic dependency path, preserving its first root.
fn trace_paths(
    key: &BindingKey,
    definitions: &[PendingDefinition],
    resolved: &[Vec<ResolvedDependency>],
    locations: &HashMap<BindingKey, BindingLocation>,
    paths: &mut HashMap<BindingKey, Vec<BindingKey>>,
) {
    if paths.contains_key(key) {
        return;
    }
    let mut stack = vec![(key.clone(), None::<Vec<BindingKey>>, 0usize)];
    while !stack.is_empty() {
        let frame_index = stack.len() - 1;
        let current = stack[frame_index].0.clone();
        if paths.contains_key(&current) {
            stack.pop();
            continue;
        }
        if stack[frame_index].1.is_none() {
            let mut current_path = stack
                .get(frame_index.checked_sub(1).unwrap_or(usize::MAX))
                .and_then(|(parent, _, _)| paths.get(parent))
                .cloned()
                .unwrap_or_default();
            current_path.push(current.clone());
            paths.insert(current.clone(), current_path);
            let location = locations.get(&current).expect("selected path key must have a binding");
            let definition_index = location.definition;
            let binding = &definitions[definition_index].bindings[location.binding];
            let targets = match &binding.kind {
                PendingBindingKind::Alias { target, .. } => vec![target.clone()],
                _ => resolved[definition_index]
                    .iter()
                    .flat_map(|dependency| dependency.keys.iter().cloned())
                    .collect(),
            };
            stack[frame_index].1 = Some(targets);
        }
        let targets = stack[frame_index].1.as_ref().expect("targets initialized");
        if stack[frame_index].2 < targets.len() {
            let target = targets[stack[frame_index].2].clone();
            stack[frame_index].2 += 1;
            if !paths.contains_key(&target) {
                stack.push((target, None, 0));
            }
        } else {
            stack.pop();
        }
    }
}
