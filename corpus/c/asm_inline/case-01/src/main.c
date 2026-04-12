int main(void) {
    int value = 1;
    __asm__("" : : "i"(value));
    return 0;
}
