#include "makos_abi.h"

void _start(void)
{
    static const char message[] = "MAKOS_NANO_ABI_PROBE_OK\n";
    long written = nano_makos_console_write(message, sizeof(message) - 1);
    nano_makos_exit(written == (long)(sizeof(message) - 1) ? 0 : 1);
}
