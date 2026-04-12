int takes(int *ptr) {
    return *ptr;
}

int main(void) {
    const int value = 1;
    return takes(&value);
}
