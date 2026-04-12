struct Pair {
    int first;
    int second;
};

int main() {
    Pair pair{1, 2};
    auto [only] = pair;
    return only;
}
