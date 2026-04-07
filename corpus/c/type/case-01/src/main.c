int takes_int(int value) { return value; }

int main(void) {
    const char *value = "x";
    return takes_int(value);
}
