template <typename T> struct Box { T value; };
int main() { Box<int> box{1}; return box.value; }
