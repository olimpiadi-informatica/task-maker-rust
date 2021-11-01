//! The new cmsMake!
//!
//! [![asciicast](https://asciinema.org/a/301849.svg)](https://asciinema.org/a/301849)
//!
//! # Installation
//! For **Ubuntu** and **Debian** users you can find the `.deb` file in the [Releases](https://github.com/edomora97/task-maker-rust/releases) page.
//! Install the package using `sudo dpkg -i the_file.deb` and it's dependencies (if you need to) with `sudo apt install -f`.
//! There is a good chance that you have already all the dependencies already installed.
//!
//! For **ArchLinux** users you can find the packages in the AUR: [`task-maker-rust`](https://aur.archlinux.org/packages/task-maker-rust) (the stable release)
//! and [`task-maker-rust-git`](https://aur.archlinux.org/packages/task-maker-rust-git) (the version based on `master`).
//!
//! For **MacOS Catalina** users you can find the pre-built bottle in the [Releases](https://github.com/edomora97/task-maker-rust/releases) page.
//! You can install it using `brew install task-maker-rust--*.catalina.bottle.tar.gz`.
//!
//! For the other operating systems the recommended way to use task-maker-rust is the following:
//!
//! - Install the latest stable rust version (and cargo). For example using [rustup](https://rustup.rs/)
//! - Install the system dependencies: `libseccomp` or `libseccomp-dev` on Ubuntu
//! - Clone this repo: `git clone https://github.com/edomora97/task-maker-rust`
//! - Build task-maker: `cargo build --release`
//!
//! The executable should be located at `target/release/task-maker`.
//! Due to limitations of cargo (the build system), `cargo install` should not be used since it
//! doesn't copy some required files. For the same reason you should not delete or move the cloned
//! repository after the build. If you need a package for your operating system/distro open an issue
//! please!
//!
//! The supported operating systems are Linux (with libseccomp support), OSX and Windows under WSL2.
//! It should be possible to build task-maker using musl but it may be hard to link libseccomp!
//!
//! # Usage
//!
//! <details>
//! <summary>Simple local usage</summary>
//!
//! Run `task-maker-rust` in the task folder to compile and run everything.
//!
//! Specifying no option all the caches are active, the next executions will be very fast, actually doing only what's needed.
//! </details>
//!
//! <details>
//! <summary>Disable cache</summary>
//!
//! If you really want to repeat the execution of something provide the `--no-cache` option:
//!
//! ```bash
//! task-maker-rust --no-cache
//! ```
//!
//! Without any options `--no-cache` won't use any caches.
//!
//! If you want, for example, just redo the evaluations (maybe for retrying the timings), use
//! `--no-cache=evaluation`. The available options for `--no-cache` can be found with `--help`.
//!
//! </details>
//!
//! <details>
//! <summary>Test only a subset of solutions</summary>
//!
//! Sometimes you only want to test only some solutions, speeding up the compilation and cleaning a
//! bit the output:
//!
//! ```bash
//! task-maker-rust sol1.cpp sol2.py
//! ```
//!
//! Note that you may or may not specify the folder of the solution (sol/ or solution/). You can
//! also specify only the prefix of the name of the solutions you want to check.
//!
//! </details>
//!
//! <details>
//! <summary>Using different task directory</summary>
//!
//! By default the task in the current directory is executed, if you want to change the task without
//! `cd`-ing away:
//!
//! ```bash
//! task-maker-rust --task-dir ~/tasks/poldo
//! ```
//!
//! </details>
//!
//! <details>
//! <summary>Extracting executable files</summary>
//!
//! All the compiled files are kept in an internal folder but if you want to use them, for example
//! to debug a solution, passing `--copy-exe` all the useful files are copied to the `bin/` folder
//! inside the task directory.
//!
//! ```bash
//! task-maker-rust --copy-exe
//! ```
//!
//! </details>
//!
//! <details>
//! <summary>Statement</summary>
//!
//! If you don't want to build the statement files (and the booklet) just pass `--no-statement`.
//!
//! ```bash
//! task-maker-rust --no-statement
//! ```
//!
//! If you want just to build the statement you can use:
//!
//! ```bash
//! task-maker-tools booklet
//! ```
//!
//! This tool can also be used to build the contest's booklet.
//!
//! </details>
//!
//! <details>
//! <summary> Clean the task directory</summary>
//!
//! If you want to clean everything, for example after the contest, simply run:
//! ```bash
//! task-maker-tools clear
//! ```
//!
//! This will remove the files that can be regenerated from the task directory. Note that the
//! internal cache is not pruned by this command.
//!
//! </details>
//!
//! <details>
//! <summary>Remote evaluation</summary>
//!
//! On a server (a machine accessible from clients and workers) run
//!
//! ```bash
//! task-maker-tools server
//! ```
//!
//! This will start `task-maker` in server mode, listening for connections from clients and workers
//! respectively on port 27182 and 27183.
//!
//! Then on the worker machines start a worker with
//! ```bash
//! task-maker-tools worker server_addr num
//! ```
//!
//! This will start a worker on that machine (**using a single core**), connecting to the server
//! and executing the jobs the server assigns. The `num` parameter can be used to distinguish
//! between multiple workers in the same machine.
//!
//! For running a remote computation on your machine just add the `--evaluate-on` option, like:
//! ```bash
//! task-maker-rust --evaluate-on server_addr
//! ```
//!
//! </details>
//!
//! ### Using docker
//!
//! You can easily spawn a task-maker server and a set of workers in your local machine without having to install all the compilers.
//!
//! ```bash
//! docker run --rm -it \
//!     --name task-maker \
//!     -p 27183:27183 \
//!     -p 27182:27182 \
//!     --privileged \
//!     edomora97/task-maker-rust:latest
//! ```
//!
//! Then you can use task-maker locally adding `--evaluate-on localhost`.
//!
//! `--privileged` is required to run the worker sandboxes.

#![allow(clippy::borrowed_box)]
#![allow(clippy::new_without_default)]
#![allow(clippy::module_inception)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use structopt::StructOpt;

use task_maker_rust::opt;
use task_maker_rust::{main_local, main_sandbox};

fn main() {
    let mut opt = opt::Opt::from_args();
    opt.enable_log();

    // internal API: run in sandbox mode if `--sandbox` is provided
    if opt.sandbox {
        main_sandbox();
        return;
    }

    main_local(opt);
}
