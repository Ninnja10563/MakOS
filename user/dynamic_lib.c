#include <stdint.h>

__attribute__((visibility("default"))) const char makos_shared_name[] =
    "libmakosdemo.so";

__attribute__((visibility("default"))) uint64_t makos_shared_add(uint64_t left,
                                                                 uint64_t right) {
    return left + right;
}
