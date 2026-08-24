#ifndef MAKOS_H
#define MAKOS_H

#include <stddef.h>
#include <stdint.h>

#define MAKOS_ABI_VERSION UINT64_C(0x00010000)
#define MAKOS_FEATURE_IPC (UINT64_C(1) << 0)
#define MAKOS_FEATURE_PROCESS (UINT64_C(1) << 1)
#define MAKOS_FEATURE_VM (UINT64_C(1) << 2)
#define MAKOS_FEATURE_VFS (UINT64_C(1) << 3)
#define MAKOS_FEATURE_NETWORK (UINT64_C(1) << 4)
#define MAKOS_FEATURE_GRAPHICS (UINT64_C(1) << 5)
#define MAKOS_FEATURE_AUTH (UINT64_C(1) << 6)
#define MAKOS_FEATURE_LOG (UINT64_C(1) << 7)
#define MAKOS_FEATURE_SYNC (UINT64_C(1) << 8)
#define MAKOS_FEATURE_LINUX_PERSONALITY (UINT64_C(1) << 9)
#define MAKOS_FEATURE_AUDIO (UINT64_C(1) << 10)
#define MAKOS_FEATURE_IPV6 (UINT64_C(1) << 11)
#define MAKOS_FEATURE_WINDOWS_PERSONALITY (UINT64_C(1) << 12)
#define MAKOS_FEATURE_SERVICE_SUPERVISION (UINT64_C(1) << 13)
#define MAKOS_FEATURE_SELF_HOSTING_SEED (UINT64_C(1) << 14)
#define MAKOS_FEATURE_SOCKET_OBJECTS (UINT64_C(1) << 15)
#define MAKOS_FEATURE_PACKAGE_TRANSACTIONS (UINT64_C(1) << 16)
#define MAKOS_FEATURE_VM_REGIONS (UINT64_C(1) << 17)
#define MAKOS_FEATURE_EXEC_BY_PATH (UINT64_C(1) << 18)
#define MAKOS_FEATURE_PROCESS_STARTUP (UINT64_C(1) << 19)
#define MAKOS_FEATURE_TTY_SIGNALS (UINT64_C(1) << 20)
#define MAKOS_FEATURE_TYPED_IPC (UINT64_C(1) << 21)
#define MAKOS_TYPED_MESSAGE_VERSION UINT8_C(1)
#define MAKOS_TYPED_MESSAGE_PAYLOAD_BYTES 52
#define MAKOS_IPC_RIGHT_SEND UINT8_C(1)
#define MAKOS_IPC_RIGHT_RECEIVE UINT8_C(2)
#define MAKOS_IPC_RIGHT_TRANSFER UINT8_C(4)
#define MAKOS_PACKAGE_SIGNATURE_BYTES 256
#define MAKOS_PACKAGE_DEPENDENCY_EXACT 0
#define MAKOS_PACKAGE_DEPENDENCY_AT_LEAST 1
#define MAKOS_SPAWN_ARGUMENTS_VERSION UINT32_C(1)
#define MAKOS_SPAWN_MAX_ARGUMENTS 8
#define MAKOS_SPAWN_MAX_ENVIRONMENT 8
#define MAKOS_SPAWN_DATA_BYTES 256
#define MAKOS_PROT_READ UINT64_C(1)
#define MAKOS_PROT_WRITE UINT64_C(2)
#define MAKOS_PROT_EXEC UINT64_C(4)
#define MAKOS_AF_INET UINT64_C(2)
#define MAKOS_SOCK_STREAM UINT64_C(1)
#define MAKOS_SOCK_DGRAM UINT64_C(2)
#define MAKOS_IPPROTO_TCP UINT64_C(6)
#define MAKOS_IPPROTO_UDP UINT64_C(17)

struct makos_sockaddr_in {
    uint16_t family;
    uint16_t port_be;
    uint8_t address[4];
};

struct makos_stat {
    uint32_t mode;
    uint32_t uid;
    uint32_t gid;
    uint32_t kind;
    uint64_t size;
    uint64_t modified_ticks;
    uint64_t inode;
};

struct makos_dirent {
    uint64_t inode;
    uint32_t kind;
    uint32_t name_length;
    char name[32];
};

