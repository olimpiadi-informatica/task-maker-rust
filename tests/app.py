#!/usr/bin/env python3

import argparse

import sqlite3
from flask import Flask, g, Response

app = Flask("task-maker tests")
database_path = None


def get_db():
    db = getattr(g, '_database', None)
    if db is None:
        db = g._database = sqlite3.connect(database_path)
    db.row_factory = sqlite3.Row
    return db


@app.teardown_appcontext
def close_connection(exception):
    db = getattr(g, '_database', None)
    if db is not None:
        db.close()


@app.route("/")
def index():
    cur = get_db().cursor()
    cur.execute("SELECT * FROM sessions")
    sessions = []
    for session in cur.fetchall():
        sessions.append(f"""
            <li>
                <a href="/session/{session["id"]}">
                    Session <strong>{session["id"]}</strong>:
                    {session["version"]} started at {session["start_time"]}
                </a>
            </li>
        """)
    sessions = "\n".join(sessions)
    return f"""
        <h1>Task maker test</h1>
        <ul>
            {sessions}
        </ul>
    """


@app.route("/session/<id>")
def session(id):
    cur = get_db().cursor()
    cur.execute("SELECT * FROM sessions WHERE id = ?", (id,))
    session = cur.fetchall()
    if not session:
        return "Session not found"
    session = session[0]
    tasks = []
    cur.execute(
        "SELECT task_name, return_code, killed FROM tests WHERE session_id = ? ORDER BY task_name",
        (session["id"],))
    for i, task in enumerate(cur.fetchall()):
        if task["killed"]:
            killed = """<span style="color: red">[killed]<span>"""
        else:
            killed = ""
        if task["return_code"] == 0:
            status = """<span style="color: green">[OK]<span>"""
        else:
            status = """<span style="color: red">[broken]<span>"""
        tasks.append(f"""
            <tr>
                <td>{i}</td>
                <td><a href="/session/{session["id"]}/task/{task["task_name"]}">{task["task_name"]}</a></td>
                <td>{killed} {status}</td>
            </tr>
        """)
    tasks = "\n".join(tasks)
    return f"""
        <h1>Session {session["id"]}</h1>
        <strong>Version</strong>: {session["version"]}<br>
        <strong>Start time</strong>: {session["start_time"]}<br>
        <strong>Tasks</strong>:
        <table>
            {tasks}
        </table>
    """


@app.route("/session/<id>/task/<name>")
def task(id, name):
    cur = get_db().cursor()
    cur.execute("SELECT * FROM tests WHERE task_name = ? AND session_id = ?",
                (name, id))
    task = cur.fetchall()
    if not task:
        return "Task not found"
    task = task[0]
    stderr = task["stderr"]
    if isinstance(stderr, bytes):
        stderr = stderr.decode()
    return f"""
        <h1>{task["task_name"]}</h1>
        <strong>Session</strong>: <a href="/session/{id}">{id}</a><br>
        <strong>Start time</strong>: {task["start_time"]}<br>
        <strong>Duration</strong>: {task["duration"]:.3f}s<br>
        <strong>Killed</strong>: {bool(task["killed"])}<br>
        <strong>Return code</strong>: {task["return_code"]}<br>
        <strong>Stdout</strong>: <a href="/session/{id}/task/{name}/stdout">view</a><br>
        <strong>Stderr</strong>:<br>
        <pre>{stderr}</pre>
    """


@app.route("/session/<id>/task/<name>/stdout")
def task_stdout(id, name):
    cur = get_db().cursor()
    cur.execute(
        "SELECT stdout FROM tests WHERE task_name = ? AND session_id = ?",
        (name, id))
    task = cur.fetchall()
    if not task:
        return "Task not found"
    task = task[0]
    return Response(task["stdout"], mimetype="text/plain")


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument("--db", help="Database file", default="db.sqlite3")
    args = parser.parse_args()
    database_path = args.db
    app.run(host="0.0.0.0")
