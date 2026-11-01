#include <stddef.h>
#include <stdint.h>

__attribute__((ms_abi)) int bf_program(uint8_t *, size_t);
#ifndef EXPECTED
#define EXPECTED 65
#endif
static uint32_t result;
__attribute__((ms_abi)) int bf_input(void) { return 65; }
__attribute__((ms_abi)) int bf_output(uint32_t value) { result = value; return 0; }

int main(void) {
    uint32_t tape[30000] = {0};
    return bf_program((uint8_t *)tape, sizeof(tape)) != 0 || result != EXPECTED;
}
