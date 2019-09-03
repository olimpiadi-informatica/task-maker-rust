#!/usr/bin/env python3

import argparse
import datetime
import glob
import os
import os.path
import subprocess
import time

import sqlite3

TIMEOUT = 3 * 60  # in seconds
NUM_CORES = 7

SCHEMA = """
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    version TEXT NOT NULL,
    start_time TIMESTAMP NOT NULL
);

CREATE TABLE IF NOT EXISTS tests (
    task_name TEXT NOT NULL,
    session_id INTEGER NOT NULL,
    start_time TIMESTAMP NOT NULL,
    duration DOUBLE NOT NULL,
    killed INTEGER NOT NULL CHECK ( killed = 0 OR killed = 1 ),
    stdout TEXT NOT NULL,
    stderr TEXT NOT NULL,
    return_code INTEGER NOT NULL,
    PRIMARY KEY (task_name, session_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);
"""


def main(args):
    db = sqlite3.connect(args.db)
    db.executescript(SCHEMA)

    if args.session is None:
        version = get_version(args)
        start_time = datetime.datetime.now()
        cur = db.cursor()
        cur.execute("INSERT INTO sessions (version, start_time) VALUES (?, ?)",
                    (version, start_time))
        session_id = cur.lastrowid
    else:
        cur = db.cursor()
        cur.execute("SELECT * FROM sessions WHERE id = ?", (args.session,))
        if not cur.fetchall():
            raise ValueError(f"Session id {args.session} not present")
        session_id = args.session
    run_tests(args, db, session_id)
    db.commit()
    db.close()


def get_version(args):
    proc = subprocess.run([args.tm, "--version"], stdout=subprocess.PIPE)
    return proc.stdout.decode().strip()


def run_tests(args, db, session_id):
    for test in sorted(glob.glob(args.dir + "/*")):
        if not os.path.isdir(test):
            continue
        task_name = os.path.basename(test)
        cur = db.cursor()
        cur.execute("SELECT * FROM tests WHERE task_name = ? AND session_id = ?",
                    (task_name, session_id))
        if cur.fetchall():
            print(f"Task {task_name} already done, skipping")
            continue

        start_time = datetime.datetime.now()
        print(f"Starting {task_name} at {start_time}")
        start = time.monotonic()
        stdout, stderr, returncode, killed = run_test(args.tm, test)
        end = time.monotonic()
        duration = end - start

        cur = db.cursor()
        cur.execute(
            "INSERT INTO tests "
            "(task_name, session_id, start_time, duration, killed,"
            "stdout, stderr, return_code)"
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (task_name, session_id, start_time, duration, killed,
             stdout, stderr, returncode))
        db.commit()
        print(f"Completed {task_name} after {duration:.3f}s")


def run_test(tm, task_dir):
    tm = os.path.abspath(tm)
    args = [tm, "--task-dir", task_dir, "--num-cores", str(NUM_CORES),
            "--no-cache", "--ui", "json"]
    cwd = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    os.putenv("RUST_BACKTRACE", "1")
    try:
        proc = subprocess.run(args, timeout=TIMEOUT, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, cwd=cwd)
        return proc.stdout, proc.stderr, proc.returncode, False
    except subprocess.TimeoutExpired:
        return b"", b"", -1, True


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", help="Database file", default="db.sqlite3")
    parser.add_argument("--session", help="Continue a session", type=int)
    parser.add_argument("tm", help="Path to task-maker")
    parser.add_argument("dir", help="Testing directory")
    args = parser.parse_args()
    main(args)
