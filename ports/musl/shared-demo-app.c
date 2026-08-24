#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>

extern const char makos_shared_name[];
extern uint64_t makos_shared_add(uint64_t, uint64_t);

int main(int argc, char **argv)
{
	static const char marker[] =
		"MAKOS_MUSL_DSO_OK loader=upstream-musl needed=libmakosdemo.so "
		"symbol=makos_shared_add result=42 file=vfs\n";
	if (argc != 2 || strcmp(argv[0], "/system/musl-dso-probe") ||
	    strcmp(argv[1], "shared") ||
	    strcmp(makos_shared_name, "libmakosdemo.so") ||
	    makos_shared_add(20, 22) != 42)
		return 121;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 122;
	return 42;
}
