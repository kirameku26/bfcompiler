#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

#ifndef BFC_TAPE_SIZE
#define BFC_TAPE_SIZE 30000
#endif
#ifndef BFC_OUTPUT_UTF8
#define BFC_OUTPUT_UTF8 0
#endif

extern int bf_program(uint8_t *tape, size_t tape_size);

int bf_input(void) {
    int byte = getchar();
    if (byte != EOF) {
        return byte;
    }
    return ferror(stdin) ? -2 : -1;
}

int bf_output(int byte) {
#if BFC_OUTPUT_UTF8
    if (byte >= 0x80) {
        if (putchar(0xc0 | (byte >> 6)) == EOF) {
            return 2;
        }
        byte = 0x80 | (byte & 0x3f);
    }
#endif
    return putchar(byte) == EOF ? 2 : 0;
}

int main(void) {
#ifdef _WIN32
    if (_setmode(_fileno(stdin), _O_BINARY) == -1 ||
        _setmode(_fileno(stdout), _O_BINARY) == -1) {
        fputs("brainfuck runtime: cannot set binary mode\n", stderr);
        return 1;
    }
#endif
    uint8_t *tape = calloc(BFC_TAPE_SIZE, sizeof(*tape));
    if (tape == NULL) {
        fputs("brainfuck runtime: cannot allocate tape\n", stderr);
        return 1;
    }
    int status = bf_program(tape, BFC_TAPE_SIZE);
    free(tape);
    if (status == 2) {
        fputs("brainfuck runtime: tape boundary or output error\n", stderr);
    }
    return status;
}

