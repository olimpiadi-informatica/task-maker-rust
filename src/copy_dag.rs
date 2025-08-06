use std::collections::HashMap;
use std::fmt::Write;

use task_maker_dag::{Execution, ExecutionDAG, File, ProvidedFile};

/// A node in the printed graph.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
enum Node {
    /// The node is an Execution.
    Execution(Execution),
    /// The node is a File.
    File(File),
}

/// An edge of the printed graph, linking 2 nodes.
type Edge = (Node, Node);

/// Render to string the `ExecutionDAG` in DOT format.
pub fn render_dag(dag: &ExecutionDAG) -> String {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut files = HashMap::new();
    for file in dag.data.provided_files.values() {
        match file {
            ProvidedFile::LocalFile { file, .. } | ProvidedFile::Content { file, .. } => {
                files.insert(file.uuid, file.clone());
            }
        }
    }
    for group in dag.data.execution_groups.values() {
        for exec in &group.executions {
            nodes.push(Node::Execution(exec.clone()));
            for out in exec.outputs.values() {
                edges.push((Node::Execution(exec.clone()), Node::File(out.clone())));
                files.insert(out.uuid, out.clone());
            }
            if let Some(out) = &exec.stdout {
                edges.push((Node::Execution(exec.clone()), Node::File(out.clone())));
                files.insert(out.uuid, out.clone());
            }
            if let Some(out) = &exec.stderr {
                edges.push((Node::Execution(exec.clone()), Node::File(out.clone())));
                files.insert(out.uuid, out.clone());
            }
        }
    }
    for group in dag.data.execution_groups.values() {
        for exec in &group.executions {
            for dep in exec.dependencies() {
                if !files.contains_key(&dep) {
                    panic!("Nope: {exec:#?} does not contain {dep:?}");
                }
                let file = &files[&dep];
                edges.push((Node::File(file.clone()), Node::Execution(exec.clone())));
            }
        }
    }
    for (_, file) in files {
        nodes.push(Node::File(file));
    }
    nodes.sort_by_cached_key(node_label);
    render_graph(nodes, edges)
}

/// Obtain the identifier of the node for the DOT file.
fn node_id(n: &Node) -> String {
    let uuid = match n {
        Node::Execution(exec) => exec.uuid.to_string(),
        Node::File(file) => file.uuid.to_string(),
    };
    "uuid".to_string() + &uuid.replace('-', "")
}

/// Obtain the label of the node for the DOT format.
fn node_label(n: &Node) -> String {
    match n {
        Node::Execution(e) => format!(
            "{} | {:?} {}",
            e.description.clone(),
            e.command,
            e.args.join(" ")
        ),
        Node::File(f) => f.description.clone(),
    }
}

/// Render to string the nodes and the edges in the DOT format, including the header and footer of
/// the format.
fn render_graph(nodes: Vec<Node>, edges: Vec<Edge>) -> String {
    let mut res = "".to_string();
    res += "digraph taskmaker {\n";
    res += "    rankdir=\"LR\";\n";
    for node in nodes {
        let style = match &node {
            Node::Execution(_) => "style=rounded shape=record",
            Node::File(_) => "style=dashed shape=box",
        };
        let _ = writeln!(
            res,
            "    {}[label=\"{}\"][{}];",
            node_id(&node),
            node_label(&node)
                .replace('"', "\\\"")
                .replace('{', "\\{")
                .replace('}', "\\}")
                .replace('<', "\\<")
                .replace('>', "\\>"),
            style
        );
    }
    for (a, b) in edges {
        let _ = writeln!(res, "    {} -> {};", node_id(&a), node_id(&b));
    }
    res += "}\n";

    res
}
