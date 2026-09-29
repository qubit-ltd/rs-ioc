// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared profile activation for definition preparation and graph validation.

/// Tests whether one definition participates in the configured profiles.
///
/// # Parameters
///
/// * `profile` - Required definition profile, or `None` for unconditional use.
/// * `active_profiles` - Explicitly active profiles; an empty slice selects
///   `default`.
///
/// # Returns
///
/// Returns `true` for an unprofiled definition or when its profile is active.
#[must_use]
#[inline]
pub(crate) fn profile_is_active(profile: Option<&str>, active_profiles: &[String]) -> bool {
    profile.is_none_or(|profile| {
        if active_profiles.is_empty() {
            profile == "default"
        } else {
            active_profiles.iter().any(|active| active == profile)
        }
    })
}
