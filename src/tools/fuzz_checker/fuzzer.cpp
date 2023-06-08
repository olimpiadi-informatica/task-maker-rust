#ifdef NDEBUG
#undef NDEBUG
#endif

#include <assert.h>
#include <dlfcn.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <iostream>
#include <string>
#include <vector>

typedef int (*new_main_t)(int argc, char **argv);

struct Exit {
  int status;
};

#ifndef NUM_INPUTS
#error Missing NUM_INPUTS
#endif

#ifndef FUZZ_DIRECTORY
#error Missing FUZZ_DIRECTORY
#endif

#ifndef TASK_DIRECTORY
#error Missing TASK_DIRECTORY
#endif

new_main_t setupDlOpen() {
  void *dl = dlopen(FUZZ_DIRECTORY "/fuzzer/checker.so", RTLD_NOW);
  if (!dl) {
    std::cerr << "Cannot open checker.so: " << dlerror() << '\n';
    std::exit(1);
  }

  new_main_t checker_main = (new_main_t)dlsym(dl, "main");
  if (!checker_main) {
    std::cerr << "Cannot find main function from checker.so: " << dlerror()
              << '\n';
    std::exit(1);
  }

  return checker_main;
}

extern "C" int LLVMFuzzerTestOneInput(const uint8_t *data, size_t size) {
  static auto checker_main = setupDlOpen();

  // Prepare input ID.
  if (size <= 4)
    return 0;
  uint32_t input_id;
  memcpy(&input_id, data, 4);
  input_id = input_id % NUM_INPUTS;

  // Prepare input file.
  int in_fd = open(FUZZ_DIRECTORY, O_TMPFILE | O_RDWR, S_IRUSR | S_IWUSR);
  assert(in_fd != -1);
  size_t pos = 4;
  while (pos < size) {
    ssize_t written = write(in_fd, data + pos, size - pos);
    assert(written > 0);
    pos += written;
  }
  lseek(in_fd, 0, SEEK_SET);

  // Prepare stdout file.
  int out_fd = open(FUZZ_DIRECTORY, O_TMPFILE | O_RDWR, S_IRUSR | S_IWUSR);
  assert(out_fd != -1);
  assert(dup2(out_fd, STDOUT_FILENO) != -1);

  // Suppress stderr from the checker.
  int err_fd = open("/dev/null", O_RDONLY);
  int old_stderr = dup(STDERR_FILENO);
  assert(old_stderr != -1);
  assert(err_fd != -1);

  // Prepare argv.
  std::string input_file =
      TASK_DIRECTORY "/input/input" + std::to_string(input_id) + ".txt";
  std::string cor_file =
      TASK_DIRECTORY "/output/output" + std::to_string(input_id) + ".txt";
  std::string output_file = "/dev/fd/" + std::to_string(in_fd);

  const char arg0_c[] = FUZZ_DIRECTORY "/checker";
  std::vector<char> arg0(std::begin(arg0_c), std::end(arg0_c));
  arg0.push_back(0);
  std::vector<char> i_file(input_file.begin(), input_file.end());
  i_file.push_back(0);
  std::vector<char> c_file(cor_file.begin(), cor_file.end());
  c_file.push_back(0);
  std::vector<char> o_file(output_file.begin(), output_file.end());
  o_file.push_back(0);
  char *argv[5] = {arg0.data(), i_file.data(), c_file.data(), o_file.data(),
                   nullptr};

  // Call the checker.
  assert(dup2(err_fd, STDERR_FILENO) != -1);
  int ret;
  try {
    ret = checker_main(4, argv);
  } catch (Exit r) {
    ret = r.status;
  }
  assert(dup2(old_stderr, STDERR_FILENO) != -1);
  assert(ret == 0);

  // Check that the checker produced a [0, 1] float.
  fflush(stdout);
  lseek(out_fd, 0, SEEK_SET);
  FILE *f = fdopen(out_fd, "r");
  float score;
  assert(fscanf(f, "%f", &score) == 1);
  assert(!(score < 0));
  assert(!(score > 1));

  // Close FDs.
  fclose(f);
  close(in_fd);
  close(out_fd);
  close(err_fd);
  close(old_stderr);
  return 0;
}
