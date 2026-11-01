#include <stddef.h>
#include <stdint.h>

extern int bf_program(uint8_t *tape, size_t size);
int bf_input(void) { return -1; }
int bf_output(int byte) { (void)byte; return 0; }

int main(void) {
    uint8_t tape[1] = {0};
    int status = bf_program(tape, sizeof(tape));
    return status != 2 || tape[0] != 0;
}
