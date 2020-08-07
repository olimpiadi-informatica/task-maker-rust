// Type aliases
export type SubtaskId = number;
export type TestcaseId = number;
export type FileUuid = string;
export type WorkerUuid = string;
export type ClientUuid = string;
export type Seed = number;
export type Language = string;
export type Mutex<T> = T;
// A message sent to the UI.
export type UIMessage =
  | "StopUI"
  | {
      ServerStatus: {
        status: ExecutorStatus<{
          secs_since_epoch: number;
          nanos_since_epoch: number;
        }>;
      };
    }
  | { Compilation: { file: string; status: UIExecutionStatus } }
  | { IOITask: { task: IOITask } }
  | {
      IOIGeneration: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        status: UIExecutionStatus;
      };
    }
  | {
      IOIValidation: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        status: UIExecutionStatus;
      };
    }
  | {
      IOISolution: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        status: UIExecutionStatus;
      };
    }
  | {
      IOIEvaluation: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        solution: string;
        status: UIExecutionStatus;
        part: number;
        num_parts: number;
      };
    }
  | {
      IOIChecker: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        solution: string;
        status: UIExecutionStatus;
      };
    }
  | {
      IOITestcaseScore: {
        subtask: SubtaskId;
        testcase: TestcaseId;
        solution: string;
        score: number;
        message: string;
      };
    }
  | {
      IOISubtaskScore: {
        subtask: SubtaskId;
        solution: string;
        normalized_score: number;
        score: number;
      };
    }
  | { IOITaskScore: { solution: string; score: number } }
  | { IOIBooklet: { name: string; status: UIExecutionStatus } }
  | {
      IOIBookletDependency: {
        booklet: string;
        name: string;
        step: number;
        num_steps: number;
        status: UIExecutionStatus;
      };
    }
  | { TerryTask: { task: TerryTask } }
  | {
      TerryGeneration: {
        solution: string;
        seed: Seed;
        status: UIExecutionStatus;
      };
    }
  | { TerryValidation: { solution: string; status: UIExecutionStatus } }
  | { TerrySolution: { solution: string; status: UIExecutionStatus } }
  | { TerryChecker: { solution: string; status: UIExecutionStatus } }
  | {
      TerrySolutionOutcome: {
        solution: string;
        outcome: { Ok: SolutionOutcome } | { Err: string };
      };
    }
  | { Warning: { message: string } };
// The status of an execution.
export type UIExecutionStatus =
  | "Pending"
  | { Started: { worker: WorkerUuid } }
  | { Done: { result: ExecutionResult } }
  | "Skipped";
// The current status of the `Executor`, this is sent to the user when the server status is asked.
// The type parameter `T` is either `SystemTime` for local usage or `Duration` for serialization.
// Unfortunately since `Instant` is not serializable by design, it cannot be used.
export type ExecutorStatus<T> = {
  connected_workers: ExecutorWorkerStatus<T>[];
  ready_execs: number;
  waiting_execs: number;
};
// Status of a worker of an `Executor`.
export type ExecutorWorkerStatus<T> = {
  uuid: WorkerUuid;
  name: string;
  current_job: WorkerCurrentJobStatus<T> | null;
};
// Information about the job the worker is currently doing.
export type WorkerCurrentJobStatus<T> = {
  job: string;
  client: ClientInfo;
  duration: T;
};
// Information about a client of the scheduler.
export type ClientInfo = { uuid: ClientUuid; name: string };
// Information about a generic IOI task.
export type IOITask = {
  path: string;
  task_type: TaskType;
  name: string;
  title: string;
  time_limit: number | null;
  memory_limit: number | null;
  infile: string | null;
  outfile: string | null;
  subtasks: { [key in SubtaskId]: SubtaskInfo };
  testcase_score_aggregator: TestcaseScoreAggregator;
  grader_map: GraderMap;
  booklets: Booklet[];
  difficulty: number | null;
  syllabus_level: number | null;
};
// Information about a generic Terry task.
export type TerryTask = {
  path: string;
  name: string;
  description: string;
  max_score: number;
};
// The output of the checker for a solution.
export type SolutionOutcome = {
  score: number;
  validation: SolutionValidation;
  feedback: SolutionFeedback;
};
// The result of an [`Execution`](struct.Execution.html).
export type ExecutionResult = {
  status: ExecutionStatus;
  was_killed: boolean;
  was_cached: boolean;
  resources: ExecutionResourcesUsage;
  stdout: number[] | null;
  stderr: number[] | null;
};
// The type of the task. This changes the behavior of the solutions.
export type TaskType =
  | { Batch: BatchTypeData }
  | { Communication: CommunicationTypeData };
