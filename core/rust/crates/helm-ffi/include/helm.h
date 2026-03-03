#ifndef HELM_H
#define HELM_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Initialize the Helm core engine with the given SQLite database path.
 *
 * # Safety
 *
 * `db_path` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_init(const char *db_path);

char *helm_list_installed_packages(void);

char *helm_list_outdated_packages(void);

char *helm_list_tasks(void);

/**
 * Return captured stdout/stderr for a task ID as JSON.
 *
 * Returns `null` only on serialization/allocation failure.
 */
char *helm_get_task_output(int64_t task_id);

/**
 * Return persisted lifecycle task logs for a task ID as JSON.
 *
 * Returns `null` only on invalid input or serialization/allocation failure.
 */
char *helm_list_task_logs(int64_t task_id, int64_t limit);

/**
 * List pending hard-timeout prompts for running tasks as JSON.
 */
char *helm_list_task_timeout_prompts(void);

/**
 * Respond to a pending task hard-timeout prompt by task ID.
 *
 * When `wait_for_completion` is true, the task deadline is extended.
 * When false, the task is terminated immediately.
 */
bool helm_respond_task_timeout_prompt(int64_t task_id, bool wait_for_completion);

bool helm_trigger_refresh(void);

bool helm_trigger_detection(void);

/**
 * Trigger detection/refresh for a single manager.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_trigger_detection_for_manager(const char *manager_id);

/**
 * Query the local search cache synchronously and return JSON results.
 *
 * # Safety
 *
 * `query` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
char *helm_search_local(const char *query);

/**
 * Submit a remote search request for the given query. Returns the task ID, or -1 on error.
 *
 * # Safety
 *
 * `query` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_trigger_remote_search(const char *query);

/**
 * Submit a remote search request for a specific manager. Returns the task ID, or -1 on error.
 *
 * # Safety
 *
 * `manager_id` and `query` must be valid, non-null pointers to NUL-terminated UTF-8 C strings.
 */
int64_t helm_trigger_remote_search_for_manager(const char *manager_id, const char *query);

/**
 * Cancel a running task by ID. Returns true on success.
 */
bool helm_cancel_task(int64_t task_id);

/**
 * Dismiss a terminal task by ID. Returns true on success.
 */
bool helm_dismiss_task(int64_t task_id);

/**
 * List manager status: detection info + preferences + implementation status as JSON.
 */
char *helm_list_manager_status(void);

/**
 * Run a local doctor scan and return a health report JSON payload.
 *
 * Current implementation scope:
 * - package-state diagnostics for metadata-only Homebrew manager installs.
 *
 * TODO(doctor-repair): wire additional detectors and remote fingerprint lookups.
 */
char *helm_doctor_scan(void);

/**
 * Return whether shared onboarding has been completed.
 */
bool helm_get_cli_onboarding_completed(void);

/**
 * Set shared onboarding completion state. Returns true on success.
 */
bool helm_set_cli_onboarding_completed(bool completed);

/**
 * Return accepted shared license terms version.
 *
 * Returns null when unset or unavailable.
 */
char *helm_get_cli_accepted_license_terms_version(void);

/**
 * Set accepted shared license terms version.
 *
 * Pass null to clear. Returns true on success.
 *
 * # Safety
 *
 * `version` may be null; when non-null, it must point to a valid NUL-terminated UTF-8 string.
 */
bool helm_set_cli_accepted_license_terms_version(const char *version);

/**
 * Return whether safe mode is enabled.
 */
bool helm_get_safe_mode(void);

/**
 * Set safe mode state. Returns true on success.
 */
bool helm_set_safe_mode(bool enabled);

/**
 * Return whether Homebrew upgrades should auto-clean old kegs by default.
 */
bool helm_get_homebrew_keg_auto_cleanup(void);

/**
 * Set the global Homebrew keg policy.
 */
bool helm_set_homebrew_keg_auto_cleanup(bool enabled);

/**
 * List per-package Homebrew keg policy overrides as JSON.
 */
char *helm_list_package_keg_policies(void);

/**
 * Set per-package Homebrew keg policy override.
 *
 * `policy_mode` values:
 * - `-1`: clear override (use global)
 * - `0`: keep old kegs
 * - `1`: cleanup old kegs
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
bool helm_set_package_keg_policy(const char *manager_id,
                                 const char *package_name,
                                 int32_t policy_mode);

/**
 * Build an ordered upgrade execution plan from cached outdated snapshot as JSON.
 *
 * - `include_pinned`: if false, pinned packages are excluded.
 * - `allow_os_updates`: explicit confirmation gate for `softwareupdate` steps.
 */
