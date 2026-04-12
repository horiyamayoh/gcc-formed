#include "wrapper.h"

typedef struct {
    int value;
} Box;

int main(void) {
    Box box = {1};
    return READ_MISSING(box);
}
