//! Private, in-process ABI. This is not an XPC/JSON protocol or an authorization
//! boundary: only the helper's successful native observer may supply evidence.
//! The separate ledger initializer accepts only a native-leased private path.
//! No process, network, adoption mutation or installer entrypoint.

use std::{path::PathBuf, slice, str};

use helm_core::external_update::preflight::PreflightRequest;
use helm_core::external_update::{Authority, Rejection, TargetObservation, adoption};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub data: *const u8,
    pub length: usize,
}

/// Keep synchronized with CExternalUpdatePolicy/include/helm_external_policy.h.
/// Strings are bounded UTF-8 slices, not NUL-terminated or deserialized facts.
#[repr(C)]
pub struct NativeTarget {
    pub abi_version: u32,
    pub canonical_path: Bytes,
    pub device: u64,
    pub inode: u64,
    pub bundle_identifier: Bytes,
    pub build: Bytes,
    pub team_identifier: Bytes,
    pub code_directory_hash: Bytes,
    pub ed25519_public_key: Bytes,
    pub feed_url: Bytes,
    pub framework_major: u32,
    pub has_store_receipt: u8,
    pub writable_by_others: u8,
    /// Bits 0..4: App Store receipt, Homebrew cask, Setapp location,
    /// Setapp bundle marker, native installer receipt. Any bit is denial only.
    pub manager_exclusions: u32,
    /// OS-account-derived root, never an XPC argument or HOME override. Empty
    /// means no user root; /Applications is always supplied by this bridge.
    pub user_applications_root: Bytes,
}

// Stable ABI result codes, not eligibility or permission levels. Zero is invalid
// so a zero-filled result cannot be mistaken for a successful preflight.
const INVALID: u32 = 0;
const UNRESOLVED: u32 = 1;
const OTHER_MANAGER: u32 = 2;
const OUTSIDE_ROOTS: u32 = 3;
const HELM_SELF_UPDATE: u32 = 4;
const UNSUPPORTED_TARGET: u32 = 5;
const INTERNAL_FAILURE: u32 = 6;
const TARGET_CHANGED: u32 = 7;

fn assessment(result: Result<(), Rejection>) -> u32 {
    match result {
        Ok(()) => UNRESOLVED,
        Err(Rejection::MalformedRequest) => INVALID,
        Err(Rejection::UnsupportedAuthority) => OTHER_MANAGER,
        Err(Rejection::TargetOutsideRoots) => OUTSIDE_ROOTS,
        Err(Rejection::HelmSelfUpdate) => HELM_SELF_UPDATE,
        Err(Rejection::TargetChanged) => TARGET_CHANGED,
        Err(_) => UNSUPPORTED_TARGET,
    }
}

unsafe fn application_roots(user_root: Bytes) -> Result<Vec<PathBuf>, Rejection> {
    let user_root = unsafe { text(user_root, 4096)? };
    let mut roots = vec![PathBuf::from("/Applications")];
    if !user_root.is_empty() {
        roots.push(PathBuf::from(user_root));
    }
    Ok(roots)
}

unsafe fn bytes(value: Bytes, maximum: usize) -> Result<Vec<u8>, Rejection> {
    if value.length > maximum || (value.length != 0 && value.data.is_null()) {
        return Err(Rejection::MalformedRequest);
    }
    if value.length == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: caller owns a readable, immutable slice for this synchronous call;
    // bounds/null are checked above. Arbitrary pointers are not a wire format.
    Ok(unsafe { slice::from_raw_parts(value.data, value.length) }.to_vec())
}

unsafe fn text(value: Bytes, maximum: usize) -> Result<String, Rejection> {
    let value = unsafe { bytes(value, maximum)? };
    let text = str::from_utf8(&value).map_err(|_| Rejection::MalformedRequest)?;
    if text.chars().any(char::is_control) {
        return Err(Rejection::MalformedRequest);
    }
    Ok(text.to_owned())
}

