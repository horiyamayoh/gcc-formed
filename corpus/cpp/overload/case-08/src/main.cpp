void takes_ref(int&);
void takes_ref(long&);

int main() {
    const char *value = "x";
    takes_ref(value);
}
