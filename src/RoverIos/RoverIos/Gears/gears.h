#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

jstring Java_com_rovernative_roverandroid_Gears_greeting(JNIEnv env, JClass _class, JString input);

void start(NSObject *view);

char *gretting(const char *name_ptr);

void greeting_free(char *s);
