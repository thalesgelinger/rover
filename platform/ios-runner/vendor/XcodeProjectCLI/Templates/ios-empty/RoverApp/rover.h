#pragma once
#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint8_t *data;
    size_t len;
    char *hits_json;
} RoverImage;

void *rover_create(const char *root_path);
void rover_destroy(void *handle);
char *rover_render_json(void *handle);
char *rover_dispatch_action_json(void *handle, const char *action);
RoverImage rover_render_rgba(void *handle, int width, int height);
void rover_image_free(RoverImage img);
void rover_string_free(char *ptr);

#ifdef __cplusplus
}
#endif
