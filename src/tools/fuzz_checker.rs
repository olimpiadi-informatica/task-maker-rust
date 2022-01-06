use std::io::{Read, Write};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, bail, Context, Error};
use regex::Regex;
use serde::Deserialize;

use task_maker_format::ioi::{Checker, TaskType};
use task_maker_format::ui::{StdoutPrinter, UIType, RED};
use task_maker_format::{cwrite, EvaluationConfig, TaskFormat};

use crate::context::RuntimeContext;
use crate::tools::opt::FuzzCheckerOpt;

const CHECKER_HEADER: &[u8] = include_bytes!("./fuzz_checker/checker_header.h");
const FUZZER: &[u8] = include_bytes!("./fuzz_checker/fuzzer.cpp");

#[derive(Debug)]
struct FuzzData {
    /// Base directory of the task.
    task_dir: PathBuf,
    /// Path to the checker source file.
    checker_source: PathBuf,
    /// Paths to the initial output files, in the same order as in the task.
    initial_output_files: Vec<PathBuf>,
    /// Fuzzer options.
    opt: FuzzCheckerOpt,
}

pub fn main_fuzz_checker(opt: FuzzCheckerOpt) -> Result<(), Error> {
    let task_format = opt
        .find_task
        .find_task(&Default::default())
        .context("Failed to locate the task")?;

    let task = if let TaskFormat::IOI(task) = &task_format {
        task
    } else {
        bail!("The fuzz-checker tool only supports IOI-tasks for now");
    };

    let task_type = if let TaskType::Batch(task_type) = &task.task_type {
        task_type
    } else {
        bail!("Only Batch tasks are supported");
    };

    let checker = if let Checker::Custom(checker) = &task_type.checker {
        checker
    } else {
        bail!("Only tasks with a checker are supported");
    };

    let language = checker.language();
    if language.name() != "C++" {
        bail!("Only C++ checkers are supported");
    }

    let num_testcases: usize = task.subtasks.values().map(|st| st.testcases.len()).sum();

    let task_dir = task.path.clone();
    let fuzz_data = FuzzData {
        checker_source: checker.path.clone(),
        initial_output_files: (0..num_testcases)
            .map(|i| task_dir.join(format!("output/output{}.txt", i)))
            .collect(),
        task_dir,
        opt,
    };

    trace!("Fuzz data: {:#?}", fuzz_data);

    if fuzz_data.opt.no_build {
        for output in &fuzz_data.initial_output_files {
            if !output.exists() {
                bail!("The output files haven't been generated, please run task-maker");
            }
        }
    } else {
        info!("Running task-maker for building the output files");

        let eval_config = EvaluationConfig {
            solution_filter: vec!["do not evaluate the solutions!!".into()],
            no_statement: true,
            ..Default::default()
        };

        // setup the configuration and the evaluation metadata
        let context = RuntimeContext::new(task_format, &fuzz_data.opt.execution, |task, eval| {
            // build the DAG for the task
            task.build_dag(eval, &eval_config)
                .context("Cannot build the task DAG")
        })?;

        // start the execution
        let executor =
            context.connect_executor(&fuzz_data.opt.execution, &fuzz_data.opt.storage)?;
        let executor = executor.start_ui(&UIType::Silent, |_, _| {})?;
        executor.execute()?;
    }

    let fuzz_dir = fuzz_data.task_dir.join(&fuzz_data.opt.fuzz_dir);
    if !fuzz_dir.exists() {
        info!("Creating fuzz directory at {}", fuzz_dir.display());
        std::fs::create_dir_all(&fuzz_dir).with_context(|| {
            anyhow!("Failed to created fuzz directory at {}", fuzz_dir.display())
        })?;
    }

    write_initial_corpus(&fuzz_dir, &fuzz_data)?;
    let checker_source = write_checker_source(&fuzz_dir, &fuzz_data)?;
    let fuzz_binary = compile_fuzzer(&fuzz_dir, &fuzz_data, &checker_source)?;
    let artifacts = run_fuzzer(&fuzz_dir, &fuzz_data, &fuzz_binary)?;
    organize_failures(&fuzz_dir, &fuzz_data, &artifacts)?;

    Ok(())
}

