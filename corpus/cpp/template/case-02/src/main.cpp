template <typename T>
void expect_ptr(T*) {}

int main() {
    int value = 0;
    expect_ptr(value);
}
