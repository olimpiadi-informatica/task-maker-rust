#include <iostream>

volatile unsigned long long X;

int main() {
  int a, b;
  std::cin >> a >> b;
  std::cout << a + b << std::endl;
  for (unsigned long long i = 0;; i++) {
    X = i + X / 2;
  }
}
