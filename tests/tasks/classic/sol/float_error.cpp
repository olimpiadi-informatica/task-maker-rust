#include <iostream>

volatile int x = 0;

int main() { std::cout << (42 % x) << std::endl; }
