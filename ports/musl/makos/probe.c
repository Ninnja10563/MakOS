/* SPDX-License-Identifier: MIT */
#include "syscall.h"

int __makos_bootstrap_main(makos_u64 loader_argument)
{
    static const char message[] = "MAKOS_MUSL_ABI_PROBE_OK\n";
    long written = __makos_write(1, message, sizeof(message) - 1);
    makos_u64 ticks = __makos_clock_ticks();
    return written == (long)(sizeof(message) - 1) && ticks != 0 &&
           loader_argument == 0 ? 0 : 1;
}
