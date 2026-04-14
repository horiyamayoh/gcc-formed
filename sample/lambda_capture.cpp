int main() {
    int value = 1;
    auto lambda = []() { return value; };
    return lambda();
}
