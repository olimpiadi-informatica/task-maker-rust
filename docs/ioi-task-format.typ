#set heading(numbering: "1.")
#set par(justify: true)
#set text(size: 11pt, font: ("Latin Modern Roman",))
#show raw: set text(size: 1.25em, font: ("Latin Modern Mono"))
#show link: underline


#align(center, text(2em)[*IOI task format for `task-maker-rust`*])

This document specifies the structure of an IOI task in the format understood
by `task-maker-rust`, as well as supported features and recommended best
practices.

= General task folder structure

A task's folder should contain the following:
- A file named `task.yaml.orig`, containing various task settings.
- A `statement` folder, containing the source files to produce the statement.
- A `gen` folder, containing instruction to generate testcases and define
  subtasks.
- A `sol` folder, containing the reference solution of the task as well as
  auxiliary source files to be used during evaluation of the contestants'
  solutions. This folder is optional for communication tasks.
- Optionally, a `att` folder, specifying attachments that should be available
  for downloading in CMS.
- Optionally, a `check` folder, containing files to control how outputs of a
  solution are evaluated.
- `task-maker-rust` will generate a `input` and a `output` folder, containing
  the testdata for the task. It will also compile the checker, if any, into
  `check/checker`, and generate a `task.yaml` as well as PDF files for each
  statement.

== `task.yaml.orig`

This file serves a similar role to CMS's `task.yaml` file. However, many keys
can be omitted from the `task.yaml.orig` file, and they will set to good
default values (or deduced from other files in the task folder) if omitted.

The following are the most commonly set keys in this file:
- `title`: the title of this task, as will be shown in CMS.
  // TODO: allow overriding the title in the task statement template.
- `time_limit`: the maximum amount of time that a solution can run for, in
  seconds.
- `memory_limit`: the maximum amount of memory that a solution can use, in
  mebibytes.
- `score_precision`: the number of decimal digits to round scores for this task
  to (defaults to no rounding).
- `user_io`: set this value to `fifo_io` to have solutions in communication
  tasks communicate via FIFOs (by default they communicate via standard I/O).
  See #ref(<communication>) for further information on communication tasks.
// TODO: change this to default to FIFOs with a stub and stdio otherwise?
// TODO: IIRC stdio communication is broken, maybe fix it.
// TODO: what about TwoStep / OutputOnly / BatchAndOutput?

`task-maker-rust` will, upon execution, create a `task.yaml` file for CMS to
read; in particular, this file will contain scoring information, and will
default to having solutions read from and write to standard I/O.

#figure(
  ```yaml
title: Political Patricians
memory_limit: 512
time_limit: 2.0
user_io: fifo_io
score_precision: 2
```,
  caption: [The `task.yaml.orig` file for task "patricians" from WEOI 2025.],
)

= Solution folder
<solution>

In the solution folder, only one file is required for non-communication tasks:
the master solution, which will be used to generate "correct" outputs.

The master solution should be named `solution.<ext>`, where `<ext>` is one of
the extension of one of the languages understood by `task-maker-rust` (see
`task-maker-lang/src/languages/` for the list of languages). It is recommended
for this solution to be a symlink to a different solution with a more
descriptive name.

This folder may contain files called `grader.<ext>` or `stub.<ext>`, which will
not be considered as solutions. Instead, they will be compiled together with
the contestant's solution, in batch and communication tasks respectively.

A solution can contain assertions about the behaviour of that solution on
various subtasks. An assertion is a string starting by a string identifying a
supported check type, followed by a `:`, followed by a space-separated list of
subtask names (see #ref(<gen>)) or subtask name patterns (where patterns
contain `*` or `?` characters), followed by a newline.

The following checks are supported:
- `@check-accepted`: the solution achieves full score on these subtasks.
- `@check-partial-score`: the solution achieves a partial score (greater than
  $0$) on these subtasks.
- `@check-wrong-answer`: the solution produces a wrong answer on at least one
  testcase of this subtask.
- `@check-time-limit-exceeded`: the solution exceeds the time limit on at least
  one testcase of this subtask.
- `@check-wall-time-limit-exceeded`: the solution exceeds the wall time limit
  on at least one testcase of this subtask.
- `@check-runtime-error`: the solution fails due to runtime error on at least
  one testcase in this subtask.
- `@check-zero`: the solution achieves $0$ points on this subtask.

For example, the following lines

```cpp
// @check-accepted: st1 st2
// @check-zero: st3*
```

define checks that require the solution to get a full score on subtasks named
`st1` and `st2`, and to get a score of $0$ on all subtasks starting with the
`st3` prefix.

