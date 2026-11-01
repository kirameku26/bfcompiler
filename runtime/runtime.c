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
#ifndef BFC_OUTPUT_ENCODING
#define BFC_OUTPUT_ENCODING ((BFC_OUTPUT_UTF8) ? 1 : 0)
#endif

extern int bf_program(uint8_t *tape, size_t tape_size);

struct bf_configuration {
    uint32_t version;
    uint32_t tape_size;
    uint32_t cell_bits;
    uint32_t output_encoding;
};
/* A strong definition in an extended object overrides this legacy default. */
__attribute__((weak)) const struct bf_configuration bf_config = {0, 0, 0, 0};
static uint32_t output_encoding = BFC_OUTPUT_ENCODING;

int bf_input(void) {
    int byte = getchar();
    if (byte != EOF) {
        return byte;
    }
    return ferror(stdin) ? -2 : -1;
}

static void encode_u16(unsigned char *bytes, uint32_t value, int big_endian) {
    bytes[big_endian ? 1 : 0] = (unsigned char)value;
    bytes[big_endian ? 0 : 1] = (unsigned char)(value >> 8);
}

int bf_output(uint32_t value) {
    if (output_encoding == 0) {
        return putchar((int)(value & 0xff)) == EOF ? 2 : 0;
    }
    if (value > 0x10ffff || (value >= 0xd800 && value <= 0xdfff)) {
        return 2;
    }
    unsigned char bytes[4];
    size_t length;
    if (output_encoding == 2 || output_encoding == 3) {
        int big_endian = output_encoding == 3;
        if (value < 0x10000) {
            encode_u16(bytes, value, big_endian);
            length = 2;
        } else {
            uint32_t scalar = value - 0x10000;
            encode_u16(bytes, 0xd800 | (scalar >> 10), big_endian);
            encode_u16(bytes + 2, 0xdc00 | (scalar & 0x3ff), big_endian);
            length = 4;
        }
    } else if (output_encoding == 4 || output_encoding == 5) {
        int big_endian = output_encoding == 5;
        for (unsigned int i = 0; i < 4; ++i) {
            bytes[i] = (unsigned char)(value >> ((big_endian ? 3 - i : i) * 8));
        }
        length = 4;
    } else if (value < 0x80) {
        bytes[0] = (unsigned char)value;
        length = 1;
    } else if (value < 0x800) {
        bytes[0] = (unsigned char)(0xc0 | (value >> 6));
        bytes[1] = (unsigned char)(0x80 | (value & 0x3f));
        length = 2;
    } else if (value < 0x10000) {
        bytes[0] = (unsigned char)(0xe0 | (value >> 12));
        bytes[1] = (unsigned char)(0x80 | ((value >> 6) & 0x3f));
        bytes[2] = (unsigned char)(0x80 | (value & 0x3f));
        length = 3;
    } else {
        bytes[0] = (unsigned char)(0xf0 | (value >> 18));
        bytes[1] = (unsigned char)(0x80 | ((value >> 12) & 0x3f));
        bytes[2] = (unsigned char)(0x80 | ((value >> 6) & 0x3f));
        bytes[3] = (unsigned char)(0x80 | (value & 0x3f));
        length = 4;
    }
    return fwrite(bytes, 1, length, stdout) == length ? 0 : 2;
}

int main(void) {
#ifdef _WIN32
    if (_setmode(_fileno(stdin), _O_BINARY) == -1 ||
        _setmode(_fileno(stdout), _O_BINARY) == -1) {
        fputs("brainfuck runtime: cannot set binary mode\n", stderr);
        return 1;
    }
#endif
    size_t tape_size = BFC_TAPE_SIZE;
    size_t cell_bytes = 1;
    if (bf_config.version != 0) {
        if ((bf_config.version != 1 && bf_config.version != 2) || bf_config.tape_size == 0 ||
            bf_config.tape_size > INT32_MAX ||
            (bf_config.cell_bits != 8 && bf_config.cell_bits != 16 && bf_config.cell_bits != 32) ||
            bf_config.output_encoding > (bf_config.version == 1 ? 1u : 5u)) {
            fputs("brainfuck runtime: invalid embedded configuration\n", stderr);
            return 1;
        }
        tape_size = bf_config.tape_size;
        cell_bytes = bf_config.cell_bits / 8;
        output_encoding = bf_config.output_encoding;
    }
    if (output_encoding > 5) {
        fputs("brainfuck runtime: invalid output encoding\n", stderr);
        return 1;
    }
    if (tape_size > SIZE_MAX / cell_bytes) {
        fputs("brainfuck runtime: tape size exceeds address space\n", stderr);
        return 1;
    }
    uint8_t *tape = calloc(tape_size, cell_bytes);
    if (tape == NULL) {
        fputs("brainfuck runtime: cannot allocate tape\n", stderr);
        return 1;
    }
    int status = bf_program(tape, tape_size * cell_bytes);
    if (fflush(stdout) == EOF) status = 2;
    free(tape);
    if (status == 2) {
        fputs("brainfuck runtime: tape boundary or output error\n", stderr);
    }
    return status;
}

