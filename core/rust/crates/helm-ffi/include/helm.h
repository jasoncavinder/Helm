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

char *helm_list_tasks(void);

bool helm_trigger_refresh(void);

/**
 * Free a string previously returned by a `helm_*` function.
 *
 * # Safety
 *
 * `s` must be a pointer previously returned by a `helm_*` function, or null.
 */
void helm_free_string(char *s);

#endif  /* HELM_H */