#figure(
  ```cpp
  // Full solution:
  // @check-accepted: examples twosets sizetwo nolimit

  // Partial solution, special-case:
  // @check-accepted: twosets
  // @check-wrong-answer: examples sizetwo nolimit

  // Partial solution, more queries:
  // @check-accepted: examples twosets
  // @check-partial-score: sizetwo nolimit
  ```,
  caption: [Some subtask checks from "patricians" from WEOI 2025.]
)

`task-maker-rust` warns when solutions do not contain checks. The tool
`add-solution-checks` in `task-maker-tools` can be used to add checks that
match the behaviour of solutions on the current machine.

= `gen` folder

This folder should contain at least the following three files:
- a `generator.<ext>` file, which is a program used to generate inputs for the
  task.
- a `GEN` file, which specifies the subtasks of this task (with their scores),
  as well as the arguments to pass to the generator.
- a `validator.<ext>` file, which is a program that verifies that the generated
  input files satisfy their constraints.

By convention, a file `constraints.yaml` is often also used, containing a
declarative description of _some_ of the constraints of the problem. This file
can be read by the validator and, if using the recommended way to produce the
statements, is also read during statement compilation.

== `generator.<ext>`

A generator should read its command line arguments and produce the testcase on
standard output.

Generators *should be deterministic*, i.e. they should produce the same output
given the same command line. Ideally, this output should also be consistent
across different versions of the programming language used. This means, for
example, that generators should not rely on the specific hash values of
objects.

#figure(
  ```python
#!/usr/bin/env python3

from sys import argv, exit, stderr
from random import randint, seed
from inspect import signature


def run(N):
    print(N)
    print(" ".join(str(randint(0, 10)) for _ in range(N)))


if __name__ == "__main__":
    num_args: int = len(signature(run).parameters) + 2
    if len(argv) != num_args:
        print(
            f"Got {len(argv)} parameters, expecting {num_args}", file=stderr)
        exit(1)

    def tryconv(x):
        for t in [int, float, str]:
            try:
                return t(x)
            except:
                pass

    *args, S = map(tryconv, argv[1:])
    seed(S)
    run(*args)
```,
  caption: [Example of a very simple Python generator (`generator.py`). Note
  that the code after the definition of the function `run` can be left
  unmodified for any generator that uses a fixed number of parameters, of which
  the last one is a RNG seed.]
)

== `validator.<ext>`

A validator takes on the command line the name of the file it should validate,
followed by the index of the subtask (starting from `1`, except when validating
samples from the `att` folder) that it should validate the testcase for.

Any non-zero return value declares that the file fails to validate.

It is recommended for the validator to read constraint values from the
`constraints.yaml` file.

#figure(
  ```python
#!/usr/bin/env python3

import sys
import yaml


def usage():
    print("Usage: %s file_input.txt [subtask_number]" %
          sys.argv[0], file=sys.stderr)
    sys.exit(1)


def run(f, st):
    for k, v in subtask[st].items():
        globals()[k] = v

    N = int(f.readline())
    assert 1 <= N <= MAXN

    vals = list(map(int, f.readline().split()))
    assert len(vals) == N


if __name__ == "__main__":
    if len(sys.argv) < 2:
        usage()

    if len(sys.argv) == 3:
        st = int(sys.argv[2])
    else:
        st = 0

    with open("constraints.yaml", "r") as constraints:
        constraints = yaml.safe_load(constraints)
        global_variables = globals()
        global_variables |= constraints

    f = open(sys.argv[1])
    sys.exit(run(f, st))
```,
  caption: [Example of a simple Python validator (`validator.py`) that reads
  constraints from `constraints.yaml`.]
)

== `constraints.yaml`
<constraints>

The file `constraints.yaml` should be a single YAML object, with each key
corresponding to the name of a constraint. It should also contain a sub-object
`subtask`, which is an array of objects with a subset of the keys; specified
keys have their limit replaced for that subtask, while omitted keys preserve
their global values.

The element of the array in position `i` corresponds to constraints for subtask
`i`. Note that, since subtask numbering is `1`-based, this array needs a
placeholder object in the beginning.

#figure(
  ```yaml
MAXN: 1000000
MAXM: 100000
MAXA: 1000000

subtask:
  - {}                         # placeholder
  - {}                         # Subtask 1: samples
  - {MAXN:10}                  # Subtask 2
  - {MAXA:100}                 # Subtask 3
  - {MAXN:10000, MAXA:10000}   # Subtask 4
  - {}                         # Subtask 5
```,
  caption: [Example `constraints.yaml` file.]
)

== `GEN`
<gen>

The `GEN` file specifies testcases and subtasks.
It can contain three kinds of lines:
- comment lines are empty lines, or lines that start with `# ` (with a space!)
  followed by any text.
