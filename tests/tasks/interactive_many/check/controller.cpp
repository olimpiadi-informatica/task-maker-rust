#include "controller_lib.h"
#include <string>

int handler(FILE **to_solution, FILE **from_solution, int num) {
  int v = 0;
  for (int i = 0; i < num; i++) {
    fprintf(to_solution[i], "%d\n", v);
    fflush(to_solution[i]);
    if (fscanf(from_solution[i], "%d", &v) != 1) {
      grade(0.0, "translate:wrong",
            ("no output at step " + std::to_string(i)).c_str());
    }
    if (v != i + 1) {
      grade(0.0, "translate:wrong",
            ("wrong output at step " + std::to_string(i)).c_str());
    }
  }
  grade(1.0, "translate:success", nullptr);
  return 0;
}

int main() { return start_many_solutions(handler, 20); }
