#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

size_t strnlen(const char *text, size_t maximum) {
    size_t length = 0;
    while (length < maximum && text[length] != 0) {
        ++length;
    }
    return length;
}

char *strrchr(const char *text, int byte) {
    const char *result = NULL;
    do {
        if (*text == (char)byte) {
            result = text;
        }
    } while (*text++ != 0);
    return (char *)result;
}

void qsort(void *base, size_t count, size_t size,
           int (*compare)(const void *, const void *)) {
    uint8_t *bytes = base;
    for (size_t index = 1; index < count; ++index) {
        size_t cursor = index;
        while (cursor != 0 &&
               compare(bytes + cursor * size, bytes + (cursor - 1) * size) < 0) {
            for (size_t byte = 0; byte < size; ++byte) {
                uint8_t temporary = bytes[cursor * size + byte];
                bytes[cursor * size + byte] = bytes[(cursor - 1) * size + byte];
                bytes[(cursor - 1) * size + byte] = temporary;
            }
            --cursor;
        }
    }
}