- command lines are lines that start with `#` *not followed by a space*. A list
  of commands is given below.
- Any other line is interpreted as a testcase definition, and it is passed as
  command line arguments to the generator.

Supported commands:
- `#ST: <score>`: defines a new subtask, worth a number of points given by
  `<score>`.
- `#STNAME: string-without-spaces`: defines the name of the current subtask.
- `#STDEP: list-of-space-separated-strings`: declares the specified subtasks as
  a *dependency* of the current subtask. This means that all testcases defined
  in those subtasks will be included in the current subtask. Note that this
  works even if the dependencies of a subtask are defined later in the `GEN`,
  and even if the dependency graph contains cycles.
- `#COPY: path-relative-to-task-directory`: declares a new testcase, obtained
  by copying the specified file. It is recommended to use this command to
  create a "examples" subtask.
  // TODO: should we have a #SAMPLES command that copies *all* the samples?

#figure(
  ```python
# N SEED

#ST: 0
#STNAME: examples
#COPY: statement/problemname.input0.txt
#COPY: statement/problemname.input1.txt

#ST: 10
#STNAME: N<=10
10 0
10 1
10 2

#ST: 20
#STNAME: N<=100
#STDEP: N<=10
100 3
100 4

#ST: 20
#STNAME: fancy-name
100000 5
100000 6

#ST: 50
#STNAME: nolimit
#STDEP: fancy-name N<=100
1000000 7
```,
  caption: [Example `GEN` file.]
)

= `statement` folder

The statement folder should contain:
- one or more statement files, named `<language>.typ`,
- sample inputs and outputs (named `<taskname>.(input|output)<i>.txt`, $0$-based) or sample interactions
  (`<taskname>.interaction<i>.txt`),
- any number of auxiliary files.

It is recommended to symlink the Typst statement template available at
#link("https://github.com/olimpiadi-informatica/typst-statement-template")
and the logo of the contest (`logo.png`) as auxiliary files.

// TODO: make the statement template a typst package to simplify this part.

We recommend using the `cetz` typst package for drawing figures. Given the
easy-to-use scripting capabilities of Typst, it is often easy to produce
reasonable looking figures directly with `cetz`, and this results in higher
visual quality compared to using external programs.

In some cases, it is even feasible to have Typst parse input files and
automatically produce images for samples from the sample's contents.

Note that, while `task-maker-rust` will compile Typst statements as part of
task preparation, you can compile a statement manually as follows:

```bash
typst compile statement/<language>.typ --root .
```

Doing so will result in some missing information in the produced statement
(such as the contest name), but is otherwise equivalent. However, this command
will read `task.yaml`, so it may be necessary to run `task-maker-rust` once
before running `typst`.

// TODO: figure out if we can remove this limitation.

To use the template, one must first `#import "template.typ": *`; then, the main
statement must be enclosed in a `statement` command. Optionally, an editorial
can be added in a `editorial` command; it will only be rendered when running
```bash
task-maker-tools booklet --booklet-solutions
```

or by manually passing `--input show_solutions=true` to Typst.

== Translated section names

By specifying the language of the statement (i.e. `#set text(lang: "en")` at
the start of the file), the statement template can provide already-translated
section names for supported languages. Supported sections are `implementation`,
`samplegrader`, `constraints`, `scoring`, `explanation`.

== `note` and `warn`

The statement template comes with two functions, `note` and `warn`, which
create boxes meant to highlight important information for the contestant.

== `signatures`

