// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Bean signature, parameter, and output validation.

use syn::Error;
use syn::FnArg;
use syn::GenericArgument;
use syn::ItemFn;
use syn::Pat;
use syn::PathArguments;
use syn::Result;
use syn::ReturnType;
use syn::Type;
use syn::TypeParamBound;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use super::component::dependency;
use super::component::single_generic;
use crate::conditions::activation_attributes;
use crate::internal::RuntimePath;
use crate::ir::BeanIr;
use crate::ir::OutputIr;
use crate::ir::OutputShape;
use crate::ir::ParamIr;
use crate::ir::SourceIr;

/// Normalizes a bean function into the intermediate representation consumed by
/// code generation.
///
/// `item` is taken by value and its signature inputs are consumed, because each
/// parameter attribute is rewritten in place by [`dependency`] after the
/// activation attributes have been read. `runtime` is used to recognize the
/// runtime's managed-type wrapper inside the declared output, and `options`
/// supplies the marker and the already validated binding options.
///
/// # Parameters
///
/// * `item` - the annotated function, taken by value so that its parameter
///   attributes can be rewritten in place while it is being normalized.
/// * `options` - the already validated `#[bean]` options; its marker is cloned
///   onto the intermediate representation and the remaining fields are
///   converted into the binding options.
/// * `runtime` - the runtime path used to recognize the managed-type wrapper in
///   the declared output; it is borrowed for the whole call.
///
/// # Returns
///
/// On success the normalized intermediate representation carries the consumed
/// function, the converted binding options, the cloned marker, one parameter
/// entry per declared parameter in declaration order, the decomposed output,
/// and the source span used for diagnostics. On failure the spanned
/// [`syn::Error`] described under `# Errors` is returned and no partial result
/// is produced.
///
/// # Errors
///
/// Returns a spanned [`syn::Error`] when the annotated function is not a
/// supported bean signature:
///
/// * `const`, `unsafe`, an explicit ABI, variadic parameters, generic
///   parameters, or a `where` clause — reported as `#[bean] requires a safe,
///   non-generic Rust function`.
/// * a receiver or otherwise non-typed argument — reported as `#[bean] cannot
///   be used on methods or receivers`.
/// * a parameter pattern that is not a plain identifier — reported as `#[bean]
///   parameters must be named identifiers`.
/// * an output form rejected by [`output`], or an attribute rejected by
///   [`activation_attributes`] and [`dependency`], whose diagnostics are
///   propagated unchanged.
pub(super) fn bean(mut item: ItemFn, options: ValidatedOptions, runtime: &RuntimePath) -> Result<BeanIr> {
    let signature = &item.sig;
    if signature.constness.is_some()
        || signature.unsafety.is_some()
        || signature.abi.is_some()
        || signature.variadic.is_some()
        || !signature.generics.params.is_empty()
        || signature.generics.where_clause.is_some()
    {
        return Err(Error::new(
            signature.span(),
            "#[bean] requires a safe, non-generic Rust function",
        ));
    }
    let source = SourceIr {
        item: signature.ident.clone(),
        span: signature.ident.span(),
    };
    let output = output(&signature.output, options.explicit_type.as_ref(), runtime)?;
    let mut params = Vec::with_capacity(signature.inputs.len());
    for argument in &mut item.sig.inputs {
        let FnArg::Typed(argument) = argument else {
            return Err(Error::new(
                argument.span(),
                "#[bean] cannot be used on methods or receivers",
            ));
        };
        let Pat::Ident(pattern) = argument.pat.as_ref() else {
            return Err(Error::new(
                argument.pat.span(),
                "#[bean] parameters must be named identifiers",
            ));
        };
        let ident = pattern.ident.clone();
        let conditions = activation_attributes(&argument.attrs)?;
        let dependency = dependency(&mut argument.attrs, &argument.ty)?;
        params.push(ParamIr {
            ident,
            conditions,
            dependency,
        });
    }
    let marker = options.marker.clone();
    Ok(BeanIr {
        item,
        options: options.binding(),
        marker,
        params,
        output,
        source,
    })
}

