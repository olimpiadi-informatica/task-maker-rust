[![Build Status](https://travis-ci.org/edomora97/task-maker-rust.svg?branch=master)](https://travis-ci.org/edomora97/task-maker-rust)

# task-maker-rust

The new cmsMake!

## Installation
The official way for installing `task-maker-rust` is not defined yet.

For now you should clone the repo and run `cargo build --release`.
The executable should be located at `target/release/task-maker`.

You may need to install `libseccomp` (or maybe `libseccomp-dev` on Ubuntu) for compiling task-maker.

## Usage

### Simple local usage
Run `task-maker` in the task folder to compile and run everything.

Specifying no option all the caches are active, the next executions will be very fast, actually doing only what's needed.

### Disable cache
If you really want to repeat the execution of something provide the `--no-cache`
option:
```bash
task-maker --no-cache
```

Without any options `--no-cache` won't use any caches.

If you want, for example, just redo the evaluations (maybe for retrying the timings), use `--no-cache=evaluation`.
The available options for `--no-cache` can be found with `--help`.

### Test only a subset of solutions
Sometimes you only want to test only some solutions, speeding up the compilation and cleaning a bit the output:
```bash
task-maker sol1.cpp sol2.py
```
Note that you may or may not specify the folder of the solution (sol/ or solution/).
You can also specify only the prefix of the name of the solutions you want to check.

### Using different task directory
By default the task in the current directory is executed, if you want to change the task without `cd`-ing away:
```bash
task-maker --task-dir ~/tasks/poldo
```

### Extracting executable files
All the compiled files are kept in an internal folder but if you want to use them, for example to debug a solution, passing `--copy-exe` all the useful files are copied to the `bin/` folder inside the task directory.
```bash
task-maker --copy-exe
```

### Do not build the statement
If you don't want to build the statement files (and the booklet) just pass `--no-statement`.
```bash
task-maker --no-statement
```

### Clean the task directory
If you want to clean everything, for example after the contest, simply run:
```bash
task-maker --clean
```
This will remove the files that can be regenerated from the task directory.
Note that the internal cache is not pruned by this command.

### Remote evaluation
On a server (a machine accessible from clients and workers) run
```bash
task-maker --server
```
This will start `task-maker` in server mode, listening for connections from clients and workers
respectively on port 27182 and 27183.

Then on the worker machines start a worker each with
```bash
task-maker --worker ip_of_the_server:27183
```
This will start a worker on that machine (using all the cores unless specified), connecting to
the server and executing the jobs the server assigns.

For running a remote computation on your machine just add the `--evaluate-on` option, like:
```bash
task-maker --evaluate-on ip_of_the_server:27182
```

License: MPL-2.0