// A subtask of a IOI task.
export type SubtaskInfo = {
  id: SubtaskId;
  description: string | null;
  max_score: number;
  testcases: { [key in TestcaseId]: TestcaseInfo };
};
// A testcase of a IOI task.
// Every testcase has an input and an output that will be put in the input/ and output/ folders.
// The files are written there only if it's not a dry-run and if the files are not static.
export type TestcaseInfo = {
  id: TestcaseId;
  input_generator: InputGenerator;
  input_validator: InputValidator;
  output_generator: OutputGenerator;
};
// The aggregator of testcase scores for computing the subtask score.
export enum TestcaseScoreAggregator {
  Min = "Min",
  Sum = "Sum",
}
// The storage of the compilation/runtime dependencies for the source files.
// A source file may need some extra dependencies in order to be compiled and/or executed. For
// example a C++ file may need a second C++ file to be linked together, or a Python file may need
// a second Python file to be run.
export type GraderMap = { graders: { [key in string]: Dependency } };
// A dependency of an execution, all the sandbox paths must be relative and inside of the sandbox.
export type Dependency = {
  file: File;
  local_path: string;
  sandbox_path: string;
  executable: boolean;
};
// An handle to a file in the evaluation, this only tracks dependencies between executions.
export type File = { uuid: FileUuid; description: string };
// A `Booklet` is a pdf file containing the statements of some tasks. It is compiled from a series
// of `.tex` files defined by `Statement` objects. The compiled pdf file is then copied somewhere.
export type Booklet = {
  config: BookletConfig;
  statements: Statement[];
  dest: string;
};
// Configuration of a `Booklet`, including the setting from the contest configuration.
export type BookletConfig = {
  language: string;
  show_solutions: boolean;
  show_summary: boolean;
  font_enc: string;
  input_enc: string;
  description: string | null;
  location: string | null;
  date: string | null;
  logo: string | null;
};
// A statement is a `.tex` file with all the other assets included in its directory.
export type Statement = {
  config: StatementConfig;
  path: string;
  content: string;
};
// The configuration of a `Statement`.
export type StatementConfig = {
  name: string;
  title: string;
  infile: string;
  outfile: string;
  time_limit: number | null;
  memory_limit: number | null;
  difficulty: number | null;
  syllabus_level: number | null;
};
// The validation part of the outcome of a solution.
export type SolutionValidation = {
  cases: SolutionValidationCase[];
  alerts: SolutionAlert[];
};
// The validation outcome of a test case.
export type SolutionValidationCase = {
  status: CaseStatus;
  message: string | null;
};
// A message with an associated severity.
export type SolutionAlert = { severity: string; message: string };
// The possible statuses of the validation of a test case.
export enum CaseStatus {
  missing = "missing",
  parsed = "parsed",
  invalid = "invalid",
}
// The feedback part of the outcome.
export type SolutionFeedback = {
  cases: SolutionFeedbackCase[];
  alerts: SolutionAlert[];
};
// The feedback of a test case.
export type SolutionFeedbackCase = { correct: boolean; message: string | null };
// Status of a completed [`Execution`](struct.Execution.html).
export type ExecutionStatus =
  | "Success"
  | { ReturnCode: number }
  | { Signal: [number, string] }
  | "TimeLimitExceeded"
  | "SysTimeLimitExceeded"
  | "WallTimeLimitExceeded"
  | "MemoryLimitExceeded"
  | { InternalError: string };
// Resources used during the execution, note that on some platform these values may not be
// accurate.
export type ExecutionResourcesUsage = {
  cpu_time: number;
  sys_time: number;
  wall_time: number;
  memory: number;
};
// The internal data of a task of type `Batch`.
export type BatchTypeData = { checker: Checker };
// The internal data of a task of type `Batch`.
export type CommunicationTypeData = {
  manager: SourceFile;
  num_processes: number;
};
// Which tool to use to compute the score on a testcase given the input file, the _correct_ output
// file and the output file to evaluate.
export type Checker = "WhiteDiff" | { Custom: SourceFile };
// A source file that will be able to be executed (with an optional compilation step).
// After creating a `SourceFile` using `new` you can add start using it via the `execute` method.
// Note that it may add to the DAG an extra execution for compiling the source file.
export type SourceFile = {
  path: string;
  base_path: string;
  language: Language;
  executable: Mutex<File | null>;
  grader_map: GraderMap | null;
  copy_exe: boolean;
  write_bin_to: string | null;
};
// The source of the input files. It can either be a statically provided input file or a custom
// command that will generate an input file.
export type InputGenerator =
  | { StaticFile: string }
  | { Custom: [SourceFile, string[]] };
// An input file validator is responsible for checking that the input file follows the format and
// constraints defined by the task.
export type InputValidator = "AssumeValid" | { Custom: [SourceFile, string[]] };
// The source of the output files. It can either be a statically provided output file or a custom
// command that will generate an output file.
export type OutputGenerator =
  | "NotAvailable"
  | { StaticFile: string }
  | { Custom: [SourceFile, string[]] };
// Information about a parsed task, returned with the `--task-info` option.
export type TaskInfo = { IOI: IOITaskInfo } | { Terry: TerryTaskInfo };
// Task information structure.
export type IOITaskInfo = {
  version: number;
  name: string;
  title: string;
  scoring: TaskInfoScoring;
  limits: TaskInfoLimits;
  statements: TaskInfoStatement[];
  attachments: TaskInfoAttachment[];
};
// Limits of the task.
export type TaskInfoLimits = { time: number | null; memory: number | null };
// Attachment of the task.
export type TaskInfoAttachment = {
  name: string;
  content_type: string;
  path: string;
};
// Info of the subtasks.
export type TaskInfoSubtask = { max_score: number; testcases: number };
// Scoring for the task.
export type TaskInfoScoring = {
  max_score: number;
  subtasks: TaskInfoSubtask[];
};
// Statement of the task.
export type TaskInfoStatement = {
  language: string;
  content_type: string;
  path: string;
};
// Task information structure.
export type TerryTaskInfo = {
  version: number;
  name: string;
  description: string;
  max_score: number;
};
