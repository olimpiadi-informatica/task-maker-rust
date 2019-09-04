#include <cassert>
#include <fstream>
#include <iostream>

int main(int argc, char** argv) {
  if(argc != 4) {
    std::cerr << "Usage: correttore <input> <correct output> <test output>"
              << std::endl;
    return 1;
  }
  assert(false);  // NOLINT
}
