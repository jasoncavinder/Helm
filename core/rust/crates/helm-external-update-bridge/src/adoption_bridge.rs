//! Private local evidence and owned handles only; never an IPC evidence format.
use super::*;
use helm_core::external_update::{BoundaryObservation, durable::DurableUpdateError};

/// Mirrors HelmExternalNativeBoundary. All six facts must be freshly established
/// by the native coordinator, including inherited sandbox state (not merely an
/// absent entitlement). A hello, cached JSON or a client assertion is insufficient.
#[repr(C)]
pub struct NativeBoundary {
    pub abi_version: u32,
    pub helper_identifier: Bytes,
    pub helper_team_identifier: Bytes,
    pub helper_code_directory_hash: Bytes,
    pub caller_identifier: Bytes,
    pub caller_team_identifier: Bytes,
    /// Bits 0..5: live authenticated caller, valid Developer ID signatures,
    /// notarization, preserved Helm sandbox, unsandboxed helper, direct channel.
    pub observed_flags: u32,
}

pub const ADOPTED: u32 = 51;
pub const REVIEW_CHANGED: u32 = 52;
pub const OUTCOME_UNKNOWN: u32 = 53;

/// Validate untrusted intent without filesystem or ledger access.
/// # Safety
/// Slices are readable/immutable; user_root is native account data, not IPC input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_adoption_request(request: Bytes, user_root: Bytes) -> u32 {
    std::panic::catch_unwind(|| {
        assessment((|| {
            let roots = unsafe { application_roots(user_root)? };
            let data = unsafe { bytes(request, 8192)? };
            adoption::AdoptionRequest::decode_for_roots(&data, &roots).map(|_| ())
        })())
    })
    .unwrap_or(INTERNAL_FAILURE)
}

unsafe fn map_boundary(input: &NativeBoundary) -> Result<BoundaryObservation, Rejection> {
    if input.abi_version != 1 || input.observed_flags != 63 {
        return Err(Rejection::BoundaryUnavailable);
    }
    Ok(BoundaryObservation {
        helper_identifier: unsafe { text(input.helper_identifier, 255)? },
        helper_team_identifier: unsafe { text(input.helper_team_identifier, 10)? },
        helper_code_directory_hash: unsafe { bytes(input.helper_code_directory_hash, 32)? },
        caller_identifier: unsafe { text(input.caller_identifier, 255)? },
        caller_team_identifier: unsafe { text(input.caller_team_identifier, 10)? },
        authenticated_live_caller: input.observed_flags & 1 != 0,
        developer_id_signature_valid: input.observed_flags & 2 != 0,
        notarization_accepted: input.observed_flags & 4 != 0,
        helm_sandbox_preserved: input.observed_flags & 8 != 0,
        external_helper_unsandboxed: input.observed_flags & 16 != 0,
        direct_consumer_channel: input.observed_flags & 32 != 0,
    })
}

/// Owned by one native coordinator. Never serialized, persisted or sent over XPC.
pub struct AdoptionReview(adoption::ReviewedLedgerAdoption);

/// Read-only preparation; null rejects unavailable/invalid evidence or storage.
/// The result is not permission to update, and cannot initialize missing storage.
/// # Safety
/// Non-null inputs are aligned, readable native snapshots; nonempty slices are
/// immutable/readable for this call. Only request is untrusted serialized intent.
/// Path comes from the native private lease, which surrounds the operation. The
/// caller establishes complete supported ownership checks before supplying target
/// evidence. The result must be exclusively consumed once by confirm or free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_adoption_prepare(
    path: Bytes,
    request: Bytes,
    target: *const NativeTarget,
    boundary: *const NativeBoundary,
    now: u64,
) -> *mut AdoptionReview {
    std::panic::catch_unwind(|| {
        let prepared = (|| {
            let target = unsafe { target.as_ref() }.ok_or(Rejection::MalformedRequest)?;
            let boundary = unsafe { boundary.as_ref() }.ok_or(Rejection::BoundaryUnavailable)?;
            let path = unsafe { private_path(path)? };
            let (target, roots) = unsafe { map_target(target)? };
            let boundary = unsafe { map_boundary(boundary)? };
            let request = adoption::AdoptionRequest::decode(&unsafe { bytes(request, 8192)? })?;
            adoption::ReviewedLedgerAdoption::prepare(&path, request, target, boundary, &roots, now)
                .map_err(|_| Rejection::MalformedRequest)
        })();
        prepared
            .map(|review| Box::into_raw(Box::new(AdoptionReview(review))))
            .unwrap_or(std::ptr::null_mut())
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Consumes the review on every outcome, including malformed fresh evidence.
/// 51=consent recorded only, 52=changed/rejected review, 53=uncertain outcome.
/// No returned code is an update permit. Storage failure/lost reply must never
/// trigger automatic replay; obtain fresh history before a new human decision.
/// # Safety
/// Review is null or a live exclusively owned prepare result, never a wire
/// pointer or previously consumed handle. Other inputs follow prepare's contract.
/// Fresh native checks, live one-time admission and the private lease precede
/// this synchronous call. Native postcheck failure must report uncertainty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_adoption_confirm(
    review: *mut AdoptionReview,
    path: Bytes,
    target: *const NativeTarget,
    boundary: *const NativeBoundary,
    now: u64,
) -> u32 {
    if review.is_null() {
        return OUTCOME_UNKNOWN;
    }
    let review = unsafe { Box::from_raw(review) };
    std::panic::catch_unwind(|| {
        let inputs = (|| {
            let target = unsafe { target.as_ref() }.ok_or(Rejection::MalformedRequest)?;
            let boundary = unsafe { boundary.as_ref() }.ok_or(Rejection::BoundaryUnavailable)?;
            let path = unsafe { private_path(path)? };
            let (target, roots) = unsafe { map_target(target)? };
            Ok::<_, Rejection>((path, target, roots, unsafe { map_boundary(boundary)? }))
        })();
        let Ok((path, target, roots, boundary)) = inputs else {
            return REVIEW_CHANGED;
        };
        match review.0.confirm(&path, target, boundary, &roots, now) {
            Ok(_) => ADOPTED,
            Err(DurableUpdateError::Conflict | DurableUpdateError::Policy(_)) => REVIEW_CHANGED,
            Err(_) => OUTCOME_UNKNOWN,
        }
    })
    .unwrap_or(OUTCOME_UNKNOWN)
}

/// # Safety
/// Null or a live, exclusively owned prepare result, never already consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_external_adoption_free(review: *mut AdoptionReview) {
    if !review.is_null() {
        drop(unsafe { Box::from_raw(review) });
    }
}

#[cfg(test)]
mod tests;
