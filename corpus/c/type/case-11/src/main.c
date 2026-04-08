void takes_int_ptr(int *value);

struct Box {
    int value;
};

int main(void) {
    struct Box box = {1};
    takes_int_ptr(&box);
    return 0;
}