/// Decomposes the supported bean output forms and an optional explicit type.
///
/// The return type is first peeled into an optional `Result` success type, then
/// a runtime managed type, then a single `Arc` argument, in that order. The
/// remaining `result` and `arc` flags select one of four [`OutputShape`]
/// variants: bare, `Arc`, `Result` of a bare value, or `Result` of an `Arc`.
/// When `explicit_type` is supplied it replaces the peeled component type and
/// therefore takes precedence over the declared return type.
///
/// # Parameters
///
/// * `return_type` - the declared return position of the bean function; it is
///   borrowed only for the duration of the decomposition.
/// * `explicit_type` - the optional `type = ...` override; when present it
///   replaces the peeled component type before the final validation and can
///   therefore turn an otherwise acceptable signature into an error.
/// * `runtime` - the runtime path used to recognize the managed-type wrapper
///   that suppresses the `Arc` peeling step.
///
/// # Returns
///
/// On success an output intermediate representation pairing the selected
/// [`OutputShape`] with the `managed` flag and the final owned component type.
/// On failure the spanned [`syn::Error`] described under `# Errors` is
/// returned.
///
/// # Errors
///
/// Returns a spanned [`syn::Error`] when:
///
/// * the signature has no return type — reported as `#[bean] requires a return
///   type`.
/// * a future is syntactically visible in the returned value — reported as
///   `#[bean] does not support a nested future output; use async fn instead`.
/// * the component type, before or after applying `explicit_type`, is `impl
///   Trait` or a reference — reported as `#[bean] does not support impl Trait
///   or borrowed outputs` and `#[bean] requires an owned concrete output type`
///   respectively.
fn output(return_type: &ReturnType, explicit_type: Option<&Type>, runtime: &RuntimePath) -> Result<OutputIr> {
    let ReturnType::Type(_, ty) = return_type else {
        return Err(Error::new(return_type.span(), "#[bean] requires a return type"));
    };
    let (inner, result) = if let Some((success, _error)) = result_types(ty) {
        (success, true)
    } else {
        (ty.as_ref(), false)
    };
    if contains_future_type(inner) {
        return Err(Error::new(
            inner.span(),
            "#[bean] does not support a nested future output; use `async fn` instead",
        ));
    }
    if matches!(inner, Type::ImplTrait(_) | Type::Reference(_)) {
        return Err(Error::new(
            inner.span(),
            "#[bean] does not support `impl Trait` or borrowed outputs",
        ));
    }
    let (inner, managed) = if let Some(inner) = runtime.managed_argument(inner) {
        (inner, true)
    } else {
        (inner, false)
    };
    let (component_type, arc) = if !managed && let Some(inner) = single_generic(inner, "Arc") {
        (inner.clone(), true)
    } else {
        (inner.clone(), false)
    };
    let component_type = explicit_type.cloned().unwrap_or(component_type);
    if matches!(component_type, Type::ImplTrait(_) | Type::Reference(_)) {
        return Err(Error::new(
            component_type.span(),
            "#[bean] requires an owned concrete output type",
        ));
    }
    let shape = match (result, arc) {
        (false, false) => OutputShape::Bare,
        (false, true) => OutputShape::Arc,
        (true, false) => OutputShape::ResultBare,
        (true, true) => OutputShape::ResultArc,
    };
    Ok(OutputIr {
        shape,
        managed,
        component_type,
    })
}

/// Extracts exactly two type arguments from `Result` or its standard qualified
/// paths.
///
/// Returns `None` for any other shape, including a qualified path such as
/// `some::module::Result`, an explicit `::Result` with a leading colon, a
/// qualified self type, and a `Result` whose arguments are not exactly two
/// types.
///
/// # Returns
///
/// `Some((success, error))` when `ty` is spelled exactly `Result<A, E>` or
/// `std::result::Result<A, E>` / `core::result::Result<A, E>` with both
/// arguments being type arguments, borrowing the success and error types from
/// `ty`. `None` for every other shape, namely a qualified
/// `some::module::Result`, an explicit `::Result` with a leading colon, a
/// qualified self type, an argument list such as `Result<A, E, F>`, and a
/// `Result` whose two arguments are not both type arguments.
///
/// # Parameters
///
/// * `ty` - the candidate type, borrowed for the duration of the inspection.
fn result_types(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    if path.qself.is_some() {
        return None;
    }
    let segments = &path.path.segments;
    let standard = segments.len() == 3
        && matches!(segments[0].ident.to_string().as_str(), "std" | "core")
        && segments[1].ident == "result"
        && segments[2].ident == "Result";
    let plain = path.path.leading_colon.is_none() && segments.len() == 1 && segments[0].ident == "Result";
    if !plain && !standard {
        return None;
    }
    let segment = segments.last()?;
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 2 {
        return None;
    }
    let mut args = args.args.iter();
    let GenericArgument::Type(success) = args.next()? else {
        return None;
    };
    let GenericArgument::Type(error) = args.next()? else {
        return None;
    };
    Some((success, error))
}

/// Detects syntactically visible futures inside a factory's returned value.
///
/// The scan is purely syntactic: a future reached through a type alias, a
/// qualified path that is not spelled `Future`, or an opaque associated type is
/// not detected, so this is a conservative guard rather than a type-level
/// proof. A future spelled as an `async fn` return position is instead handled
/// by the bean signature itself.
#[must_use]
fn contains_future_type(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path.path.segments.iter().any(|segment| {
            segment.ident == "Future"
                || match &segment.arguments {
                    PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(inner) => contains_future_type(inner),
                        GenericArgument::AssocType(assoc) => contains_future_type(&assoc.ty),
                        _ => false,
                    }),
                    _ => false,
                }
        }),
        Type::TraitObject(object) => object.bounds.iter().any(|bound| match bound {
            TypeParamBound::Trait(trait_bound) => trait_bound
                .path
                .segments
                .iter()
                .any(|segment| segment.ident == "Future"),
            _ => false,
        }),
        Type::Group(group) => contains_future_type(&group.elem),
        Type::Paren(paren) => contains_future_type(&paren.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(contains_future_type),
        Type::Slice(slice) => contains_future_type(&slice.elem),
        Type::Array(array) => contains_future_type(&array.elem),
        _ => false,
    }
}
