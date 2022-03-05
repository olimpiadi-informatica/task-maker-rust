#!/bin/bash -e


# Run this script in the folder of a IOI-style task with a checker to try to
# find contestant's outputs that crash the checker, or otherwise make it
# produce invalid output.
# Using this script requires having clang++ installed (the newest the better!),
# and requires that the `main` function returns an `int` and doesn't rely on
# implicitly returning 0 from main.
# Your checker should also not use global variables, nor leak memory.

if ! [ -f cor/correttore.cpp ]
then
  if [ "$QUIET" != "1" ]
  then
    echo "$(pwd): No checker"
  fi
  exit
fi

[ -d output ] || task-maker-rust --ui print > /dev/null



rm -rf fuzz
mkdir -p fuzz

NUM_INPUTS=$(ls input | wc -l)

mkdir fuzz/initial_corpus

le32 () {
  v=$(awk -v n=$1 'BEGIN{printf "%08X", n;}')
  echo -n -e "\\x${v:6:2}\\x${v:4:2}\\x${v:2:2}\\x${v:0:2}" >> $2
}

for i in $(seq 0 $((NUM_INPUTS-1)))
do
  le32 $i fuzz/initial_corpus/$i.txt
  cat output/output$i.txt >> fuzz/initial_corpus/$i.txt
done



cat > fuzz/fuzz.cpp << EOF
#ifdef NDEBUG
#undef NDEBUG
#endif
#include <stdio.h>
#include <string.h>
#include <assert.h>
#include <stdint.h>
#include <unistd.h>
#include <fcntl.h>
#include <string>
#include <vector>
int MAIN(int argc, char **argv);

void EXIT(int status) {
  throw status;
}

extern "C" int LLVMFuzzerTestOneInput(const uint8_t* data, size_t size) {
  // Prepare input ID.
  if (size <= 4) return 0;
  int input_id;
  static_assert(sizeof(input_id) == 4, "int size must be 4");
  memcpy(&input_id, data, 4);
  input_id = ((input_id % $NUM_INPUTS) + $NUM_INPUTS) % $NUM_INPUTS;

  // Prepare input file.
  int in_fd = open("fuzz/", O_TMPFILE | O_RDWR, S_IRUSR | S_IWUSR);
  assert(in_fd != -1);
  size_t pos = 4;
  while (pos < size) {
    ssize_t written = write(in_fd, data + pos, size - pos);
    assert(written > 0);
    pos += written;
  }
  lseek(in_fd, 0, SEEK_SET);

  // Prepare stdout file.
  int out_fd = open("fuzz/", O_TMPFILE | O_RDWR, S_IRUSR | S_IWUSR);
  assert(out_fd != -1);
  assert(dup2(out_fd, STDOUT_FILENO) != -1);

  // Suppress stderr from the checker.
  int err_fd = open("/dev/null", O_RDONLY);
  int old_stderr = dup(STDERR_FILENO);
  assert(old_stderr != -1);
  assert(err_fd != -1);

  // Prepare argv.
  std::string input_file = "input/input" + std::to_string(input_id) + ".txt";
  std::string cor_file = "output/output" + std::to_string(input_id) + ".txt";
  std::string output_file = "/dev/fd/" + std::to_string(in_fd);

  const char arg0_c[] = "checker";
  std::vector<char> arg0(std::begin(arg0_c), std::end(arg0_c)); arg0.push_back(0);
  std::vector<char> i_file(input_file.begin(), input_file.end()); i_file.push_back(0);
  std::vector<char> c_file(cor_file.begin(), cor_file.end()); c_file.push_back(0);
  std::vector<char> o_file(output_file.begin(), output_file.end()); o_file.push_back(0);
  char* argv[5] = {
    arg0.data(),
    i_file.data(),
    c_file.data(),
    o_file.data(),
    nullptr
  };

  // Call the checker.
  assert(dup2(err_fd, STDERR_FILENO) != -1);
  int ret;
  try {
    ret = MAIN(4, argv);
  } catch (int r) {
    ret = r;
  }
  assert(dup2(old_stderr, STDERR_FILENO) != -1);
  assert(ret == 0);

  // Check that the checker produced a 0-1 float.
  fflush(stdout);
  lseek(out_fd, 0, SEEK_SET);
  FILE* f = fdopen(out_fd, "r");
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

EOF

cat > fuzz/checker.cpp << EOF
#include <stdlib.h>
#define main MAIN
#define _exit EXIT
#define exit EXIT

void EXIT(int status);
EOF

sed "s/std::exit/exit/g" cor/correttore.cpp >> fuzz/checker.cpp

clang++ -O2 fuzz/*.cpp -fsanitize=fuzzer,address,undefined -o fuzz/checker

mkdir -p fuzz/corpus fuzz/artifacts

FUZZ_COMMAND="./fuzz/checker -fork=$(nproc) fuzz/initial_corpus -artifact_prefix=fuzz/artifacts/ "$@" fuzz/corpus"

if [ "$QUIET" == "1" ]
then
  $FUZZ_COMMAND 2>/dev/null || true
else
  $FUZZ_COMMAND || true
fi

mkdir -p fuzz/failures

for fail in $(ls fuzz/artifacts)
do
  NUM=$(head -c 4 fuzz/artifacts/$fail | perl -nle 'print unpack "L<", $_')
  TC=$(($(($((NUM % NUM_INPUTS))+NUM_INPUTS)) % NUM_INPUTS))
  echo -e "$(pwd): \033[31;1mCHECKER FAILURE on testcase $TC: fuzz/failures/${TC}_$fail\033[;m"
  tail -c +5 fuzz/artifacts/$fail > fuzz/failures/${TC}_$fail
done