unsafe fn map_target(input: &NativeTarget) -> Result<(TargetObservation, Vec<PathBuf>), Rejection> {
    if input.abi_version != 1
        || input.has_store_receipt > 1
        || input.writable_by_others > 1
        || input.manager_exclusions & !31 != 0
        || (input.manager_exclusions & 1 != 0) != (input.has_store_receipt == 1)
    {
        return Err(Rejection::MalformedRequest);
    }
    let roots = unsafe { application_roots(input.user_applications_root)? };
    let target = TargetObservation {
        canonical_path: unsafe { text(input.canonical_path, 4096)? }.into(),
        device: input.device,
        inode: input.inode,
        bundle_identifier: unsafe { text(input.bundle_identifier, 255)? },
        build: unsafe { text(input.build, 128)? },
        team_identifier: unsafe { text(input.team_identifier, 10)? },
        code_directory_hash: unsafe { bytes(input.code_directory_hash, 32)? },
        ed25519_public_key: unsafe { bytes(input.ed25519_public_key, 32)? },
        feed_url: unsafe { text(input.feed_url, 4096)? },
        framework_major: input.framework_major,
        // A native observation is returned only after signature/path validation.
        // No client-settable signature, translocation or authority booleans exist.
        signature_valid: true,
        translocated: false,
        authority: if input.manager_exclusions == 0 {
            Authority::Unknown
        } else {
            Authority::OtherManager
        },
        has_store_receipt: input.has_store_receipt == 1,
        writable_by_others: input.writable_by_others == 1,
    };
    Ok((target, roots))
}

/// Assess local native facts using the same core gate as adoption review/resolve.
/// UNRESOLVED requires further authority work; it NEVER means "can update".
///
/// # Safety
/// `input` may be null (rejected); otherwise it must be aligned/readable, with
/// each nonempty byte slice readable and immutable for the entire call.
/// Only successful local native observations are
/// permitted. Never construct this input from requests, cached JSON or IPC facts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_target_preflight(input: *const NativeTarget) -> u32 {
    // SAFETY: a non-null pointer satisfies the ABI lifetime/alignment contract.
    // Convert before the closure so only a checked reference crosses that boundary.
    let Some(input) = (unsafe { input.as_ref() }) else {
        return INVALID;
    };
    std::panic::catch_unwind(|| {
        // SAFETY: the ABI caller guarantees the byte-slice lifetimes above.
        let result = unsafe { map_target(input) }
            .and_then(|(target, roots)| adoption::validate_unresolved_target(&target, &roots));
        assessment(result)
    })
    .unwrap_or(INTERNAL_FAILURE)
}

/// Validate the bounded, untrusted request before any native target read.
/// # Safety
/// Nonempty slices must be readable and immutable for the call. `user_root`
/// comes from the helper's OS account, never a wire parameter or environment.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_preflight_request(request: Bytes, user_root: Bytes) -> u32 {
    std::panic::catch_unwind(|| {
        assessment((|| {
            let roots = unsafe { application_roots(user_root)? };
            let bytes = unsafe { bytes(request, 8192)? };
            PreflightRequest::decode(&bytes, &roots).map(|_| ())
        })())
    })
    .unwrap_or(INTERNAL_FAILURE)
}

/// Bind successful native evidence to the exact requested app identity/build.
/// # Safety
/// `input` has the same contract as `helm_external_target_preflight`; `request`
/// is a readable, immutable slice. It contains untrusted intent, not evidence.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_requested_preflight(
    input: *const NativeTarget,
    request: Bytes,
) -> u32 {
    let Some(input) = (unsafe { input.as_ref() }) else {
        return INVALID;
    };
    std::panic::catch_unwind(|| {
        assessment((|| {
            let (target, roots) = unsafe { map_target(input)? };
            let bytes = unsafe { bytes(request, 8192)? };
            PreflightRequest::decode(&bytes, &roots)?.assess(&target, &roots)
        })())
    })
    .unwrap_or(INTERNAL_FAILURE)
}

/// Prepare the helper's private ledger, not Helm's normal application database.
/// This is NOT a wire operation or an adoption grant. The native caller must
/// hold and revalidate its filesystem lease around this synchronous call.
/// # Safety
/// `path` must be readable/immutable for the call and supplied by the native
/// OS-account path policy, never by IPC, preferences or environment overrides.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_ledger_prepare(path: Bytes, fresh: u8) -> u32 {
    std::panic::catch_unwind(|| {
        let Ok(path) = (unsafe { text(path, 4096) }) else {
            return 0;
        };
        let path = PathBuf::from(path);
        if fresh > 1
            || !path.is_absolute()
            || path.file_name().and_then(|name| name.to_str()) != Some("ledger.sqlite")
        {
            return 0;
        }
        u32::from(
            helm_core::sqlite::SqliteStore::prepare_external_update_ledger(&path, fresh == 1)
                .is_ok(),
        )
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests;
