#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import sys
import shlex
import shutil
import signal
import atexit
import time
import threading
import subprocess
import logging
from glob import glob
from pathlib import Path
from multiprocessing import cpu_count
import tempfile
import argparse
from typing import List, Optional, Any
from types import FrameType

# ---------- defaults from env ----------
SERVER_ARGS: str = os.environ.get("SERVER_ARGS", "")
WORKER_ARGS: str = os.environ.get("WORKER_ARGS", "")
SERVER_ADDR: str = os.environ.get("SERVER_ADDR", "127.0.0.1:27183")
SPAWN_SERVER: bool = os.environ.get("SPAWN_SERVER", "true").lower() == "true"
SPAWN_WORKERS: bool = os.environ.get("SPAWN_WORKERS", "true").lower() == "true"
TM_LOGLEVEL: str = os.environ.get("TM_LOGLEVEL", "info").lower()

# default: nproc - 1 (at least 1)
default_nworkers: int = max(cpu_count() - 1, 1)

# ---------- logging ----------
LOGGER_NAME = "tm_entrypoint"
logger = logging.getLogger(LOGGER_NAME)

def _map_tm_loglevel(level: str) -> int:
    table = {
        "error": logging.ERROR,
        "warn":  logging.WARNING,
        "warning": logging.WARNING,
        "info":  logging.INFO,
        "debug": logging.DEBUG,
    }
    return table.get(level, logging.ERROR)

def configure_logging() -> None:
    logging.basicConfig(
        level=_map_tm_loglevel(TM_LOGLEVEL),
        format="[%(asctime)s %(levelname)s\t%(name)s::%(funcName)s] %(message)s",
        datefmt= "%Y-%m-%dT%H:%M:%S.xxxxxxxxxZ",
        stream=sys.stderr,
    )
    eff = logging.getLevelName(logger.getEffectiveLevel())
    logger.debug("Logger initialized at DEBUG (effective: %s).", eff)
    logger.info("Logging configured (level=%s).", eff)

# ---------- CLI ----------
def cli() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Entrypoint for task-maker-rust Docker image."
    )
    parser.add_argument(
        "-j", "--jobs",
        type=int,
        default=default_nworkers,
        help=f"Number of workers to launch [default: <nproc>-1 = {default_nworkers}]"
    )

    args = parser.parse_args()
    assert args.jobs > 0, f"Please specify a positive number of jobs, not {args.jobs}."

    return args

# ---------- cleanup ----------
if SPAWN_SERVER or SPAWN_WORKERS:
    def cleanup() -> None:
        logger.debug("Cleaning up temporary stores")
        for p in glob("/tmp/tmserver.*"):
            logger.debug("Removing server store: %s", p)
            shutil.rmtree(p, ignore_errors=True)
        for p in glob("/tmp/tmworker.*"):
            logger.debug("Removing worker store: %s", p)
            shutil.rmtree(p, ignore_errors=True)
        logger.debug("Cleanup complete.")

    atexit.register(cleanup)

    def _signal_handler(signum: int, frame: Optional[FrameType]) -> None:
        name = signal.Signals(signum).name if hasattr(signal, "Signals") else str(signum)
        logger.warning("Received signal %s — shutting down.", name)
        # atexit will run cleanup
        code = 130 if signum == signal.SIGINT else 143
        sys.exit(code)

    signal.signal(signal.SIGINT, _signal_handler)
    signal.signal(signal.SIGTERM, _signal_handler)

# ---------- verbosity mapping for task-maker-tools ----------
def loglevel_verbosity_flag(level: str) -> str:
    flag = {
        "error": "",
        "warn": "-v",
        "warning": "-v",
        "info": "-vv",
        "debug": "-vvv",
    }.get(level, "")
    logger.debug(f"Mapped TM_LOGLEVEL={level} to "
                 f"task-maker-tools flag '{flag or "<none>"}'")
    return flag

# ---------- spawn helpers ----------
def make_worker_stores(nworkers: int) -> List[str]:
    base = tempfile.mktemp(prefix="tmworker.", dir="/tmp")
    stores: List[str] = []
    for i in range(1, nworkers + 1):
        idx = f"{i:02d}"
        d = f"{base}-{idx}"
        Path(d).mkdir(parents=True, exist_ok=True)
        stores.append(d)
    logger.debug("Created %d worker stores (base=%s): %s", nworkers, base, stores)
    return stores

