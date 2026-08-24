/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. https://mozilla.org/MPL/2.0/ */

int makos_stack_protector_probe(const char *input) {
    volatile char local[32];
    int index = 0;
    while (index < 31 && input[index] != 0) {
        local[index] = input[index];
        ++index;
    }
    local[index] = 0;
    return local[0];
}
