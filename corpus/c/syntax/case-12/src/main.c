int broken_if(void) {
    if (1 {
        return 1;
    }
    return 0;
}

int broken_type(void) {
    int *ptr = 0;
    return ptr + "x";
}
