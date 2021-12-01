#include <fstream>
#include <iostream>

int main(int argc, char** argv) {
  if (argc != 4) {
    std::cerr << "Usage: correttore <input> <correct output> <test output>"
              << std::endl;
    return 1;
  }
  std::ifstream in(argv[1]);    // NOLINT
  std::ifstream cor(argv[2]);   // NOLINT
  std::ifstream test(argv[3]);  // NOLINT

  int N, N_cor, N_test;
  in >> N;
  cor >> N_cor;
  test >> N_test;

  if (N_cor == N_test) {
    std::cout << 1.0 << std::endl;
    std::cerr << "Ok!" << std::endl;
  } else {
    std::cout << 0.0 << std::endl;
    std::cerr << "Ko!" << std::endl;
  }
}
