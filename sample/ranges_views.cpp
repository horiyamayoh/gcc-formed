#include <ranges>

int main() {
    auto result = 42 | std::views::take(2);
    return result.empty();
}
