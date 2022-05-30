#include "iolib.hpp"
#include "iospec.hpp"

#include <cassert>
#include <fstream>

int main(int argc, char** argv) {
    std::ifstream input(argv[1]);
    std::ifstream correct_output(argv[2]);
    std::ifstream submission_output(argv[3]);
    
    IoData correct_data = read_input_output<IoData>(input, correct_output);
    IoData submission_data = read_input_output<IoData>(input, submission_output);

    // Check `submission_data` against `correct_data`, e.g.:
    // assert(submission_data.S == correct_data.S);

    // TODO: verify output format and assertions in task-maker, before calling the custom checker

    return 0;
}