char *helm_preview_upgrade_plan(bool include_pinned, bool allow_os_updates);

/**
 * Queue upgrade tasks for supported managers using cached outdated snapshot.
 *
 * - `include_pinned`: if false, pinned packages are excluded.
 * - `allow_os_updates`: explicit confirmation gate for `softwareupdate` upgrades.
 */
bool helm_upgrade_all(bool include_pinned, bool allow_os_updates);

/**
 * Queue an upgrade task for a single package. Returns the task ID, or -1 on error.
 *
 * Currently supported manager IDs:
 * - "homebrew_formula"
 * - "mise"
 * - "npm"
 * - "pnpm"
 * - "yarn"
 * - "cargo"
 * - "cargo_binstall"
 * - "pip"
 * - "pipx"
 * - "poetry"
 * - "rubygems"
 * - "bundler"
 * - "rustup"
 * - "softwareupdate" (requires package_name "__confirm_os_updates__")
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
int64_t helm_upgrade_package(const char *manager_id, const char *package_name);

/**
 * Queue an install task for a single package. Returns the task ID, or -1 on error.
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
int64_t helm_install_package(const char *manager_id, const char *package_name);

/**
 * Queue an uninstall task for a single package. Returns the task ID, or -1 on error.
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
int64_t helm_uninstall_package(const char *manager_id, const char *package_name);

/**
 * Preview package uninstall blast radius as JSON.
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
char *helm_preview_package_uninstall(const char *manager_id, const char *package_name);

/**
 * List pin records as JSON.
 */
char *helm_list_pins(void);

/**
 * Persist a virtual pin for a package. Returns true on success.
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings. `pinned_version` may be null.
 */
bool helm_pin_package(const char *manager_id, const char *package_name, const char *pinned_version);

/**
 * Remove a pin for a package. Returns true on success.
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
bool helm_unpin_package(const char *manager_id, const char *package_name);

/**
 * Set a manager as enabled or disabled.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_set_manager_enabled(const char *manager_id, bool enabled);

/**
 * Set (or clear) the selected executable path for a manager.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 * `selected_path` may be null (to clear override).
 */
bool helm_set_manager_selected_executable_path(const char *manager_id, const char *selected_path);

/**
 * Set the managed install instance for a manager by stable `instance_id`.
 *
 * This updates selected executable-path preference, marks the selected instance active,
 * clears multi-instance acknowledgement, and refreshes in-memory executable overrides.
 *
 * # Safety
 *
 * `manager_id` and `instance_id` must be valid, non-null pointers to NUL-terminated UTF-8
 * C strings.
 */
bool helm_set_manager_active_install_instance(const char *manager_id, const char *instance_id);

/**
 * Acknowledge current multi-instance install set for a manager.
 *
 * Stores the active install-set fingerprint so manager health can be considered acknowledged.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_ack_manager_multi_instance_state(const char *manager_id);

/**
 * Clear multi-instance acknowledgement state for a manager.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_clear_manager_multi_instance_ack(const char *manager_id);

/**
 * Set (or clear) the selected install method for a manager.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 * `install_method` may be null (to clear override).
 */
bool helm_set_manager_install_method(const char *manager_id, const char *install_method);

/**
 * Set manager timeout profile overrides in seconds.
 *
 * Positive values set an override; zero/negative values clear the override.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
bool helm_set_manager_timeout_profile(const char *manager_id,
                                      int64_t hard_timeout_seconds,
                                      int64_t idle_timeout_seconds);

/**
 * Apply a manager package-state repair option and queue the corresponding task.
 *
 * The current scaffold supports metadata-only Homebrew manager installs by routing one of:
 * - `reinstall_manager_via_homebrew`
 * - `remove_stale_package_entry`
 *
 * # Safety
 *
 * All pointers must be valid, non-null pointers to NUL-terminated UTF-8 C strings.
 */
int64_t helm_apply_manager_package_state_issue_repair(const char *manager_id,
                                                      const char *source_manager_id,
                                                      const char *package_name,
                                                      const char *issue_code,
                                                      const char *option_id);

