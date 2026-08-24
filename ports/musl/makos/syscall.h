/* SPDX-License-Identifier: MIT */
#ifndef MUSL_MAKOS_SYSCALL_H
#define MUSL_MAKOS_SYSCALL_H

typedef __SIZE_TYPE__ makos_size_t;
typedef __UINT16_TYPE__ makos_u16;
typedef __UINT32_TYPE__ makos_u32;
typedef __UINT64_TYPE__ makos_u64;

struct makos_termios {
    makos_u32 input_flags;
    makos_u32 output_flags;
    makos_u32 control_flags;
    makos_u32 local_flags;
    unsigned char control_characters[32];
    makos_u32 input_speed;
    makos_u32 output_speed;
};

struct makos_winsize {
    makos_u16 rows;
    makos_u16 columns;
    makos_u16 pixel_width;
    makos_u16 pixel_height;
};

struct makos_signal_action {
    makos_u64 handler;
    makos_u64 flags;
    makos_u64 restorer;
    makos_u64 mask;
};

struct makos_sockaddr_in {
    makos_u16 family;
    makos_u16 port_be;
    unsigned char address[4];
};

long __makos_raw_syscall4(makos_u64 number, makos_u64 first,
                          makos_u64 second, makos_u64 third,
                          makos_u64 fourth);
long __makos_read(long fd, void *buffer, makos_size_t length);
long __makos_write(long fd, const void *buffer, makos_size_t length);
long __makos_open(const char *path, makos_size_t path_length, int write_access);
long __makos_close(long fd);
long __makos_create(const char *path, makos_size_t path_length);
long __makos_yield(void);
makos_u64 __makos_clock_ticks(void);
long __makos_isatty(long fd);
long __makos_tcgetattr(long fd, struct makos_termios *value);
long __makos_tcsetattr(long fd, unsigned action,
                       const struct makos_termios *value);
long __makos_tcflush(long fd, unsigned queue);
long __makos_ioctl_winsize(long fd, unsigned request,
                           struct makos_winsize *value);
long __makos_sigaction(unsigned signal,
                       const struct makos_signal_action *replacement,
                       struct makos_signal_action *previous);
long __makos_raise(unsigned signal);
long __makos_socket_create(unsigned domain, unsigned type, unsigned protocol);
long __makos_socket_connect(long socket,
                            const struct makos_sockaddr_in *address);
long __makos_socket_send(long socket, const void *buffer,
                         makos_size_t length);
long __makos_socket_receive(long socket, void *buffer, makos_size_t length);
long __makos_socket_close(long socket);
__attribute__((noreturn)) void __makos_exit(int status);

#endif
