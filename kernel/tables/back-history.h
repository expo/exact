#ifndef EXACT_BACK_HISTORY_H
#define EXACT_BACK_HISTORY_H

#include <stdint.h>

// Unicode 17.0.0 White_Space and Default_Ignorable_Code_Point, plus U+2800.
// kernel/build.rs uses these same ranges for the Contract warning.
#define EXACT_HISTORY_BLANK_RANGES(X) \
    X(0x0009, 0x000D) \
    X(0x0020, 0x0020) \
    X(0x0085, 0x0085) \
    X(0x00A0, 0x00A0) \
    X(0x00AD, 0x00AD) \
    X(0x034F, 0x034F) \
    X(0x061C, 0x061C) \
    X(0x115F, 0x1160) \
    X(0x1680, 0x1680) \
    X(0x17B4, 0x17B5) \
    X(0x180B, 0x180F) \
    X(0x2000, 0x200F) \
    X(0x2028, 0x202F) \
    X(0x205F, 0x206F) \
    X(0x2800, 0x2800) \
    X(0x3000, 0x3000) \
    X(0x3164, 0x3164) \
    X(0xFE00, 0xFE0F) \
    X(0xFEFF, 0xFEFF) \
    X(0xFFA0, 0xFFA0) \
    X(0xFFF0, 0xFFF8) \
    X(0x1BCA0, 0x1BCA3) \
    X(0x1D173, 0x1D17A) \
    X(0xE0000, 0xE0FFF)

static inline int exact_history_blank_scalar(uint32_t scalar) {
#define EXACT_IN_RANGE(first, last) || (scalar >= (first) && scalar <= (last))
    return 0 EXACT_HISTORY_BLANK_RANGES(EXACT_IN_RANGE);
#undef EXACT_IN_RANGE
}

#undef EXACT_HISTORY_BLANK_RANGES
#endif
