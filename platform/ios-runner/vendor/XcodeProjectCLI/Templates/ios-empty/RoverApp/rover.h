#pragma once
#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

typedef struct {
    uint8_t *data;
    size_t len;
    int32_t width;
    int32_t height;
    size_t row_bytes;
    char *hits_json;
} RoverImage;

void *rover_create(const char *root_path);
void rover_destroy(void *handle);
bool rover_enable_hot_reload(void *handle);
char *rover_render_json(void *handle);
char *rover_dispatch_action_json(void *handle, const char *action);
RoverImage rover_render_rgba(void *handle, int width, int height);
bool rover_render_metal(void *handle, void *device, void *queue, void *texture, int32_t width, int32_t height, float scale);
bool rover_pointer_tap(void *handle, float x, float y);
void rover_image_free(RoverImage img);
void rover_string_free(char *ptr);

#ifdef __cplusplus
}
#endif
