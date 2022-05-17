#include "iolib.hpp"
#include "iospec.hpp"

#include <cassert>
#include <fstream>

int main(int argc, char** argv) {
    std::ifstream input(argv[1]);
    auto data = read_input<IoData>(input);

    // Check any non-trivial assumptions here, e.g.:
    // assert(is_prime_number(data.N));
    // assert(is_graph_connected(data));

    // TODO: verify input format and assumptions in task-maker, before calling the custom validator

    return 0;
}
