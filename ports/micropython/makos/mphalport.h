#pragma once

#include <stdint.h>
#include "py/mpconfig.h"

mp_uint_t mp_hal_stdout_tx_strn(const char *text, mp_uint_t length);
int mp_hal_stdin_rx_chr(void);

static inline mp_uint_t mp_hal_ticks_ms(void) {
    return 0;
}

static inline void mp_hal_set_interrupt_char(char value) {
    (void)value;
}
