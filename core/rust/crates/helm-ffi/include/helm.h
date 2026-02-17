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

bool helm_trigger_refresh(void);

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
 * Cancel a running task by ID. Returns true on success.
 */
bool helm_cancel_task(int64_t task_id);

/**
 * List manager status: detection info + preferences + implementation status as JSON.
 */
char *helm_list_manager_status(void);

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
 *
 * # Safety
 *
 * `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
 * strings.
 */
int64_t helm_upgrade_package(const char *manager_id, const char *package_name);

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
 * Install a manager tool via Homebrew. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs: "mise", "mas".
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_install_manager(const char *manager_id);

/**
 * Update a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs:
 * - "homebrew_formula" -> `brew update`
 * - "mise" -> `brew upgrade mise`
 * - "mas" -> `brew upgrade mas`
 * - "rustup" -> `rustup self update`
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_update_manager(const char *manager_id);

/**
 * Uninstall a manager tool. Returns the task ID, or -1 on error.
 *
 * Supported manager IDs: "mise", "mas" (via Homebrew), "rustup" (self uninstall).
 *
 * # Safety
 *
 * `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
 */
int64_t helm_uninstall_manager(const char *manager_id);

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
