#include <fstream>
#include <iostream>

int main() {
  int N;
  std::ifstream in("input.txt");
  std::ofstream out("output.txt");

  in >> N;
  out << N << std::endl;
}
