#include <iostream>

int main(int argc, char** argv) {
  if (argc != 2) {
    std::cerr << "Usage: " << argv[0] << " N" << std::endl;  // NOLINT
    return 1;
  }
  std::cout << argv[1] << std::endl;  // NOLINT
  std::cerr << "This string should not appear in the input.txt" << std::endl;
}