This command allows to specify the signature of the functions contestants
should implement on problems that have a grader or a stub (see
#ref(<solution>)). It takes as input a list of pairs
`(language code, signature)`.

== `constraint`

This object contains the global constraints of the task, as specified in
`constraints.yaml` (see #ref(<constraints>)).

#figure(
  ```typst
#constraints
- $2 <= N <= constraint.MAXN$.
- $1 <= M <= constraint.MAXM$.
- $0 <= A_i <= constraint.MAXA$.
- The sum of all the elements in the array is even.
```,
  caption: [Example of a "constraints" section]
)

== `subtasks`

The `subtasks` function takes as input an array of functions. Each of the
functions takes as input the subtask-specific constraints obtained by
`constraints.yaml` (see #ref(<constraints>)). Scoring information is extracted
automatically from `GEN` (see #ref(<gen>)). The optional argument `index_start`
specifies the number of the first subtask; this can be used to have the sample
subtask be subtask $0$, as CMS and IOI do.

#figure(
  ```typst
#subtasks((
  subtask => [Sample test cases.],
  subtask => [$N <= subtask.MAXN$.],
  subtask => [$A_i <= subtask.MAXA$ for $i = 0, .., N-1$.],
  subtask => [$N<=subtask.MAXN$ and $A_i<=subtask.MAXA$ for $i=0,..,N-1$.],
  subtask => [No additional constraint.],
), index_start: 0)
```,
  caption: [Example of usage of the `subtasks` function.]
)

== `examples` and `examples-interactive`

These functions will render the specified number of examples input/output pairs
or example interactions.

For examples, a side-by-side table with the contents of each pair of
input/output examples will be shown.

For example interactions, the interaction file will be read. Lines starting
with `<` will be shown as function calls or input coming from the grader, while
lines starting with `>` will be shown as function calls or output done by the
solution.

= `att` folder

Any file in this folder is given to the contestants as an attachment. Typically,
we suggest including at least the following:
- A solution template (we recommend for this to be symlinked into the `sol`
  folder, so that `task-maker-rust` will check that the template compiles and
  behaves reasonably).
- If the problem uses a grader or a stub, a sample grader for the contestants
  to use for local testing. This *may* be a symlink to the official grader, but
  does not need to be.
- All the input/output sample pairs on batch tasks, or sample inputs
  corresponding to sample interactions in communication tasks.

= `check` folder

The `check` folder can contain either a `checker.<ext>` file, or a
`manager.<ext>` file, or be empty.

If it contains `manager.<ext>`, the task is interpreted as a communication
task, for which details are given in #ref(<communication>).

The `checker.<ext>` file will be compiled by `task-maker-rust` (if necessary),
and gets executed both by `task-maker-rust` and by CMS with three command line
arguments, in order:
- the input file,
- the file containing the output of the master solution,
- the file containing the output of the contestant's solution.

It should write on standard output the score of the testcase, as a float
between `0.0` and `1.0`, and on standard error a message for the contestant.
The special messages `translate:success`, `translate:wrong` and `translate:partial`
are shown by CMS as translated strings in the UI.

A checker should *never* return a non-zero error code or crash, as CMS will
mark evaluation as failed in that case.

To aid in ensuring that this does not happen, `task-maker-rust` has the
`task-maker-tools fuzz-checker` tool, which will use a fuzzing engine to try to
crash the checker. This tool works significantly better if there is no global
state in the checker, so try to avoid global variables.

#figure(
  ```cpp
#include <iostream>
#include <fstream>
#include <vector>

[[noreturn]] void grade(float score, const char* msg) {
    std::cout << score;
    std::cerr << msg;
    exit(0);
}

int main(int argc, char* argv[]) {
    std::ifstream input(argv[1]);
    std::ifstream master_output(argv[2]);
    std::ifstream contestant_output(argv[3]);
    grade(0, "translate:wrong");
}
```,
  caption: [A checker that always gives $0$ points.]
)

= Communication tasks
<communication>

A communication task is identified by the presence of a `check/manager.<ext>`
file. This file is very similar to a checker in batch tasks, with one notable
difference: it communicates with the solution in an interactive way.

To achieve this, it receives as input on the command line the paths to two
FIFOs, used -- in order -- to write to the contestant's solution and to read
from the contestant's solution.

The manager is also responsible for reading the input file from standard input.

The contestant's solution will either communicate with standard I/O to the
manager (if `user_io: std_io` is set in `task.yaml`), or will also receive
FIFOs if `user_io: fifo_io`; this last mode is only recommended when using a
stub.
// TODO: std_io seems broken in tmr.

Note that it is very easy to deadlock execution in communication tasks. You
should take care to ensure that FIFOs are opened in the correct order, and that
all writes are flushed.

// TODO: fuzzer for communication?

Below we provide two skeletons for `manager.cpp` and `stub.cpp` that handle
FIFOs correctly.

== `manager.cpp`

```cpp
#include <csignal>
#include <cstdio>
#include <unistd.h>

FILE *to_contestant, *from_contestant;

[[noreturn]] void grade(float score, const char *text) {
  printf("%f\n", score);
  fprintf(stderr, "%s\n", text);

  // You may want to signal to the contestant that they should terminate here.

  fclose(to_contestant);
  fclose(from_contestant);

  exit(0);
}

int main(int argc, char *argv[]) {
  signal(SIGPIPE, SIG_IGN);
  to_contestant = fopen(argv[1], "w");
  from_contestant = fopen(argv[2], "r");

  // This should probably be replaced.
  grade(0, "translate:wrong");
}
```

== `stub.cpp`

```cpp
#include <cassert>
#include <cstdio>
#include <cstdlib>

static FILE *to_manager, *from_manager;

int main(int argc, char **argv) {
  from_manager = fopen(argv[2], "r");
  to_manager = fopen(argv[1], "w");
  // Call into the contestant's solution here.
}
```
