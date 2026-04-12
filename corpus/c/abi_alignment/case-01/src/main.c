typedef struct __attribute__((aligned(16))) BigAlign {
    int value;
} BigAlign;

BigAlign *convert(char *raw) {
    return (BigAlign *)(raw + 1);
}
