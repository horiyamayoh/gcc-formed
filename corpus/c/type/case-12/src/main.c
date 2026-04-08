struct Box {
    int value;
};

int main(void) {
    struct Box box = {1};
    int value = box;
    return value;
}