struct makos_spawn_arguments {
    uint32_t version;
    uint32_t argc;
    uint32_t envc;
    uint32_t data_length;
    uint32_t argv_offsets[MAKOS_SPAWN_MAX_ARGUMENTS];
    uint32_t env_offsets[MAKOS_SPAWN_MAX_ENVIRONMENT];
    char data[MAKOS_SPAWN_DATA_BYTES];
};

struct makos_typed_message {
    uint8_t version;
    uint8_t length;
    uint16_t type;
    uint32_t sender_pid;
    uint32_t sender_uid;
    uint8_t payload[MAKOS_TYPED_MESSAGE_PAYLOAD_BYTES];
};

_Static_assert(sizeof(struct makos_typed_message) == 64,
               "MakOS typed IPC message ABI size changed");

_Static_assert(sizeof(struct makos_spawn_arguments) == 336,
               "MakOS spawn argument ABI size changed");

long makos_write(const void *bytes, size_t length);
long makos_open(const char *path, size_t length, int write_access);
long makos_read(long fd, void *bytes, size_t length);
long makos_close(long fd);
long makos_channel_create(uint64_t pair[2]);
long makos_service_publish(const char *name, size_t length);
long makos_service_connect(const char *name, size_t length);
long makos_service_accept(long listener);
long makos_typed_channel_send(long endpoint,
                              const struct makos_typed_message *message,
                              long transfer_handle, uint8_t transfer_rights);
long makos_typed_channel_receive(long endpoint,
                                 struct makos_typed_message *message,
                                 uint64_t *transfer_handle);
void *makos_mmap(void);
long makos_munmap(void *address);
long makos_mprotect(void *address, uint64_t protection);
void *makos_mmap_range(size_t length, uint64_t protection);
long makos_munmap_range(void *address, size_t length);
long makos_mprotect_range(void *address, size_t length, uint64_t protection);
long makos_process_spawn_path(const char *path, size_t length);
int makos_spawn_arguments_init(struct makos_spawn_arguments *arguments,
                               const char *const argv[], size_t argc,
                               const char *const envp[], size_t envc);
long makos_process_spawn_path_args(
    const char *path, size_t length,
    const struct makos_spawn_arguments *arguments);
long makos_thread_create(void (*entry)(void *), void *argument);
long makos_thread_join(long tid);
_Noreturn void makos_thread_exit(int status);
uint64_t makos_abi_info(uint64_t selector);
long makos_event_create(int initially_signaled);
long makos_event_signal(long event);
long makos_event_wait(long event);
long makos_handle_close(long handle);
long makos_stat(const char *path, size_t length, struct makos_stat *metadata);
long makos_readdir(const char *path, size_t length, size_t index,
                   struct makos_dirent *entry);
long makos_audio_write(const int16_t *samples, size_t frames, uint32_t rate,
                       uint32_t channels);
long makos_ipv6_echo(void);
long makos_socket_create(uint64_t domain, uint64_t type, uint64_t protocol);
long makos_socket_connect(long socket, const struct makos_sockaddr_in *address);
long makos_socket_send(long socket, const void *bytes, size_t length,
                       uint64_t flags);
long makos_socket_receive(long socket, void *bytes, size_t length,
                          uint64_t flags);
long makos_socket_close(long socket);
long makos_package_install(const char *name, size_t name_length,
                           const char *version, size_t version_length,
                           const void *content, size_t content_length,
                           const void *dependencies, size_t dependency_length,
                           const uint8_t signature[MAKOS_PACKAGE_SIGNATURE_BYTES]);
long makos_package_query(const char *name, size_t name_length, char *version,
                         size_t capacity);
long makos_package_rollback(void);
long makos_package_remove(const char *name, size_t length);
long makos_create(const char *path, size_t length);
long makos_unlink(const char *path, size_t length);
long makos_surface_create(uint32_t width, uint32_t height);
long makos_surface_fill(long surface, uint16_t x, uint16_t y, uint16_t width,
                        uint16_t height, uint32_t argb);
long makos_surface_present(long surface);
_Noreturn void makos_exit(int status);

static inline uint16_t makos_htons(uint16_t value) {
    return (uint16_t)((value << 8) | (value >> 8));
}

#endif
