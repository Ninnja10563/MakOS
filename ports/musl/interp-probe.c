#include <stddef.h>
#include <stdint.h>

enum { SYS_WRITE = 0, SYS_EXIT = 5 };

static const char marker[] =
	"MAKOS_MUSL_INTERP_OK loader=upstream-musl pt_interp=1 "
	"relative_relocation=1 entry=dynamic\n";
static const char *volatile relocated_marker = marker;

static uint64_t syscall2(uint64_t number, uint64_t first, uint64_t second)
{
	register uint64_t x0 __asm__("x0") = first;
	register uint64_t x1 __asm__("x1") = second;
	register uint64_t x8 __asm__("x8") = number;
	__asm__ volatile("svc #0" : "+r"(x0) : "r"(x1), "r"(x8) : "memory", "cc");
	return x0;
}

void _start(void)
{
	const char *value = relocated_marker;
	size_t length = 0;
	while (value[length])
		++length;
	if (value != marker || syscall2(SYS_WRITE, (uintptr_t)value, length) != length)
		syscall2(SYS_EXIT, 120, 0);
	syscall2(SYS_EXIT, 42, 0);
	for (;;)
		__asm__ volatile("wfe");
}
