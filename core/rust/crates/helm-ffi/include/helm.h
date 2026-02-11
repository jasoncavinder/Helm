#ifndef HELM_H
#define HELM_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

bool helm_init(const char *db_path);

char *helm_list_installed_packages(void);

char *helm_list_tasks(void);

bool helm_trigger_refresh(void);

void helm_free_string(char *s);

#endif  /* HELM_H */
