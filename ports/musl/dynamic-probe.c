#include <string.h>
#include <unistd.h>

int main(int argc, char **argv)
{
	static const char marker[] =
		"MAKOS_MUSL_DYNAMIC_OK loader=musl relocations=executed\n";
	if (argc != 2 || strcmp(argv[0], "/system/musl-dynamic-probe") ||
	    strcmp(argv[1], "dynamic"))
		return 121;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 122;
	return 42;
}
