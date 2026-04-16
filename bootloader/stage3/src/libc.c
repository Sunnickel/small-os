#include <stdint.h>
#include <stddef.h>

int memcmp(const void* a, const void* b, size_t n)
{
    const unsigned char* x = a;
    const unsigned char* y = b;

    for (size_t i = 0; i < n; i++)
    {
        if (x[i] != y[i])
            return (int)x[i] - (int)y[i];
    }
    return 0;
}

void* memcpy(void* dst, const void* src, size_t n)
{
    unsigned char* d = dst;
    const unsigned char* s = src;

    for (size_t i = 0; i < n; i++)
        d[i] = s[i];

    return dst;
}

void* memset(void* dst, int v, size_t n)
{
    unsigned char* d = dst;

    for (size_t i = 0; i < n; i++)
        d[i] = (unsigned char)v;

    return dst;
}