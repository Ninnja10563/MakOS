// SPDX-License-Identifier: MIT
#include <unistd.h>

int main(void)
{
	static const char marker[] =
		"MAKOS_MUSL_EXEC_CALL_OK syscall=execve target=/usr/bin/makos-exec-target\n";
	char *argv[] = {
		"/usr/bin/makos-exec-target",
		"alpha",
		"two words",
		0,
	};
	char *envp[] = {
		"MAKOS_EXEC=ready",
		"PATH=/usr/bin",
		0,
	};
	write(STDOUT_FILENO, marker, sizeof marker - 1);
	execve(argv[0], argv, envp);
	return 125;
}
