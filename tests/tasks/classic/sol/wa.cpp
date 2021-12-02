#include <fstream>
#include <iostream>

int main() { std::ofstream("output.txt") << 42 << std::endl; }
