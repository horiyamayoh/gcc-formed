extern void consume(int);

int read_value(int flag) {
    int value;
    if (flag == 1)
        value = 1;
    else if (flag == 2)
        value = 2;
    consume(value);
    return 0;
}
