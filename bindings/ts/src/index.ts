import {Output, spawn} from "promisify-child-process";
import * as readline from "readline";
import {fromEvent, Observable} from "rxjs";
import {map, reduce, takeUntil} from "rxjs/operators";
import {Readable} from "stream";

export type RemoteAddr = {
    host: string;
    port?: number;
    password?: string;
};

export enum CacheKey {
    Compilation = "compilation",
    Generation = "generation",
    Evaluation = "evaluation",
    Checking = "checking",
    Booklet = "booklet",
}

export type CacheConfig = {
    minCache?: number;
    maxCache?: number;
    noCache?: boolean | CacheKey[];
};

export type Config = {
    taskMakerPath?: string;
    remote?: string | RemoteAddr;
    localStoreDir?: string;
    dryRun?: boolean;
    noStatement?: boolean;
    cache?: CacheConfig;
};

export type EvaluationConfig = {
    taskDir: string;
    solution?: string;
    filter?: string[];
};

export type TaskInfoConfig = {
    taskDir: string;
};

export type CleanConfig = {
    taskDir: string;
};

export type EvaluationResult = {
    lines: Observable<any>;
    stderr: Promise<string>;
    child: Promise<Output>;
};

export type CleanResult = {
    stderr: Promise<string>;
    child: Promise<Output>;
};

export type TaskInfoResult = {
    taskInfo: Promise<any>;
    stderr: Promise<string>;
    child: Promise<Output>;
};

export const defaultConfig: Config = {
    taskMakerPath: null,
    remote: null,
    localStoreDir: null,
    dryRun: true,
    noStatement: true,
    cache: {
        minCache: null,
        maxCache: null,
        noCache: false,
    },
};

export class TaskMaker {
    config: Config;

    constructor(config: Config = {}) {
        this.config = Object.assign({}, defaultConfig, config);
    }

    evaluate(evalConfig: EvaluationConfig): EvaluationResult {
        const bin = binPath(this.config);
        let args: string[] = buildArgs(this.config);
        args = args.concat(["--task-dir", evalConfig.taskDir]);
        if (evalConfig.solution) {
            args = args.concat(["--solution", evalConfig.solution]);
        }
        if (evalConfig.filter) {
            for (const filter of evalConfig.filter) {
                args.push(filter);
            }
        }

        const child = spawn(bin, args);
        const stdoutReader = readline.createInterface(child.stdout);
        const lines = fromEvent<string>(stdoutReader, "line").pipe(
            takeUntil(fromEvent(stdoutReader, "close")),
            map((json) => JSON.parse(json))
        );
        const stderr = capture(child.stderr);
        return {
            lines,
            stderr,
            child,
        };
    }

    taskInfo(infoConfig: TaskInfoConfig): TaskInfoResult {
        let bin = binPath(this.config);
        let args = buildArgs(this.config);
        args = args.concat(["--task-dir", infoConfig.taskDir]);
        args.push("--task-info");

        const child = spawn(bin, args);
        const stdout = capture(child.stdout);
        const stderr = capture(child.stderr);
        return {
            taskInfo: stdout.then((stdout) => JSON.parse(stdout)),
            stderr,
            child,
        };
    }

    clean(cleanConfig: CleanConfig): CleanResult {
        let bin = binPath(this.config);
        let args = buildArgs(this.config);
        args = args.concat(["--task-dir", cleanConfig.taskDir]);
        args.push("--clean");
        const child = spawn(bin, args);
        const stderr = capture(child.stderr);
        return {
            stderr,
            child,
        };
    }
}

const binPath = (config: Config): string => {
    return config.taskMakerPath ?? "task-maker-rust";
};

const buildArgs = (config: Config): string[] => {
    let res: string[] = ["--ui", "json"];
    if (config.remote) {
        res = res.concat(["--evaluate-on", buildRemoteAddr(config.remote)]);
    }
    if (config.localStoreDir) {
        res = res.concat(["--store-dir", config.localStoreDir]);
    }
    if (config.dryRun) {
        res.push("--dry-run");
    }
    if (config.noStatement) {
        res.push("--no-statement");
    }
    if (config.cache) {
        if (config.cache.minCache) {
            res = res.concat(["--min-cache", config.cache.minCache.toString()]);
        }
        if (config.cache.maxCache) {
            res = res.concat(["--max-cache", config.cache.maxCache.toString()]);
        }
        if (config.cache.noCache) {
            if (typeof config.cache.noCache === "boolean") {
                res = res.concat(["--no-cache", Object.values(CacheKey).join(",")]);
            } else {
                res = res.concat(["--no-cache", config.cache.noCache.join(",")]);
            }
        }
    }
    return res;
};

const capture = (stream: Readable): Promise<string> => {
    return fromEvent<Buffer>(stream, "data")
        .pipe(
            takeUntil(fromEvent(stream, "end")),
            map((l) => l.toString("utf-8")),
            reduce((a, b) => a + b, "")
        )
        .toPromise();
};

const buildRemoteAddr = (remote: string | RemoteAddr): string => {
    if (typeof remote === "string") {
        return remote;
    }
    let url = "tcp://";
    if (remote.password) {
        url += `:${remote.password}@`;
    }
    url += remote.host;
    if (remote.port) {
        url += `:${remote.port}`;
    }
    return url;
};
