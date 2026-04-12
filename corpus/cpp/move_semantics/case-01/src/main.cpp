#include <utility>

struct Widget {};

Widget make_widget() {
    Widget value;
    return std::move(value);
}

int main() {
    return static_cast<int>(sizeof(make_widget()));
}
