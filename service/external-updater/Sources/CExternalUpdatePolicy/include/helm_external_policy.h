#ifndef HELM_EXTERNAL_POLICY_H
#define HELM_EXTERNAL_POLICY_H

#include <stddef.h>
#include <stdint.h>

/* Private in-process ABI. NEVER accept these facts from XPC/JSON clients.
 * All pointers are borrowed, immutable and readable for the synchronous call.
 * No pointer/string is retained, and no database or installation is touched. */
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
    uint32_t manager_exclusions; /* 1 receipt, 2 cask reference, 4 Setapp */
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

#endif
