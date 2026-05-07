#ifndef ROVER_MACOS_H
#define ROVER_MACOS_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

typedef void *RoverNativeView;
typedef void *RoverMacosRuntime;

typedef enum RoverNativeViewKind {
  RoverNativeViewKindWindow = 0,
  RoverNativeViewKindView = 1,
  RoverNativeViewKindColumn = 2,
  RoverNativeViewKindRow = 3,
  RoverNativeViewKindText = 4,
  RoverNativeViewKindButton = 5,
  RoverNativeViewKindInput = 6,
  RoverNativeViewKindCheckbox = 7,
  RoverNativeViewKindImage = 8,
  RoverNativeViewKindScrollView = 9,
} RoverNativeViewKind;

typedef RoverNativeView (*RoverCreateViewFn)(uint32_t node_id, RoverNativeViewKind kind);
typedef void (*RoverAppendChildFn)(RoverNativeView parent, RoverNativeView child);
typedef void (*RoverRemoveViewFn)(RoverNativeView view);
typedef void (*RoverSetFrameFn)(RoverNativeView view, float x, float y, float width, float height);
typedef void (*RoverSetTextFn)(RoverNativeView view, const char *ptr, uintptr_t len);
typedef void (*RoverSetBoolFn)(RoverNativeView view, bool value);
typedef void (*RoverSetWindowFn)(RoverNativeView view, const char *title, uintptr_t len, float width, float height);
typedef void (*RoverStopAppFn)(void);

typedef struct RoverHostCallbacks {
  RoverCreateViewFn create_view;
  RoverAppendChildFn append_child;
  RoverRemoveViewFn remove_view;
  RoverSetFrameFn set_frame;
  RoverSetTextFn set_text;
  RoverSetBoolFn set_bool;
  RoverSetWindowFn set_window;
  RoverStopAppFn stop_app;
} RoverHostCallbacks;

RoverMacosRuntime rover_macos_init(RoverHostCallbacks callbacks);
RoverMacosRuntime rover_macos_init_with_callbacks(
  RoverCreateViewFn create_view,
  RoverAppendChildFn append_child,
  RoverRemoveViewFn remove_view,
  RoverSetFrameFn set_frame,
  RoverSetTextFn set_text,
  RoverSetBoolFn set_bool,
  RoverSetWindowFn set_window,
  RoverStopAppFn stop_app
);
void rover_macos_free(RoverMacosRuntime runtime);
int32_t rover_macos_load_lua(RoverMacosRuntime runtime, const char *source);
int32_t rover_macos_tick(RoverMacosRuntime runtime);
int32_t rover_macos_next_wake_ms(RoverMacosRuntime runtime);
int32_t rover_macos_dispatch_click(RoverMacosRuntime runtime, uint32_t id);
int32_t rover_macos_dispatch_input(RoverMacosRuntime runtime, uint32_t id, const char *value);
int32_t rover_macos_dispatch_submit(RoverMacosRuntime runtime, uint32_t id, const char *value);
int32_t rover_macos_dispatch_toggle(RoverMacosRuntime runtime, uint32_t id, bool checked);
int32_t rover_macos_set_viewport(RoverMacosRuntime runtime, uint16_t width, uint16_t height);
const char *rover_macos_last_error(RoverMacosRuntime runtime);

#endif
