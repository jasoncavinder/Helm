#ifndef HELM_EXTERNAL_POLICY_H
#define HELM_EXTERNAL_POLICY_H

#include <stddef.h>
#include <stdint.h>

/* Private in-process ABI. NEVER accept these facts from XPC/JSON clients.
 * All pointers are borrowed, immutable and readable for the synchronous call.
 * No pointer/string is retained. Preflight never touches a database/install. */
typedef struct {
    const uint8_t *data;
    size_t length;
} HelmExternalBytes;

typedef struct {
    uint32_t abi_version;
    HelmExternalBytes canonical_path;
    uint64_t device;
    uint64_t inode;
    HelmExternalBytes bundle_identifier;
    HelmExternalBytes build;
    HelmExternalBytes team_identifier;
    HelmExternalBytes code_directory_hash;
    HelmExternalBytes ed25519_public_key;
    HelmExternalBytes feed_url;
    uint32_t framework_major;
    uint8_t has_store_receipt;
    uint8_t writable_by_others;
    /* Bits 0..4: MAS, cask, Setapp location, Setapp bundle marker, pkg receipt.
     * Denial signals only; no bits ever authorize adoption or installation. */
    uint32_t manager_exclusions;
    HelmExternalBytes user_applications_root; /* OS account, not HOME/client */
} HelmExternalNativeTarget;

enum {
    HELM_EXTERNAL_INVALID = 0,
    HELM_EXTERNAL_UNRESOLVED = 1, /* Not standalone, adopted or update-approved. */
    HELM_EXTERNAL_OTHER_MANAGER = 2,
    HELM_EXTERNAL_OUTSIDE_ROOTS = 3,
    HELM_EXTERNAL_SELF_UPDATE = 4,
    HELM_EXTERNAL_UNSUPPORTED_TARGET = 5,
    HELM_EXTERNAL_INTERNAL_FAILURE = 6,
    HELM_EXTERNAL_TARGET_CHANGED = 7
};

/* A NULL input is rejected with HELM_EXTERNAL_INVALID without reading a target. */
uint32_t helm_external_target_preflight(const HelmExternalNativeTarget *input);
/* Request bytes are untrusted JSON; the root and target are local native facts. */
uint32_t helm_external_preflight_request(HelmExternalBytes request, HelmExternalBytes user_root);
uint32_t helm_external_requested_preflight(const HelmExternalNativeTarget *input, HelmExternalBytes request);

/* Separate native-only initializer. The caller MUST hold its private path lease.
 * No DB override may come from XPC/HOME/preferences. Returns 1 on preparation,
 * 0 on rejection; neither grants adoption, update consent or install authority. */
uint32_t helm_external_ledger_prepare(HelmExternalBytes path, uint8_t fresh);

/* Read-only history codes 20..25; not update/adoption authorization. */
uint32_t helm_external_consent_status(const HelmExternalNativeTarget *input,
                                      HelmExternalBytes request, HelmExternalBytes path);

/* Native-owned, nonserializable, one-shot review. No adoption/install authority. */
typedef struct RevocationReview HelmExternalRevocationReview;
uint32_t helm_external_revocation_request(HelmExternalBytes request, HelmExternalBytes user_root);
HelmExternalRevocationReview *helm_external_revocation_prepare(HelmExternalBytes path, HelmExternalBytes request,
                                                              HelmExternalBytes user_root, uint64_t now);
/* Consumes review on every outcome: 40=revoked, 41=stale, 42=outcome unknown. */
uint32_t helm_external_revocation_confirm(HelmExternalRevocationReview *review, HelmExternalBytes path, uint64_t now);
void helm_external_revocation_free(HelmExternalRevocationReview *review);

/* Native-only freshly established boundary facts, NOT an IPC/JSON payload.
 * None of these facts may be inferred solely from hello or absent entitlements.
 * This ABI transports observations; it does not authenticate or collect them. */
typedef struct {
    uint32_t abi_version;
    HelmExternalBytes helper_identifier;
    HelmExternalBytes helper_team_identifier;
    HelmExternalBytes helper_code_directory_hash;
    HelmExternalBytes caller_identifier;
    HelmExternalBytes caller_team_identifier;
    /* Bits 0..5: live authenticated caller, valid Developer ID signatures,
     * notarization, preserved Helm sandbox, unsandboxed helper, direct channel. */
    uint32_t observed_flags;
} HelmExternalNativeBoundary;

/* A separate explicit adoption review. Never grants candidate/install consent.
 * Both calls require the native private filesystem lease; confirm additionally
 * requires fresh observations and one-time live-session admission. The pointer
 * and observations never cross XPC. Production adoption remains disabled. Ownership scan
 * completeness is a caller obligation. */
typedef struct AdoptionReview HelmExternalAdoptionReview;
uint32_t helm_external_adoption_request(HelmExternalBytes request, HelmExternalBytes user_root);
HelmExternalAdoptionReview *helm_external_adoption_prepare(HelmExternalBytes path, HelmExternalBytes request,
    const HelmExternalNativeTarget *target, const HelmExternalNativeBoundary *boundary, uint64_t now);
/* Consumes review even on invalid input. 51=recorded, 52=changed, 53=unknown.
 * Lost replies/storage failures are uncertainty, never automatic retry consent. */
uint32_t helm_external_adoption_confirm(HelmExternalAdoptionReview *review, HelmExternalBytes path,
    const HelmExternalNativeTarget *target, const HelmExternalNativeBoundary *boundary, uint64_t now);
void helm_external_adoption_free(HelmExternalAdoptionReview *review);
#endif
