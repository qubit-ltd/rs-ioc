// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validation and source attribution for builder registrations.

use std::collections::HashSet;
use std::panic::Location;

use crate::dependency::Dependency;
use crate::error::RegistrationError;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;

/// Reports the caller's file location and marks unavailable package/module
/// data.
///
/// The type name becomes the diagnostic item name; package and module are
/// placeholders because explicit registrations have no generated source.
///
/// # Type Parameters
///
/// `T` is the component type this registration publishes. It must be `'static`
/// so its full type name is available for diagnostics and so the same type is
/// identified again by every later resolution.
///
/// # Returns
///
/// A source whose item name is the full type name of `T` and whose file, line,
/// and column point at the call site that requested the registration.
#[track_caller]
pub(in crate::builder) fn source<T: ?Sized + 'static>() -> DefinitionSource {
    let location = Location::caller();
    DefinitionSource::new(
        "<explicit>",
        "<explicit>",
        location.file(),
        location.line(),
        location.column(),
        std::any::type_name::<T>(),
    )
}

/// Records source location for builder-level profile input.
///
/// The caller location points to the public profile-selection call.
///
/// # Returns
///
/// A source whose item name is the fixed `active_profiles` label and whose
/// file, line, and column point at the profile-selection call site.
#[track_caller]
pub(in crate::builder) fn profile_source() -> DefinitionSource {
    let location = Location::caller();
    DefinitionSource::new(
        "<explicit>",
        "<explicit>",
        location.file(),
        location.line(),
        location.column(),
        "active_profiles",
    )
}

/// Validates options and returns a typed key plus the optional profile.
///
/// An explicit `options.id` must parse as a [`BindingId`], and an explicit
/// `options.profile` must satisfy the grammar checked by [`valid_profile`].
/// `definition` is carried into every rejection so the caller sees the same
/// location it reported when registering.
///
/// # Type Parameters
///
/// `T` is the component type the produced key identifies. It must be `'static`
/// so [`BindingKey::of`] records a type id that later resolutions reproduce.
///
/// # Parameters
///
/// `options` carries the explicit id and profile requested at the registration
/// site, and `definition` is the reported source location copied into every
/// rejection so diagnostics point back at that site.
///
/// # Returns
///
/// The registration key for `T`, and the requested profile: `Some` when the
/// registration is profile-qualified and `None` when it always applies.
///
/// # Errors
///
/// Returns [`RegistrationError::InvalidBindingId`] when `options.id` is
/// present but malformed, and [`RegistrationError::InvalidProfile`] when
/// `options.profile` is present but violates the profile grammar. The first
/// failing option is reported and no key is produced.
// Keep the public RegistrationError's source and key fields intact.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_options<T: ?Sized + 'static>(
    options: &BindingOptions,
    definition: DefinitionSource,
) -> Result<(BindingKey, Option<String>), RegistrationError> {
    let id = options
        .id
        .as_deref()
        .map(BindingId::parse)
        .transpose()
        .map_err(|error| RegistrationError::InvalidBindingId { error, definition })?;
    if let Some(profile) = &options.profile
        && !valid_profile(profile)
    {
        return Err(RegistrationError::InvalidProfile {
            value: profile.clone(),
            definition,
        });
    }
    Ok((BindingKey::of::<T>(id), options.profile.clone()))
}

/// Checks one profile identifier against the documented ASCII grammar.
///
/// The first byte must be an ASCII letter; every later byte must be an ASCII
/// letter, an ASCII digit, an underscore, or a hyphen. The check reads bytes
/// only, allocates nothing, and accepts multi-byte UTF-8 input solely by
/// rejecting its non-ASCII bytes.
///
/// # Parameters
///
/// `profile` is the candidate identifier examined byte by byte. It is borrowed
/// for the duration of the check and never copied or normalised.
///
/// # Returns
///
/// `true` when the whole identifier matches the grammar, `false` as soon as any
/// byte violates it. An empty identifier is rejected because it has no leading
/// letter.
#[inline]
#[must_use]
pub(in crate::builder) fn valid_profile(profile: &str) -> bool {
    let mut bytes = profile.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Rejects malformed IDs and exact duplicate requests before staging a factory.
///
/// Every dependency that names an explicit id is parsed, and the whole request
/// list is checked for exact duplicates, so a factory is only staged once its
/// requests are known to be usable.
///
/// # Parameters
///
/// `dependencies` is the complete request list scanned in order for malformed
/// explicit ids and exact duplicates, and `definition` is the reported source
/// location copied into every rejection.
///
/// # Errors
///
/// Returns [`RegistrationError::InvalidBindingId`] for the first dependency
/// whose explicit id is malformed, and
/// [`RegistrationError::DuplicateDependency`] for the first request that
/// repeats an earlier one; both carry the clone of the offending request plus
/// `definition`.
// Keep the public RegistrationError's source and request fields intact.
#[allow(clippy::result_large_err)]
pub(crate) fn validate_dependencies(
    dependencies: &[Dependency],
    definition: DefinitionSource,
) -> Result<(), RegistrationError> {
    let mut seen = HashSet::with_capacity(dependencies.len());
    for dependency in dependencies {
        if let Some(id) = dependency.id() {
            BindingId::parse(id).map_err(|error| RegistrationError::InvalidBindingId { error, definition })?;
        }
        if !seen.insert(dependency) {
            return Err(RegistrationError::DuplicateDependency {
                dependency: dependency.clone(),
                definition,
            });
        }
    }
    Ok(())
}
