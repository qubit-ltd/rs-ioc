// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component, field, and dependency validation.

use syn::Attribute;
use syn::Error;
use syn::Fields;
use syn::GenericArgument;
use syn::ItemStruct;
use syn::LitStr;
use syn::Meta;
use syn::PathArguments;
use syn::Result;
use syn::Type;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use super::options::validate_id;
use crate::conditions::activation_attributes;
use crate::ir::ComponentIr;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::FieldIr;
use crate::ir::MacroKind;
use crate::ir::SourceIr;
use crate::parse::RawOption;
use crate::parse::RawValue;
use crate::parse::parse_options;

/// Normalizes a named-field or unit struct into ordered dependency requests.
///
/// The struct is consumed and rewritten in place: every `inject` and `value`
/// helper attribute is removed from the fields it was parsed from, so the
/// [`ComponentIr::item`] handed to the expansion stage no longer carries them.
/// The caller's own copy of the attributes must therefore be treated as moved,
/// not shared.
///
/// A field whose dependency request cannot be parsed is only rejected
/// immediately when the field has no activation conditions. When it does, the
/// failure is recorded in [`FieldIr::validation_error`] and re-reported only if
/// the condition can actually hold at runtime, so an inactive branch never
/// breaks the build.
///
/// # Parameters
///
/// * `kind` – macro that produced the item, used for diagnostics only.
/// * `item` – the validated struct, taken by value and returned rewritten.
/// * `options` – already validated macro options, bound into the result.
///
/// # Returns
///
/// The intermediate representation for the expansion stage, or the first error
/// raised while validating the struct shape, the field conditions, or a
/// dependency request on a field without activation conditions.
pub(super) fn component(kind: MacroKind, mut item: ItemStruct, options: ValidatedOptions) -> Result<ComponentIr> {
    validate_struct(&item, kind)?;
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    let mut fields = Vec::new();
    if let Fields::Named(named) = &mut item.fields {
        fields.reserve(named.named.len());
        for field in &mut named.named {
            let ident = field.ident.clone().expect("named fields always have identifiers");
            let conditions = activation_attributes(&field.attrs)?;
            let (dependency, validation_error) = match dependency(&mut field.attrs, &field.ty) {
                Ok(dependency) => (dependency, None),
                Err(error) if conditions.is_empty() => return Err(error),
                Err(error) => (
                    DependencyIr {
                        kind: DependencyKind::Required,
                        requested_type: field.ty.clone(),
                        id: None,
                    },
                    Some(error),
                ),
            };
            fields.push(FieldIr {
                ident,
                dependency,
                validation_error,
                conditions,
            });
        }
    }
    Ok(ComponentIr {
        item,
        options: options.binding(),
        fields,
        source,
    })
}

/// Validates that a struct has a supported shape and no generic parameters.
///
/// Two shapes are rejected: any generic parameter or `where` clause, and tuple
/// structs. Named-field structs and unit structs are accepted. The error span
/// points at the offending syntax so the diagnostic lands on the user's own
/// declaration rather than on the attribute.
///
/// # Parameters
///
/// * `item` – borrowed struct to check; it is not modified.
/// * `kind` – macro that produced the item, used to name the attribute in the
///   diagnostic.
///
/// # Errors
///
/// Returns an error when `item` declares generic parameters or a `where`
/// clause, or when its fields are unnamed.
pub(super) fn validate_struct(item: &ItemStruct, kind: MacroKind) -> Result<()> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(
            item.generics.span(),
            format!("#[{}] does not support generic structs", kind.name()),
        ));
    }
    if matches!(item.fields, Fields::Unnamed(_)) {
        return Err(Error::new(
            item.fields.span(),
            format!("#[{}] requires named fields or a unit struct", kind.name()),
        ));
    }
    Ok(())
}

