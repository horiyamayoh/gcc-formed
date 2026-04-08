template <typename T>
struct Pair {
    Pair(T, T) {}
};

int main() {
    Pair value(1, "x");
    (void)value;
}
