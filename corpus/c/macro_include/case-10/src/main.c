#define READ_FIELD(item) ((item).missing)
typedef struct { int value; } Box;
int main(void) { Box box = {1}; return READ_FIELD(box); }
