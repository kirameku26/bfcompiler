#include <stddef.h>
#include <stdint.h>

__attribute__((ms_abi)) int bf_program(uint8_t *, size_t);
static int result;
__attribute__((ms_abi)) int bf_input(void) { return 65; }
__attribute__((ms_abi)) int bf_output(int byte) { result = byte; return 0; }

int main(void) {
    uint8_t tape[30000] = {0};
    return bf_program(tape, sizeof(tape)) != 0 || result != 65;
}
