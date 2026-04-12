int* make_ptr() {
    int value = 1;
    return &value;
}

int main() {
    return *make_ptr();
}
