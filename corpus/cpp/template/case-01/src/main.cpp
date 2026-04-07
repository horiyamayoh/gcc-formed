template <typename T>
void takes_same(T, T) {}

int main() { takes_same(1, 2.0); }
