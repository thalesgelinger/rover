#pragma once
#ifdef __cplusplus
extern "C" {
#endif

void *rover_create(const char *root_path);
void rover_destroy(void *handle);
char *rover_render_json(void *handle);
char *rover_dispatch_action_json(void *handle, const char *action);
void rover_string_free(char *ptr);

#ifdef __cplusplus
}
#endif
