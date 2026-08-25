#include <makos.h>

enum {
    SYS_WRITE = 0,
    SYS_CHANNEL_CREATE = 2,
    SYS_EXIT = 5,
    SYS_SURFACE_CREATE = 8,
    SYS_SURFACE_FILL = 9,
    SYS_SURFACE_PRESENT = 10,
    SYS_OPEN = 11,
    SYS_READ = 12,
    SYS_CLOSE = 13,
    SYS_PACKAGE_INSTALL = 18,
    SYS_PACKAGE_QUERY = 19,
    SYS_PACKAGE_ROLLBACK = 20,
    SYS_VM_MAP = 21,
    SYS_VM_UNMAP = 22,
    SYS_THREAD_CREATE = 23,
    SYS_THREAD_JOIN = 24,
    SYS_THREAD_EXIT = 25,
    SYS_ABI_INFO = 31,
    SYS_EVENT_CREATE = 32,
    SYS_EVENT_SIGNAL = 33,
    SYS_EVENT_WAIT = 34,
    SYS_HANDLE_CLOSE = 35,
    SYS_STAT = 36,
    SYS_READ_DIR = 37,
    SYS_AUDIO_WRITE = 39,
    SYS_SOCKET_IPV6_ECHO = 40,
    SYS_CREATE = 43,
    SYS_UNLINK = 44,
    SYS_VM_PROTECT = 45,
    SYS_SOCKET_CREATE = 47,
    SYS_SOCKET_CONNECT = 48,
    SYS_SOCKET_SEND = 49,
    SYS_SOCKET_RECEIVE = 50,
    SYS_SOCKET_CLOSE = 51,
    SYS_PACKAGE_REMOVE = 52,
    SYS_VM_MAP_RANGE = 53,
    SYS_VM_UNMAP_RANGE = 54,
    SYS_VM_PROTECT_RANGE = 55,
    SYS_PROCESS_SPAWN_PATH = 56,
    SYS_PROCESS_SPAWN_PATH_ARGS = 57,
    SYS_TYPED_SERVICE_PUBLISH = 143,
    SYS_TYPED_SERVICE_CONNECT = 144,
    SYS_TYPED_SERVICE_ACCEPT = 145,
    SYS_TYPED_CHANNEL_SEND = 146,
    SYS_TYPED_CHANNEL_RECEIVE = 147,
    SYS_THREAD_AFFINITY = 148,
};

static long syscall4(uint64_t number, uint64_t first, uint64_t second,
                     uint64_t third, uint64_t fourth) {
    register uint64_t arg4 __asm__("r10") = fourth;
    __asm__ volatile("int $0x80"
                     : "+a"(number)
                     : "D"(first), "S"(second), "d"(third), "r"(arg4)
                     : "memory", "cc");
    return (long)number;
}

long makos_write(const void *bytes, size_t length) {
    return syscall4(SYS_WRITE, (uintptr_t)bytes, length, 0, 0);
}

long makos_open(const char *path, size_t length, int write_access) {
    return syscall4(SYS_OPEN, (uintptr_t)path, length, write_access != 0, 0);
}

long makos_read(long fd, void *bytes, size_t length) {
    return syscall4(SYS_READ, (uint64_t)fd, (uintptr_t)bytes, length, 0);
}

long makos_close(long fd) { return syscall4(SYS_CLOSE, (uint64_t)fd, 0, 0, 0); }

long makos_channel_create(uint64_t pair[2]) {
    return syscall4(SYS_CHANNEL_CREATE, (uintptr_t)pair, 0, 0, 0);
}

long makos_service_publish(const char *name, size_t length) {
    return syscall4(SYS_TYPED_SERVICE_PUBLISH, (uintptr_t)name, length, 0, 0);
}

long makos_service_connect(const char *name, size_t length) {
    return syscall4(SYS_TYPED_SERVICE_CONNECT, (uintptr_t)name, length, 0, 0);
}

long makos_service_accept(long listener) {
    return syscall4(SYS_TYPED_SERVICE_ACCEPT, (uint64_t)listener, 0, 0, 0);
}

long makos_typed_channel_send(long endpoint,
                              const struct makos_typed_message *message,
                              long transfer_handle, uint8_t transfer_rights) {
    return syscall4(SYS_TYPED_CHANNEL_SEND, (uint64_t)endpoint,
                    (uintptr_t)message, (uint64_t)transfer_handle,
                    transfer_rights);
}

long makos_typed_channel_receive(long endpoint,
                                 struct makos_typed_message *message,
                                 uint64_t *transfer_handle) {
    return syscall4(SYS_TYPED_CHANNEL_RECEIVE, (uint64_t)endpoint,
                    (uintptr_t)message, (uintptr_t)transfer_handle, 0);
}

void *makos_mmap(void) {
    return (void *)(uintptr_t)syscall4(SYS_VM_MAP, 0, 0, 0, 0);
}

