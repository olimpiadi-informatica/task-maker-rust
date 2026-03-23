#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

__attribute__((noreturn)) void grade(double score, const char *msg,
                                     const char *admin_msg) {
  fprintf(stderr, "SCORE: %f\n", score);
  fprintf(stderr, "USER_MESSAGE: %s\n", msg);
  if (admin_msg) {
    fprintf(stderr, "ADMIN_MESSAGE: %s\n", admin_msg);
  }
  exit(0);
}

int start_one_solution(int (*handler)(FILE *to_solution, FILE *from_solution)) {
  printf("START_SOLUTION\n");
  fflush(stdout);
  int fdin, fdout;
  scanf("%d%d", &fdin, &fdout);
  FILE *to_solution = fdopen(fdin, "w");
  FILE *from_solution = fdopen(fdout, "r");
  assert(to_solution);
  assert(from_solution);
  int ret = handler(to_solution, from_solution);
  fclose(to_solution);
  fclose(from_solution);
  return ret;
}

int start_many_solutions(int (*handler)(FILE **to_solution,
                                        FILE **from_solution, int num),
                         int num) {
  FILE **to_solution = (FILE **)malloc(sizeof(FILE *) * num);
  FILE **from_solution = (FILE **)malloc(sizeof(FILE *) * num);
  for (int i = 0; i < num; i++) {
    printf("START_SOLUTION\n");
    fflush(stdout);
    int fdin, fdout;
    scanf("%d%d", &fdin, &fdout);
    to_solution[i] = fdopen(fdin, "w");
    from_solution[i] = fdopen(fdout, "r");
    assert(to_solution[i]);
    assert(from_solution[i]);
  }
  int ret = handler(to_solution, from_solution, num);
  for (int i = 0; i < num; i++) {
    fclose(to_solution[i]);
    fclose(from_solution[i]);
  }
  free(to_solution);
  free(from_solution);
  return ret;
}