def spawn_tmserver() -> int:
    verbosity_flag: str = loglevel_verbosity_flag(TM_LOGLEVEL)
    def make_server_store() -> Path:
        path = tempfile.mkdtemp(prefix="tmserver.", dir="/tmp")
        logger.debug("Created server store: %s", path)
        return path

    server_store: str = make_server_store()
    cmd: List[str] = ["task-maker-tools"]
    if verbosity_flag:
        cmd.append(verbosity_flag)
    cmd += ["server", "--store-dir", server_store]
    if SERVER_ARGS.strip():
        cmd += shlex.split(SERVER_ARGS)

    logger.debug("Exec: %s", " ".join(shlex.quote(c) for c in cmd))
    logger.info("Starting server (store=%s)…", server_store)
    rc = subprocess.call(cmd)
    logger.info("Server exited with rc=%d", rc)
    return rc

def spawn_tmworker(store_dir: str) -> subprocess.Popen[Any]:
    verbosity_flag: str = loglevel_verbosity_flag(TM_LOGLEVEL)

    cmd: List[str] = ["task-maker-tools"]
    if verbosity_flag:
        cmd.append(verbosity_flag)
    cmd += ["worker", "--store-dir", store_dir]
    if WORKER_ARGS.strip():
        cmd += shlex.split(WORKER_ARGS)
    cmd.append(SERVER_ADDR)

    logger.debug("Exec: %s", " ".join(shlex.quote(c) for c in cmd))
    logger.info("Starting worker (store=%s) → %s", store_dir, SERVER_ADDR)
    proc = subprocess.Popen(cmd)
    logger.debug("Worker PID %s started for store %s", proc.pid, store_dir)
    return proc

# ---------- main ----------
def main() -> int:
    configure_logging()
    args = cli()
    nworkers: int = int(args.jobs)

    logger.info(
        f"Config: jobs={nworkers}, server={SPAWN_SERVER}, worker={SPAWN_WORKERS}, "
        f"addr={SERVER_ADDR}, tm_loglevel={TM_LOGLEVEL}"
    )
    if SERVER_ARGS.strip():
        logger.debug("SERVER_ARGS=%r", SERVER_ARGS)
    if WORKER_ARGS.strip():
        logger.debug("WORKER_ARGS=%r", WORKER_ARGS)

    # create nworkers file in the current dir and save a 0 into it
    nworkers_file: Path = Path('nworkers')
    with nworkers_file.open('w') as nw_fp:
        nw_fp.write("0\n")

    if SPAWN_WORKERS:
        worker_stores: List[str] = make_worker_stores(nworkers)
        # overwrite ith the actual number of wokers if we spawn them
        with nworkers_file.open('w') as nw_fp:
            nw_fp.write(f"{nworkers}\n")

    # worker only
    if (not SPAWN_SERVER) and SPAWN_WORKERS:
        logger.info("Mode: workers only.")
        procs: List[subprocess.Popen[Any]] = [spawn_tmworker(store_dir=w)
                                              for w in worker_stores]
        exit_codes: List[Optional[int]] = [p.wait() for p in procs]
        max_rc = max(code or 0 for code in exit_codes)
        logger.info("All workers exited. Max rc=%d", max_rc)
        return max_rc

    # server only
    elif SPAWN_SERVER and (not SPAWN_WORKERS):
        logger.info("Mode: server only.")
        return spawn_tmserver()

    # server + worker
    elif SPAWN_SERVER and SPAWN_WORKERS:
        logger.info("Mode: server + workers.")
        procs: List[subprocess.Popen[Any]] = []

        def launch_workers() -> None:
            logger.debug("Delaying worker launch by 2s to let server come up…")
            time.sleep(2.0)
            for w in worker_stores:
                procs.append(spawn_tmworker(store_dir=w))
            for p in procs:
                rc = p.wait()
                logger.info("Worker PID %s exited with rc=%d", p.pid, rc)

        t = threading.Thread(target=launch_workers, daemon=True)
        t.start()
        server_rc: int = spawn_tmserver()

        logger.debug("Server finished (rc=%d). Joining worker launcher…", server_rc)
        t.join(timeout=1.0)
        # If any workers still running, terminate politely
        for p in procs:
            if p.poll() is None:
                logger.debug("Terminating lingering worker PID %s…", p.pid)
                try:
                    p.terminate()
                except Exception as e:
                    logger.warning("Failed to terminate worker PID %s: %s", p.pid, e)
        return server_rc

    # nothing to spawn -> shell
    else:
        logger.info("Mode: nothing to spawn — opening interactive shell.")
        shell: str = os.environ.get("SHELL", "/bin/bash")
        try:
            return subprocess.call([shell])
        except FileNotFoundError:
            logger.error("Shell %r not found; exiting 0.", shell)
            return 0

if __name__ == "__main__":
    try:
        sys.exit(main())
    except subprocess.CalledProcessError as e:
        logger.exception("Subprocess failed (rc=%s).", e.returncode)
        sys.exit(e.returncode)
    except Exception:
        logger.exception("Fatal error:")
        sys.exit(1)
