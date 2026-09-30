// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! An alias binding paired with options awaiting final validation.

use crate::binding::PendingBinding;
use crate::error::RegistrationError;
use crate::key::BindingKey;
use crate::options::BindingOptions;
use crate::options::DefinitionSource;

/// Validates an alias key, profile, and source before registration.
///
/// This is a plain function pointer rather than a generic parameter or a trait
/// object because every alias targets a different interface type, and the
/// monomorphized `validate_options::<U>` instantiations cannot be stored in one
/// homogeneous list. The pointer is filled in with the instantiation for the
/// alias target when the draft is created, so the type it validates always
/// matches the projection stored alongside it.
///
/// The options are taken by value: validation consumes them, and the caller
/// cannot read them again afterwards.
///
/// The result is the concrete binding key for the alias together with the
/// profile the alias resolves to — `Some` when the alias declared a profile
/// explicitly, and `None` when it did not, in which case the caller inherits
/// the definition's final concrete profile. A `RegistrationError` means the
/// options were rejected, tagged with the definition's source.
pub(in crate::definition) type AliasValidator =
    fn(&BindingOptions, DefinitionSource) -> Result<(BindingKey, Option<String>), RegistrationError>;

/// Keeps alias options until the concrete profile and key are final.
///
/// One draft is created per `bind` call and held by the definition builder
/// until `build` runs. At that point the drafts are validated in declaration
/// order; the first failure aborts the whole build, so a definition is either
/// registered completely or not at all. This is a build-time intermediate only:
/// it holds no runtime state, no registration handle, and no deferred
/// initialization, and it is dropped once the definition is built.
pub(in crate::definition) struct AliasDraft {
    /// ID, profile, and selection metadata for the alias.
    ///
    /// Moved into the validator when the draft is checked, so it cannot be
    /// read again after validation.
    pub(in crate::definition) options: BindingOptions,
    /// Validates options in the interface type namespace.
    ///
    /// Always the instantiation for this draft's target interface type, so the
    /// validated key and the deferred projection below cannot disagree.
    pub(in crate::definition) validate: AliasValidator,
    /// Deferred projection with its typed alias key.
    ///
    /// Carries both the alias-side key and the bound-side key. The projection
    /// function it holds does not run here: it executes later, while the
    /// dependency graph is constructed.
    pub(in crate::definition) binding: PendingBinding,
}
