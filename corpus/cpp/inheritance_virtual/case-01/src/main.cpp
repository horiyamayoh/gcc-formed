struct Base {
    virtual int value() const = 0;
};

struct Derived : Base {};

int main() {
    Derived value;
    return 0;
}