long makos_munmap(void *address) {
    return syscall4(SYS_VM_UNMAP, (uintptr_t)address, 0, 0, 0);
}

long makos_mprotect(void *address, uint64_t protection) {
    return syscall4(SYS_VM_PROTECT, (uintptr_t)address, protection, 0, 0);
}

void *makos_mmap_range(size_t length, uint64_t protection) {
    return (void *)(uintptr_t)syscall4(SYS_VM_MAP_RANGE, length, protection, 0,
                                      0);
}

long makos_munmap_range(void *address, size_t length) {
    return syscall4(SYS_VM_UNMAP_RANGE, (uintptr_t)address, length, 0, 0);
}

long makos_mprotect_range(void *address, size_t length,
                          uint64_t protection) {
    return syscall4(SYS_VM_PROTECT_RANGE, (uintptr_t)address, length,
                    protection, 0);
}

long makos_process_spawn_path(const char *path, size_t length) {
    return syscall4(SYS_PROCESS_SPAWN_PATH, (uintptr_t)path, length, 0, 0);
}

static int append_spawn_string(struct makos_spawn_arguments *arguments,
                               const char *value, uint32_t *offset) {
    if (value == NULL || value[0] == '\0')
        return 0;
    *offset = arguments->data_length;
    do {
        if (arguments->data_length == MAKOS_SPAWN_DATA_BYTES)
            return 0;
        arguments->data[arguments->data_length++] = *value;
    } while (*value++ != '\0');
    return 1;
}

int makos_spawn_arguments_init(struct makos_spawn_arguments *arguments,
                               const char *const argv[], size_t argc,
                               const char *const envp[], size_t envc) {
    if (arguments == NULL || argv == NULL || argc == 0 ||
        argc > MAKOS_SPAWN_MAX_ARGUMENTS ||
        envc > MAKOS_SPAWN_MAX_ENVIRONMENT || (envc != 0 && envp == NULL))
        return 0;
    unsigned char *bytes = (unsigned char *)arguments;
    for (size_t index = 0; index < sizeof(*arguments); ++index)
        bytes[index] = 0;
    arguments->version = MAKOS_SPAWN_ARGUMENTS_VERSION;
    arguments->argc = (uint32_t)argc;
    arguments->envc = (uint32_t)envc;
    for (size_t index = 0; index < argc; ++index) {
        if (!append_spawn_string(arguments, argv[index],
                                 &arguments->argv_offsets[index]))
            return 0;
    }
    for (size_t index = 0; index < envc; ++index) {
        int has_equals = 0;
        for (const char *cursor = envp[index]; cursor != NULL && *cursor != '\0';
             ++cursor) {
            if (*cursor == '=' && cursor != envp[index])
                has_equals = 1;
        }
        if (!has_equals ||
            !append_spawn_string(arguments, envp[index],
                                 &arguments->env_offsets[index]))
            return 0;
    }
    return 1;
}

long makos_process_spawn_path_args(
    const char *path, size_t length,
    const struct makos_spawn_arguments *arguments) {
    return syscall4(SYS_PROCESS_SPAWN_PATH_ARGS, (uintptr_t)path, length,
                    (uintptr_t)arguments, sizeof(*arguments));
}

long makos_thread_create(void (*entry)(void *), void *argument) {
    return syscall4(SYS_THREAD_CREATE, (uintptr_t)entry, (uintptr_t)argument, 0,
                    0);
}

long makos_thread_join(long tid) {
    return syscall4(SYS_THREAD_JOIN, (uint64_t)tid, 0, 0, 0);
}

_Noreturn void makos_thread_exit(int status) {
    syscall4(SYS_THREAD_EXIT, (uint64_t)status, 0, 0, 0);
    __builtin_trap();
}

long makos_thread_get_affinity(long tid) {
    return syscall4(SYS_THREAD_AFFINITY, 0, (uint64_t)tid, 0, 0);
}

long makos_thread_set_affinity(long tid, uint64_t cpu_mask) {
    return syscall4(SYS_THREAD_AFFINITY, 1, (uint64_t)tid, cpu_mask, 0);
}

uint64_t makos_abi_info(uint64_t selector) {
    return (uint64_t)syscall4(SYS_ABI_INFO, selector, 0, 0, 0);
}

long makos_event_create(int initially_signaled) {
    return syscall4(SYS_EVENT_CREATE, initially_signaled != 0, 0, 0, 0);
}

long makos_event_signal(long event) {
    return syscall4(SYS_EVENT_SIGNAL, (uint64_t)event, 0, 0, 0);
}

long makos_event_wait(long event) {
    return syscall4(SYS_EVENT_WAIT, (uint64_t)event, 0, 0, 0);
}

long makos_handle_close(long handle) {
    return syscall4(SYS_HANDLE_CLOSE, (uint64_t)handle, 0, 0, 0);
}

long makos_stat(const char *path, size_t length, struct makos_stat *metadata) {
    return syscall4(SYS_STAT, (uintptr_t)path, length, (uintptr_t)metadata, 0);
}

