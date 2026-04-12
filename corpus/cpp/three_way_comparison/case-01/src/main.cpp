struct Widget {};

int main() {
    Widget left;
    Widget right;
    return (left <=> right) == 0;
}
