#include "iolib.hpp"
#include "iospec.hpp"

int main(int argc, char** argv) {
    IoData data;

    // Fill-in `data` with generated values, e.g.:
    // data.N = atol(argv[1]);

    write_input(data);

    return 0;
}
