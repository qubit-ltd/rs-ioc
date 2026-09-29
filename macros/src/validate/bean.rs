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

/// Normalizes a bean function's parameters, output, and marker option.
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
