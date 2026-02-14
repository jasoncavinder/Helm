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
 * Queue upgrade tasks for supported managers using cached outdated snapshot.
 *
 * - `include_pinned`: if false, pinned packages are excluded.
 * - `allow_os_updates`: explicit confirmation gate for `softwareupdate` upgrades.
 */
bool helm_upgrade_all(bool include_pinned, bool allow_os_updates);

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
 * Free a string previously returned by a `helm_*` function.
 *
 * # Safety
 *
 * `s` must be a pointer previously returned by a `helm_*` function, or null.
 */
void helm_free_string(char *s);

#endif  /* HELM_H */
