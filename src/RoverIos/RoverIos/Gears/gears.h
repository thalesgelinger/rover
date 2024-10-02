#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef void (*Callback)(const char*);

void start(NSObject *view, const char *path);

void devServer(Callback callback);
