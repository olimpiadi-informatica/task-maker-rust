#include <iostream>
#include <vector>

int main() {
  std::vector<int> vec;
  while (vec.size() < 400000000)
    vec.push_back(vec.size() / (1 + vec.size() / 42));
  std::cout << vec.back() << std::endl;
}
