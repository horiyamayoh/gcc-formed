struct __attribute__((packed)) Packet {
    char c;
    int value;
};

int *get_ptr(struct Packet *packet) {
    return &packet->value;
}