long makos_readdir(const char *path, size_t length, size_t index,
                   struct makos_dirent *entry) {
    return syscall4(SYS_READ_DIR, (uintptr_t)path, length, index,
                    (uintptr_t)entry);
}

long makos_audio_write(const int16_t *samples, size_t frames, uint32_t rate,
                       uint32_t channels) {
    return syscall4(SYS_AUDIO_WRITE, (uintptr_t)samples, frames, rate, channels);
}

long makos_ipv6_echo(void) {
    return syscall4(SYS_SOCKET_IPV6_ECHO, 0, 0, 0, 0);
}

long makos_socket_create(uint64_t domain, uint64_t type, uint64_t protocol) {
    return syscall4(SYS_SOCKET_CREATE, domain, type, protocol, 0);
}

long makos_socket_connect(long socket, const struct makos_sockaddr_in *address) {
    return syscall4(SYS_SOCKET_CONNECT, (uint64_t)socket, (uintptr_t)address,
                    sizeof(*address), 0);
}

long makos_socket_send(long socket, const void *bytes, size_t length,
                       uint64_t flags) {
    return syscall4(SYS_SOCKET_SEND, (uint64_t)socket, (uintptr_t)bytes, length,
                    flags);
}

long makos_socket_receive(long socket, void *bytes, size_t length,
                          uint64_t flags) {
    return syscall4(SYS_SOCKET_RECEIVE, (uint64_t)socket, (uintptr_t)bytes,
                    length, flags);
}

long makos_socket_close(long socket) {
    return syscall4(SYS_SOCKET_CLOSE, (uint64_t)socket, 0, 0, 0);
}

long makos_package_install(const char *name, size_t name_length,
                           const char *version, size_t version_length,
                           const void *content, size_t content_length,
                           const void *dependencies, size_t dependency_length,
                           const uint8_t signature[MAKOS_PACKAGE_SIGNATURE_BYTES]) {
    uint8_t fields[16 + 255 + 255 + MAKOS_PACKAGE_SIGNATURE_BYTES];
    const uint8_t *content_bytes = content;
    const uint8_t *dependency_bytes = dependencies;
    size_t cursor = 0;
    if (!name || !version || !content || !dependencies || !signature ||
        name_length == 0 || name_length > 32 || version_length == 0 ||
        version_length > 16 || content_length == 0 || content_length > 255 ||
        dependency_length == 0 || dependency_length > 255) {
        return -1;
    }
    for (size_t i = 0; i < version_length; ++i) fields[cursor++] = (uint8_t)version[i];
    for (size_t i = 0; i < content_length; ++i) fields[cursor++] = content_bytes[i];
    for (size_t i = 0; i < dependency_length; ++i) fields[cursor++] = dependency_bytes[i];
    for (size_t i = 0; i < MAKOS_PACKAGE_SIGNATURE_BYTES; ++i) fields[cursor++] = signature[i];
    uint64_t packed = version_length | (content_length << 8) |
                      (dependency_length << 16) | (UINT64_C(1) << 24);
    return syscall4(SYS_PACKAGE_INSTALL, (uintptr_t)name, name_length,
                    (uintptr_t)fields, packed);
}

long makos_package_query(const char *name, size_t name_length, char *version,
                         size_t capacity) {
    return syscall4(SYS_PACKAGE_QUERY, (uintptr_t)name, name_length,
                    (uintptr_t)version, capacity);
}

long makos_package_rollback(void) {
    return syscall4(SYS_PACKAGE_ROLLBACK, 0, 0, 0, 0);
}

long makos_package_remove(const char *name, size_t length) {
    return syscall4(SYS_PACKAGE_REMOVE, (uintptr_t)name, length, 0, 0);
}

long makos_create(const char *path, size_t length) {
    return syscall4(SYS_CREATE, (uintptr_t)path, length, 0, 0);
}

long makos_unlink(const char *path, size_t length) {
    return syscall4(SYS_UNLINK, (uintptr_t)path, length, 0, 0);
}

long makos_surface_create(uint32_t width, uint32_t height) {
    return syscall4(SYS_SURFACE_CREATE, width, height, 0, 0);
}

long makos_surface_fill(long surface, uint16_t x, uint16_t y, uint16_t width,
                        uint16_t height, uint32_t argb) {
    uint64_t rectangle = (uint64_t)x | ((uint64_t)y << 16) |
                         ((uint64_t)width << 32) | ((uint64_t)height << 48);
    return syscall4(SYS_SURFACE_FILL, (uint64_t)surface, argb, rectangle, 0);
}

long makos_surface_present(long surface) {
    return syscall4(SYS_SURFACE_PRESENT, (uint64_t)surface, 0, 0, 0);
}

_Noreturn void makos_exit(int status) {
    syscall4(SYS_EXIT, (uint64_t)status, 0, 0, 0);
    __builtin_trap();
}
