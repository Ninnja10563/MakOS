// SPDX-License-Identifier: MIT
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv, char **envp)
{
	static const char marker[] =
		"MAKOS_MUSL_EXEC_TARGET_OK argc=3 argv=alpha,two-words env=ready pid=preserved dynamic=1\n";
	int environment_ok = 0;
	for (char **entry = envp; entry && *entry; ++entry)
		if (!strcmp(*entry, "MAKOS_EXEC=ready")) environment_ok = 1;
	if (argc != 3 || strcmp(argv[0], "/usr/bin/makos-exec-target") ||
	    strcmp(argv[1], "alpha") || strcmp(argv[2], "two words") ||
	    !environment_ok)
		return 124;
	write(STDOUT_FILENO, marker, sizeof marker - 1);
	return 42;
}
