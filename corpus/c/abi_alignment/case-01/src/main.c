struct __attribute__((packed)) Packed {
    char c;
    int value;
};

int *get_ptr(struct Packed *packed) {
    return &packed->value;
}
