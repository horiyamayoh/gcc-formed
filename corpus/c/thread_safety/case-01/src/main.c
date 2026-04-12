struct Blob {
    int x;
};

int main(void) {
    struct Blob blob = {1};
    __atomic_store_n(&blob, blob, __ATOMIC_SEQ_CST);
    return 0;
}
