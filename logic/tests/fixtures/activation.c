#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

__attribute__((constructor)) static void prepare(void) {
    char byte = 1;
    write(@ENTERED@, &byte, 1);
    read(@RELEASE@, &byte, 1);
}

typedef struct { uint8_t output[4096]; uint32_t length; } Session;
uint32_t exact_logic_abi(void) { return @ABI@ + @REFUSE@; }
uintptr_t exact_logic_create(void) { return (uintptr_t)calloc(1, sizeof(Session)); }
void exact_logic_destroy(uintptr_t session) { free((void *)session); }
uintptr_t exact_logic_alloc(uint32_t len) { return (uintptr_t)malloc(len); }
void exact_logic_dealloc(uintptr_t ptr, uint32_t len) { free((void *)ptr); }
uint32_t exact_logic_call(uintptr_t session, uintptr_t ptr, uint32_t len) {
    uint8_t metadata[] = {@METADATA@}, unit[] = {@UNIT@}, answer[] = {@ANSWER@};
    uint8_t op = ((uint8_t *)ptr)[4];
    uint8_t *bytes = op == 0 ? metadata : op < 3 ? unit : answer;
    uint32_t length = op == 0 ? sizeof(metadata) : op < 3 ? sizeof(unit) : sizeof(answer);
    memcpy(((Session *)session)->output, bytes, length);
    ((Session *)session)->length = length;
    return 0;
}
uintptr_t exact_logic_output(uintptr_t session) { return (uintptr_t)((Session *)session)->output; }
uint32_t exact_logic_output_len(uintptr_t session) { return ((Session *)session)->length; }
