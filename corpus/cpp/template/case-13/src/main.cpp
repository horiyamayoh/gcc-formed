template <typename T>
void expect_ptr(T*) {}

template <typename T>
void call_expect(T value) {
    expect_ptr(value);
}

int main() {
    int value = 0;
    call_expect(value);
}
