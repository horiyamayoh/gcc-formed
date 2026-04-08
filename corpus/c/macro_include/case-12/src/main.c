#include "wrapper.h"

typedef struct {
    int value;
} Counter;

int main(void) {
    Counter counter = {1};
    return OUTER_ACCESS(counter);
}
