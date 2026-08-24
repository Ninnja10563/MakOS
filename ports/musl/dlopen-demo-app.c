#include <dlfcn.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>

typedef uint64_t (*add_fn)(uint64_t, uint64_t);

int main(int argc, char **argv)
{
	static const char marker[] =
		"MAKOS_MUSL_DLOPEN_OK loader=upstream-musl path=/usr/lib/libmakosdemo.so "
		"mode=RTLD_NOW dlsym=makos_shared_add result=42 dlclose=1 "
		"large_file=libc.so,multi-page\n";
	if (argc != 2 || strcmp(argv[0], "/system/musl-dlopen-probe") ||
	    strcmp(argv[1], "runtime"))
		return 121;
	void *libc = dlopen("/usr/lib/libc.so", RTLD_NOW | RTLD_LOCAL);
	if (!libc || !dlsym(libc, "malloc") || dlclose(libc))
		return 122;
	void *handle = dlopen("/usr/lib/libmakosdemo.so", RTLD_NOW | RTLD_LOCAL);
	if (!handle)
		return 123;
	add_fn add = (add_fn)dlsym(handle, "makos_shared_add");
	if (!add || add(19, 23) != 42)
		return 124;
	if (dlclose(handle))
		return 125;
	if (write(1, marker, sizeof marker - 1) != sizeof marker - 1)
		return 126;
	return 42;
}
