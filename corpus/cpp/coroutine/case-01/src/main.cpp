#include <coroutine>

struct Task {};

Task make_task() {
    co_return;
}

int main() {
    make_task();
    return 0;
}
