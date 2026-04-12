template <typename T>
concept HasSize = requires(T value) { value.size(); };

template <HasSize T>
int consume(T value) { return static_cast<int>(value.size()); }

int main() {
    return consume(1);
}
