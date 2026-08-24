/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/ */

#include "_makos.cfg"

#define MAP_PRIVATE 2
#include "_makos.h"

#if !defined(MAKOS) || !defined(XP_UNIX)
#error "NSPR MakOS platform identity missing"
#endif
#if defined(LINUX) || defined(__linux__)
#error "NSPR MakOS platform must not masquerade as Linux"
#endif

_Static_assert(PR_BYTES_PER_LONG == 8, "MakOS uses AArch64 LP64");
_Static_assert(PR_BYTES_PER_WORD == 8, "MakOS word size changed");
_Static_assert(PR_ALIGN_OF_POINTER == 8, "MakOS pointer alignment changed");
_Static_assert(IS_LITTLE_ENDIAN == 1, "MakOS target is little-endian");

const char* makos_nspr_platform(void) { return PR_LINKER_ARCH; }
