#include <stdlib.h>

// Override the common exit functions, EXIT will throw an exception caught by
// the fuzzer
#define _exit EXIT
#define exit EXIT

struct Exit {
  int status;
};

[[noreturn]] inline void EXIT(int status) { throw Exit{status}; }
