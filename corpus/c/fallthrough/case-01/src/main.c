int main(void) {
    int value = 0;
    switch (value) {
    case 0:
        value = 1;
    case 1:
        return value;
    default:
        return 2;
    }
}
