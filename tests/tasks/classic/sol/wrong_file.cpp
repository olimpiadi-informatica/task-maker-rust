#include <fstream>

int main() {
  int N;
  std::ifstream in("imput.txt");
  std::ofstream out("oufput.txt");
  in >> N;
  out << N << std::endl;
}