/**
 * Install a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs:
 * - "mise" -> script installer (default), Homebrew, MacPorts, or cargo install
 * - "asdf" -> script installer (default) or Homebrew
 * - "mas" -> Homebrew
 * - "rustup" -> rustup-init (default) or Homebrew, based on selected install method
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_install_manager(const char *manager_id);

/**
 * Install a manager tool with optional JSON options. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs:
 * - "mise" -> script installer (default), Homebrew, MacPorts, or cargo install
 * - "asdf" -> script installer (default) or Homebrew
 * - "mas" -> Homebrew
 * - "rustup" -> rustup-init (default) or Homebrew, based on selected install method
 *
 * Supported options (method-specific):
 * - `installMethodOverride`: one-off method id (e.g. `homebrew`) without mutating saved manager preference
 * - `rustupInstallSource`: `officialDownload` (default) or `existingBinaryPath`
 * - `rustupBinaryPath`: absolute path used when `rustupInstallSource=existingBinaryPath`
 * - `miseInstallSource`: `officialDownload` (default) or `existingBinaryPath`
 * - `miseBinaryPath`: absolute path used when `miseInstallSource=existingBinaryPath`
 * - `completePostInstallSetupAutomatically`: automatically apply recommended setup defaults
 *   after install succeeds for managers that support post-install setup (`rustup`, `mise`,
 *   `asdf`)
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 * `options_json` may be null.
 */
int64_t helm_install_manager_with_options(const char *manager_id,
                                          const char *options_json);

/**
 * Update a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs:
 * - "homebrew_formula" -> `brew update`
 * - "rustup" -> provenance-driven (`brew upgrade rustup` or `rustup self update`)
 * - Homebrew one-to-one managers -> provenance-driven (`asdf`, `mise`, `mas`, `pnpm`,
 *   `yarn`, `pipx`, `poetry`, `cargo-binstall`, `podman`, `colima`)
 * - Homebrew parent-formula managers -> provenance-driven (`npm`, `pip`, `rubygems`,
 *   `bundler`, `cargo`) when active install-instance formula ownership can be resolved.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_update_manager(const char *manager_id);

/**
 * Uninstall a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs include rustup and Homebrew-routed manager adapters where
 * provenance strategy is supported.
 *
 * This is a strict compatibility wrapper over `helm_uninstall_manager_with_options` with
 * `allow_unknown_provenance=false`.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_uninstall_manager(const char *manager_id);

/**
 * Preview manager uninstall blast radius and strategy as JSON.
 *
 * `allow_unknown_provenance` controls whether unknown-provenance routing uses override mode.
 * For preview-only UI flows, callers typically pass `false` and rely on `unknown_override_required`
 * in the JSON response to gate destructive execution.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
char *helm_preview_manager_uninstall(const char *manager_id, bool allow_unknown_provenance);

/**
 * Preview manager uninstall blast radius and strategy as JSON with structured options.
 *
 * `options_json` supports:
 * - `allowUnknownProvenance` (bool)
 * - `homebrewCleanupMode` (`managerOnly` | `fullCleanup`)
 * - `miseCleanupMode` (`managerOnly` | `fullCleanup`)
 * - `miseConfigRemoval` (`keepConfig` | `removeConfig`)
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 * `options_json` must be null or a valid pointer to a NUL-terminated UTF-8 JSON string.
 */
char *helm_preview_manager_uninstall_with_options(const char *manager_id, const char *options_json);

/**
 * Uninstall a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs include rustup and Homebrew-routed manager adapters where
 * provenance strategy is supported.
 *
 * `allow_unknown_provenance` enables explicit override for ambiguous manager provenance where
 * uninstall routing supports override-based fallback.
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_uninstall_manager_with_options(const char *manager_id, bool allow_unknown_provenance);

/**
 * Uninstall a manager tool with structured options. Returns the task ID, or -1 on error.
 *
 * `options_json` supports:
 * - `allowUnknownProvenance` (bool)
 * - `homebrewCleanupMode` (`managerOnly` | `fullCleanup`)
 * - `miseCleanupMode` (`managerOnly` | `fullCleanup`)
 * - `miseConfigRemoval` (`keepConfig` | `removeConfig`)
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 * `options_json` must be null or a valid pointer to a NUL-terminated UTF-8 JSON string.
 */
int64_t helm_uninstall_manager_with_uninstall_options(const char *manager_id,
                                                      const char *options_json);

/**
 * Reset the database by rolling back all migrations and re-applying them.
 * Returns true on success.
 */
bool helm_reset_database(void);

/**
 * Return and clear the most recent service error localization key.
 */
char *helm_take_last_error_key(void);

/**
 * Free a string previously returned by a `helm_*` function.
 *
 * # Safety
 *
 * `s` must be a pointer previously returned by a `helm_*` function, or null.
 */
void helm_free_string(char *s);

#endif  /* HELM_H */
