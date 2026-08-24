#include <fcntl.h>
#include <string.h>
#include <unistd.h>

__attribute__((noinline)) static void overwrite(volatile unsigned char *bytes,
						 size_t count)
{
	for (size_t index = 0; index < count; ++index)
		bytes[index] = (unsigned char)(0xa0 + index);
}

__attribute__((noinline)) static void trigger_stack_check(void)
{
	volatile unsigned char buffer[16];
	overwrite(buffer, 32);
}

int main(int argc, char **argv, char **envp)
{
	static const char marker[] =
		"MAKOS_MUSL_CRT_OK version=1.2.6 crt1=upstream "
		"libc_start_main=1 argc=2 envp=1 tls=1 tid=real "
		"fd=dup,dup3,fcntl,shared-offset,lseek,cloexec\n";
	static const char stack_marker[] =
		"MAKOS_STACK_PROTECTOR_TRIGGER_OK instrumentation=strong canary=corrupt\n";
	char first, duplicate_byte, next;
	if (argc == 2 && !strcmp(argv[0], "/system/musl-crt-probe") &&
	    !strcmp(argv[1], "stack-smash") && envp[0] &&
	    !strcmp(envp[0], "MODE=stack-smash") && !envp[1]) {
		if (write(1, stack_marker, sizeof stack_marker - 1) !=
		    sizeof stack_marker - 1)
			return 133;
		trigger_stack_check();
		return 134;
	}
	if (argc != 2 || strcmp(argv[0], "/system/musl-crt-probe") ||
	    strcmp(argv[1], "crt") || !envp[0] || strcmp(envp[0], "MODE=crt") ||
	    envp[1])
		return 121;
	int fd = open("/boot-count.txt", O_RDONLY);
	if (fd < 0 || read(fd, &first, 1) != 1)
		return 122;
	int copy = dup(fd);
	if (copy < 0 || lseek(copy, 0, SEEK_SET) != 0 ||
	    read(copy, &duplicate_byte, 1) != 1 || duplicate_byte != first ||
	    read(fd, &next, 1) != 1)
		return 124;
	int exact = dup3(fd, 9, O_CLOEXEC);
	if (exact != 9 || fcntl(exact, F_GETFD) != FD_CLOEXEC ||
	    fcntl(exact, F_SETFD, 0) != 0 || fcntl(exact, F_GETFD) != 0)
		return 125;
	int minimum = fcntl(fd, F_DUPFD, 7);
	int cloexec_minimum = fcntl(fd, F_DUPFD_CLOEXEC, 10);
	if (minimum < 7)
		return 126;
	if (cloexec_minimum < 10)
		return 127;
	if (fcntl(cloexec_minimum, F_GETFD) != FD_CLOEXEC)
		return 128;
	if ((fcntl(fd, F_GETFL) & O_ACCMODE) != O_RDONLY)
		return 129;
	if (fcntl(fd, F_SETFL, O_NONBLOCK) != 0)
		return 130;
	if (!(fcntl(copy, F_GETFL) & O_NONBLOCK))
		return 131;
	if (close(cloexec_minimum) != 0 || close(minimum) != 0 ||
	    close(exact) != 0 || close(copy) != 0 || close(fd) != 0)
		return 132;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 123;
	return 42;
}