fn write_initial_corpus(fuzz_dir: &Path, data: &FuzzData) -> Result<(), Error> {
    info!("Creating initial corpus");
    let initial_corpus_dir = fuzz_dir.join("initial_corpus");
    if !initial_corpus_dir.exists() {
        std::fs::create_dir(&initial_corpus_dir).with_context(|| {
            anyhow!(
                "Failed to create initial_corpus directory at {}",
                initial_corpus_dir.display()
            )
        })?;
    }

    for (index, output) in data.initial_output_files.iter().enumerate() {
        let path = initial_corpus_dir.join(format!("{}.txt", index));
        if path.exists() {
            debug!("Removing old corpus at {}", path.display());
            std::fs::remove_file(&path)
                .with_context(|| anyhow!("Failed to remove {}", path.display()))?;
        }

        let mut file = std::fs::File::create(&path)
            .with_context(|| anyhow!("Failed to create {}", path.display()))?;

        // write the testcase number as the first 4 bytes of the output file
        let index_bytes = (index as u32).to_le_bytes();
        file.write_all(&index_bytes)
            .with_context(|| anyhow!("Failed to write index to {}", path.display()))?;

        let output_content = std::fs::read(output)
            .with_context(|| anyhow!("Failed to read {}", output.display()))?;

        // write the rest of the output file
        file.write_all(&output_content).with_context(|| {
            anyhow!(
                "Failed to write {} bytes of output at {}",
                output_content.len(),
                path.display()
            )
        })?;
    }

    Ok(())
}

fn write_checker_source(fuzz_dir: &Path, data: &FuzzData) -> Result<Vec<PathBuf>, Error> {
    let sources_dir = fuzz_dir.join("sources");
    std::fs::create_dir_all(&sources_dir)
        .with_context(|| anyhow!("Failed to create sources dir at {}", sources_dir.display()))?;
    let path = sources_dir.join("checker-patched.cpp");
    info!("Writing patched checker source at {}", path.display());
    let mut checker_file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&path)
        .with_context(|| anyhow!("Failed to create {}", path.display()))?;

    checker_file
        .write_all(CHECKER_HEADER)
        .with_context(|| anyhow!("Failed to write checker header at {}", path.display()))?;

    let mut checker_content = std::fs::read_to_string(&data.checker_source).with_context(|| {
        anyhow!(
            "Failed to read checker source at {}",
            data.checker_source.display()
        )
    })?;

    checker_sanity_check(data, &checker_content)?;
    patch_checker(&mut checker_content).with_context(|| anyhow!("Failed to patch checker"))?;

    checker_file
        .write_all(checker_content.as_bytes())
        .with_context(|| {
            anyhow!(
                "Failed to write {} bytes of checker at {}",
                checker_content.len(),
                path.display()
            )
        })?;

    let fuzzer = sources_dir.join("fuzzer.cpp");
    info!("Writing fuzzer source at {}", fuzzer.display());
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&fuzzer)
        .with_context(|| anyhow!("Failed to create {}", fuzzer.display()))?
        .write_all(FUZZER)
        .with_context(|| {
            anyhow!(
                "Failed to write {} bytes of fuzzer at {}",
                FUZZER.len(),
                fuzzer.display()
            )
        })?;

    Ok(vec![path, fuzzer])
}

fn checker_sanity_check(data: &FuzzData, checker_content: &str) -> Result<(), Error> {
    let mut command = std::process::Command::new("ctags");
    command.arg("--output-format=json");
    command.arg("--c-kinds=v");
    command.arg("--extras=-F");
    command.arg(&data.checker_source);
    command.stdout(Stdio::piped());
    let output = match command.output() {
        Ok(output) => output,
        Err(e) => {
            warn!(
                "Failed to execute ctags, you may need to installed it: {:?}",
                e
            );
            return Ok(());
        }
    };
    if !output.status.success() {
        warn!(
            "ctags failed to analyze the checker source (exit code {:?})",
            output.status.code()
        );
    }

    // Example output line:
    //     {"_type": "tag", "name": "H", "path": "cor/correttore.cpp", "pattern": "/^int H[MAXN], rep[MAXN], lep[MAXN], nxt[MAXN], prv[MAXN];$/", "typeref": "typename:int[]", "kind": "variable"}
    //     {"_type": "tag", "name": "MAXN", "path": "cor/correttore.cpp", "pattern": "/^const int MAXN = 2000005;$/", "typeref": "typename:const int", "kind": "variable"}
    #[derive(Deserialize)]
    struct OutputLine {
        name: String,
        typeref: String,
        kind: String,
    }

    let output = String::from_utf8_lossy(&output.stdout);
    for line in output.as_ref().lines() {
        let line = match serde_json::from_str::<OutputLine>(line) {
            Ok(line) => line,
            Err(e) => {
                warn!(
                    "Failed to deserialize line from ctags: {:?} (line was {})",
                    e, line
                );
                continue;
            }
        };
        // we are only interested in global variables
        if line.kind != "variable" {
            continue;
        }
        // constants are ok to be global
        if line.typeref.starts_with("typename:const ") {
            continue;
        }
        // we found a global variable!
        error!(
            "Global variable found! '{}' looks like a global variable and it will probably interfere with the fuzzing process", line.name
        )
    }

    let re = Regex::new(r"\b(static\s[^=;]*)").expect("bad regex");
    for cap in re.captures_iter(checker_content) {
        error!("Static variable found! '{}' looks like a static variable and it will probably interfere with the fuzzing process", &cap[1]);
    }

    Ok(())
}

