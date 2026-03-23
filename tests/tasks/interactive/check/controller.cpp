#include "controller_lib.h"
#include <cstdio>

int test(FILE *to_solution, FILE *from_solution) {
  FILE *input = fopen("input.txt", "r");
  assert(input);
  int a, b, c;
  fscanf(input, "%d %d %d", &a, &b, &c);

  fprintf(to_solution, "%d %d\n", a, b);
  fflush(to_solution);

  int res1;
  if (fscanf(from_solution, "%d", &res1) != 1) {
    grade(0.0, "Ko1!", nullptr);
  }

  fprintf(to_solution, "%d %d\n", res1, c);
  fflush(to_solution);

  int res2;
  if (fscanf(from_solution, "%d", &res2) != 1) {
    grade(0.0, "Ko2!", nullptr);
  }

  if (res2 == (a + b) * c) {
    grade(1.0, "Ok!", nullptr);
  } else {
    grade(0.0, "Ko!", nullptr);
  }

  return 0;
}

int main() { return start_one_solution(test); }
