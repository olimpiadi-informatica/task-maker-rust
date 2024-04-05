#include <fstream>

int main() {
    std::ifstream in("input.txt");
    std::ofstream out("output.txt");

    int n;
    in >> n;
    out << n << '\n';
}
