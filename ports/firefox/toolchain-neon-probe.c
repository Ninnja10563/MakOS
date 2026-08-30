/* Verify that the cross compiler can provide its own AArch64 intrinsic API. */
#include <arm_neon.h>

uint8x16_t makos_neon_add(uint8x16_t left, uint8x16_t right) {
    return vaddq_u8(left, right);
}
