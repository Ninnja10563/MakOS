#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <stdio.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <unistd.h>

static const char message[] =
	"MAKOS_MUSL_RUNTIME_OK version=1.2.6 libc=upstream-static "
	"syscalls=open,read,close,write,exit custom_entry=1 crt=upstream\n";

void _start(void)
{
	char byte;
	int fd = open("/boot-count.txt", O_RDONLY);
	if (fd < 0 || read(fd, &byte, 1) != 1 || close(fd) != 0)
		_exit(111);
	if (write(1, message, sizeof message - 1) != sizeof message - 1)
		_exit(112);
	_exit(42);
}
