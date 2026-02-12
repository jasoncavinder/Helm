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
 * Free a string previously returned by a `helm_*` function.
 *
 * # Safety
 *
 * `s` must be a pointer previously returned by a `helm_*` function, or null.
 */
void helm_free_string(char *s);

#endif  /* HELM_H */