/// Converts field and parameter helper attributes into a typed dependency.
///
/// This rewrites `attributes` in place: the `inject` and `value` helper
/// attributes are consumed and the remaining attributes are written back, so
/// the expansion stage never sees them again. `inject` and `value` are mutually
/// exclusive and each may appear at most once. `inject` accepts either a bare
/// marker or a single `id` option whose value must be a string literal, and the
/// identifier itself is checked by [`validate_id`]. A named `id` cannot be
/// combined with `Vec<Arc<T>>`, because that shape already requests every
/// instance and has no single binding to name.
///
/// # Parameters
///
/// * `attributes` – mutable attribute list; on success it contains only the
///   attributes that are not `inject` or `value`.
/// * `ty` – the declared field or parameter type, cloned into the result.
///
/// # Returns
///
/// The typed dependency request, or the first error found in the helper
/// attributes or in the declared type.
pub(super) fn dependency(attributes: &mut Vec<Attribute>, ty: &Type) -> Result<DependencyIr> {
    let mut id = None;
    let mut value_path = None;
    let mut seen_inject = false;
    let mut seen_value = false;
    let mut helper_span = ty.span();
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("inject") {
            if seen_inject || seen_value {
                return Err(Error::new(
                    attribute.span(),
                    "duplicate or conflicting `inject` attribute",
                ));
            }
            seen_inject = true;
            helper_span = attribute.span();
            let options = match &attribute.meta {
                Meta::Path(_) => Vec::new(),
                Meta::List(_) => parse_options(attribute.parse_args()?)?,
                Meta::NameValue(_) => {
                    return Err(Error::new(
                        attribute.span(),
                        "`inject` accepts a bare marker or `#[inject(id = \"...\")]`",
                    ));
                }
            };
            for RawOption { key, value } in options {
                if key != "id" {
                    return Err(Error::new(
                        key.span(),
                        "`inject` only accepts `id`, not `name` or other options",
                    ));
                }
                if id.is_some() {
                    return Err(Error::new(key.span(), "duplicate `inject` option `id`"));
                }
                let RawValue::String(value) = value else {
                    return Err(Error::new(key.span(), "`inject(id = ...)` requires a string literal"));
                };
                validate_id(&value)?;
                id = Some(value);
            }
        } else if attribute.path().is_ident("value") {
            if seen_inject || seen_value {
                return Err(Error::new(
                    attribute.span(),
                    "`value` cannot be combined with `inject` or another `value`",
                ));
            }
            seen_value = true;
            helper_span = attribute.span();
            value_path = Some(attribute.parse_args::<LitStr>()?);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    if let Some(path) = value_path {
        return Ok(DependencyIr {
            kind: DependencyKind::Value { path },
            requested_type: ty.clone(),
            id: None,
        });
    }
    let (kind, requested_type) = classify_dependency(ty)?;
    if matches!(kind, DependencyKind::All) && id.is_some() {
        return Err(Error::new(
            helper_span,
            "`#[inject(id = ...)]` is unsupported on `Vec<Arc<T>>`",
        ));
    }
    Ok(DependencyIr {
        kind,
        requested_type,
        id,
    })
}

/// Recognizes exactly `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`.
///
/// The recognized path spellings are normalized by [`single_generic`], so
/// `std::sync::Arc<T>` and `alloc::sync::Arc<T>` are accepted alongside the
/// bare `Arc<T>`. The returned type is the inner `T`, cloned so the result
/// outlives the borrow of `ty`.
///
/// # Parameters
///
/// * `ty` – the declared type to classify.
///
/// # Returns
///
/// The dependency kind together with the inner `T`, or an error naming the
/// three supported shapes when `ty` matches none of them.
fn classify_dependency(ty: &Type) -> Result<(DependencyKind, Type)> {
    if let Some(inner) = single_generic(ty, "Arc") {
        return Ok((DependencyKind::Required, inner.clone()));
    }
    if let Some(inner) = single_generic(ty, "Option")
        && let Some(target) = single_generic(inner, "Arc")
    {
        return Ok((DependencyKind::Optional, target.clone()));
    }
    if let Some(inner) = single_generic(ty, "Vec")
        && let Some(target) = single_generic(inner, "Arc")
    {
        return Ok((DependencyKind::All, target.clone()));
    }
    Err(Error::new(
        ty.span(),
        "unsupported injection type; expected `Arc<T>`, `Option<Arc<T>>`, or `Vec<Arc<T>>`",
    ))
}

/// Returns the single type argument of a supported standard generic path.
///
/// `name` selects the accepted path list and must be one of `Arc`, `Option`, or
/// `Vec`; any other value matches nothing and yields `None`. `Arc` accepts the
/// bare, `std`, and `alloc` paths; `Option` accepts the bare, `std`, and `core`
/// paths; `Vec` accepts the bare, `std`, and `alloc` paths. The returned
/// reference borrows from `ty` and performs no allocation beyond the temporary
/// segment-name vector used for the comparison.
///
/// # Parameters
///
/// * `ty` – the type to inspect; it is only read.
/// * `name` – generic wrapper to match, one of `Arc`, `Option`, or `Vec`.
///
/// # Returns
///
/// `Some` with the sole generic type argument when `ty` is an unqualified path
/// whose segments match one of the accepted spellings for `name`, whose last
/// segment uses angle-bracketed arguments, and whose argument list is exactly
/// one type argument. `None` when `name` is not a supported wrapper, when `ty`
/// is not a plain path type, when it is a qualified path such as
/// `<T as Trait>::Output`, when the path is not an accepted spelling, when the
/// last segment is not generic, when there is not exactly one argument, or when
/// that argument is not a type.
#[must_use]
#[inline]
pub(super) fn single_generic<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }

    let segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    let supported_paths: &[&[&str]] = match name {
        "Arc" => &[&["Arc"], &["std", "sync", "Arc"], &["alloc", "sync", "Arc"]],
        "Option" => &[&["Option"], &["std", "option", "Option"], &["core", "option", "Option"]],
        "Vec" => &[&["Vec"], &["std", "vec", "Vec"], &["alloc", "vec", "Vec"]],
        _ => &[],
    };
    if !supported_paths
        .iter()
        .any(|candidate| segments.iter().map(String::as_str).eq(candidate.iter().copied()))
    {
        return None;
    }
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}
