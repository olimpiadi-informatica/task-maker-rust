[![Build Status](https://travis-ci.org/edomora97/task-maker-rust.svg?branch=master)](https://travis-ci.org/edomora97/task-maker-rust)

# task-maker-rust

The new cmsMake!

[![asciicast](https://asciinema.org/a/301849.svg)](https://asciinema.org/a/301849)

## Installation
For **Ubuntu** and **Debian** users you can find the `.deb` file in the [Releases](https://github.com/edomora97/task-maker-rust/releases) page.
Install the package using `sudo dpkg -i the_file.deb` and it's dependencies (if you need to) with `sudo apt install -f`.
There is a good chance that you have already all the dependencies already installed.

For **ArchLinux** users you can find the packages in the AUR: [`task-maker-rust`](https://aur.archlinux.org/packages/task-maker-rust) (the stable release)
and [`task-maker-rust-git`](https://aur.archlinux.org/packages/task-maker-rust-git) (the version based on `master`).

For the other operating systems the recommended way to use task-maker-rust is the following:

- Install the latest stable rust version (and cargo). For example using [rustup](https://rustup.rs/)
- Install the system dependencies: `libseccomp` or `libseccomp-dev` on Ubuntu
- Clone this repo: `git clone https://github.com/edomora97/task-maker-rust`
- Build task-maker: `cargo build --release`

The executable should be located at `target/release/task-maker`.
Due to limitations of cargo (the build system), `cargo install` should not be used since it
doesn't copy some required files. For the same reason you should not delete or move the cloned
repository after the build. If you need a package for your operating system/distro open an issue
please!

The supported operating systems are Linux (with libseccomp support), OSX and Windows under WSL2.
It should be possible to build task-maker using musl but it may be hard to link libseccomp!

## Usage

### Simple local usage
Run `task-maker-rust` in the task folder to compile and run everything.

Specifying no option all the caches are active, the next executions will be very fast, actually doing only what's needed.

### Disable cache
If you really want to repeat the execution of something provide the `--no-cache`
option:
```bash
task-maker-rust --no-cache
```

Without any options `--no-cache` won't use any caches.

If you want, for example, just redo the evaluations (maybe for retrying the timings), use `--no-cache=evaluation`.
The available options for `--no-cache` can be found with `--help`.

### Test only a subset of solutions
Sometimes you only want to test only some solutions, speeding up the compilation and cleaning a bit the output:
```bash
task-maker-rust sol1.cpp sol2.py
```
Note that you may or may not specify the folder of the solution (sol/ or solution/).
You can also specify only the prefix of the name of the solutions you want to check.

### Using different task directory
By default the task in the current directory is executed, if you want to change the task without `cd`-ing away:
```bash
task-maker-rust --task-dir ~/tasks/poldo
```

### Extracting executable files
All the compiled files are kept in an internal folder but if you want to use them, for example to debug a solution, passing `--copy-exe` all the useful files are copied to the `bin/` folder inside the task directory.
```bash
task-maker-rust --copy-exe
```

### Do not build the statement
If you don't want to build the statement files (and the booklet) just pass `--no-statement`.
```bash
task-maker-rust --no-statement
```

### Clean the task directory
If you want to clean everything, for example after the contest, simply run:
```bash
task-maker-rust --clean
```
This will remove the files that can be regenerated from the task directory.
Note that the internal cache is not pruned by this command.

### Remote evaluation
On a server (a machine accessible from clients and workers) run
```bash
task-maker-rust --server
```
This will start `task-maker` in server mode, listening for connections from clients and workers
respectively on port 27182 and 27183.

Then on the worker machines start a worker each with
```bash
task-maker-rust --worker ip_of_the_server:27183
```
This will start a worker on that machine (using all the cores unless specified), connecting to
the server and executing the jobs the server assigns.

For running a remote computation on your machine just add the `--evaluate-on` option, like:
```bash
task-maker-rust --evaluate-on ip_of_the_server:27182
```

License: MPL-2.0