fn patch_checker(source: &mut String) -> Result<(), Error> {
    info!("Patching checker source file");
    *source = source.replace("std::exit", "exit");
    Ok(())
}

fn compile_fuzzer(fuzz_dir: &Path, data: &FuzzData, sources: &[PathBuf]) -> Result<PathBuf, Error> {
    let fuzzer_dir = fuzz_dir.join("fuzzer");
    std::fs::create_dir_all(&fuzzer_dir)
        .with_context(|| anyhow!("Failed to create fuzzer dir at {}", fuzzer_dir.display()))?;
    let target = fuzzer_dir.join("fuzzer");
    info!("Compiling {} with clang++", target.display());

    let mut command = std::process::Command::new("clang++");
    for source in sources {
        command.arg(source);
    }
    command.arg("-o");
    command.arg(&target);

    command.arg(format!("-DNUM_INPUTS={}", data.initial_output_files.len()));
    command.arg(format!("-DFUZZ_DIRECTORY=\"{}\"", fuzz_dir.display()));
    command.arg(format!("-DTASK_DIRECTORY=\"{}\"", data.task_dir.display()));

    let mut sanitizers = "-fsanitize=fuzzer".to_string();
    if !data.opt.sanitizers.is_empty() {
        sanitizers += ",";
        sanitizers += &data.opt.sanitizers;
    }
    command.arg(sanitizers);

    if data.opt.extra_args.is_empty() {
        debug!("Adding -O2 and -g since no extra argument has been specified");
        command.arg("-O2");
        command.arg("-g");
    } else {
        for arg in &data.opt.extra_args {
            command.arg(arg);
        }
    }

    info!("Compiling with: {:?}", command);
    let status = command
        .status()
        .with_context(|| anyhow!("Failed to start the checker compilation with {:?}", command))?;
    if !status.success() {
        bail!("Checker compilation failed (exit code {:?})", status.code());
    }

    Ok(target)
}

fn run_fuzzer(fuzz_dir: &Path, data: &FuzzData, fuzzer: &Path) -> Result<Vec<PathBuf>, Error> {
    let artifacts = fuzz_dir.join("artifacts");
    if artifacts.exists() {
        warn!(
            "Removing existing artifacts directory at {}",
            artifacts.display()
        );
        std::fs::remove_dir_all(&artifacts).with_context(|| {
            anyhow!("Failed to remove artifacts dir at {}", artifacts.display())
        })?;
    }
    std::fs::create_dir(&artifacts)
        .with_context(|| anyhow!("Failed to create artifacts dir at {}", artifacts.display()))?;

    let mut command = std::process::Command::new(fuzzer);
    let jobs = if let Some(jobs) = data.opt.jobs {
        jobs
    } else {
        num_cpus::get()
    };
    command.arg(format!("-fork={}", jobs));
    command.arg(format!("-timeout={}", data.opt.checker_timeout));
    command.arg(format!("-max_total_time={}", data.opt.max_time));
    command.arg(fuzz_dir.join("initial_corpus"));
    command.arg(format!("-artifact_prefix={}/", artifacts.display()));
    if data.opt.quiet {
        let stdout = fuzz_dir.join("fuzzer/stdout.txt");
        debug!("Redirecting stdout to {}", stdout.display());
        let stdout_file = std::fs::File::create(&stdout)
            .with_context(|| anyhow!("Failed to create stdout at {}", stdout.display()))?;
        // SAFETY: the file is constructed and dropped here, it is not shared anywhere
        let stdout = unsafe { Stdio::from_raw_fd(stdout_file.into_raw_fd()) };
        command.stdout(stdout);

        let stderr = fuzz_dir.join("fuzzer/stderr.txt");
        debug!("Redirecting stderr to {}", stderr.display());
        let stderr_file = std::fs::File::create(&stderr)
            .with_context(|| anyhow!("Failed to create stderr at {}", stderr.display()))?;
        let stderr = unsafe { Stdio::from_raw_fd(stderr_file.into_raw_fd()) };
        command.stderr(stderr);
    };

    info!("Running fuzzer with: {:?}", command);
    let status = command
        .status()
        .with_context(|| anyhow!("Failed to start the fuzzer with {:?}", command))?;
    if !status.success() {
        warn!("Fuzzer failed (exit code {:?})", status.code());
    }

    let paths = std::fs::read_dir(&artifacts)
        .with_context(|| {
            anyhow!(
                "Failed to list artifacts directory content at {}",
                artifacts.display()
            )
        })?
        .filter_map(|path| path.ok().map(|p| p.path()))
        .collect();
    Ok(paths)
}

