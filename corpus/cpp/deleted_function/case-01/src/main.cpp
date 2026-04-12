struct NoCopy {
    NoCopy() = default;
    NoCopy(const NoCopy&) = delete;
};

void take(NoCopy value) {}

int main() {
    NoCopy value;
    take(value);
}
