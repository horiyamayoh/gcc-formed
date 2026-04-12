enum class Color { Red, Blue };

int main() {
    Color color = Color::Red;
    switch (color) {
    case Color::Red:
        return 0;
    }
    return 1;
}