fn organize_failures(fuzz_dir: &Path, data: &FuzzData, artifacts: &[PathBuf]) -> Result<(), Error> {
    if artifacts.is_empty() {
        info!("No failure found!");
        return Ok(());
    }
    info!("Reorganizing failure files");
    let failures = fuzz_dir.join("failures");
    if failures.exists() {
        warn!(
            "Removing existing failures directory at {}",
            failures.display()
        );
        std::fs::remove_dir_all(&failures)
            .with_context(|| anyhow!("Failed to remove failure dir at {}", failures.display()))?;
    }
    std::fs::create_dir(&failures)
        .with_context(|| anyhow!("Failed to create failures dir at {}", failures.display()))?;

    let mut printer = StdoutPrinter::default();

    for (artifact_id, artifact) in artifacts.iter().enumerate() {
        let mut file = std::fs::File::open(artifact)
            .with_context(|| anyhow!("Failed to open artifact at {}", artifact.display()))?;

        // read the input id from the first 4 bytes of the output file
        let mut id = [0u8; 4];
        file.read_exact(&mut id)
            .with_context(|| anyhow!("Failed to read testcase id from {}", artifact.display()))?;
        let id = u32::from_le_bytes(id) % data.initial_output_files.len() as u32;

        // read the actual output file
        let mut output = vec![];
        file.read_to_end(&mut output).with_context(|| {
            anyhow!(
                "Failed to read the output file from the artifact at {}",
                artifact.display()
            )
        })?;

        // obtain the failure type from the artifact file name (crash-... timeout-... ecc)
        let artifact_name = artifact.file_name().unwrap();
        let fail_type = if let Some(fail_type) = artifact_name.to_str().unwrap().split('-').next() {
            fail_type
        } else {
            bail!("Invalid artifact name {}", artifact.display());
        };

        let target_dir = failures.join(format!("fail-{}", artifact_id));
        std::fs::create_dir(&target_dir).with_context(|| {
            anyhow!(
                "Failed to create artifact output directory at {}",
                target_dir.display()
            )
        })?;

        let failure_path = target_dir.join(format!("output-{}.txt", fail_type));
        std::fs::write(&failure_path, &output).with_context(|| {
            anyhow!(
                "Failed to write {} bytes of output file at {}",
                output.len(),
                failure_path.display()
            )
        })?;

        let source_input_path = data.task_dir.join(format!("input/input{}.txt", id));
        let source_correct_path = data.task_dir.join(format!("output/output{}.txt", id));
        let target_input_path = target_dir.join("input.txt");
        let target_output_path = target_dir.join("output.txt");
        std::os::unix::fs::symlink(&source_input_path, &target_input_path).with_context(|| {
            anyhow!(
                "Failed to create symlink: {} -> {}",
                target_input_path.display(),
                source_input_path.display()
            )
        })?;
        std::os::unix::fs::symlink(&source_correct_path, &target_output_path).with_context(
            || {
                anyhow!(
                    "Failed to create symlink: {} -> {}",
                    target_output_path.display(),
                    source_correct_path.display()
                )
            },
        )?;
        cwrite!(printer, RED, "[FAIL] {:<8}", fail_type);
        println!(" {}", target_dir.display());
    }

    warn!(
        "{} checker failure written to {}",
        artifacts.len(),
        failures.display()
    );

    Ok(())
}
