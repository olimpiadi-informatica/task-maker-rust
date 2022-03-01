import * as path from "path";
import * as vscode from 'vscode';

const gengenSelector: vscode.DocumentSelector = { scheme: "file", language: "gengen" };

// Folding for the subtasks in the gen/GEN file.
vscode.languages.registerFoldingRangeProvider(gengenSelector, {
	provideFoldingRanges: (document) => {
		const text = document.getText();
		const lines = text.split("\n");
		const subtaskPositions = [];
		for (let lineNumber = 0; lineNumber < lines.length; lineNumber++) {
			if (lines[lineNumber].startsWith("#ST:")) {
				subtaskPositions.push(lineNumber);
			}
		}
		subtaskPositions.push(lines.length);
		const result = [];
		for (let i = 0; i < subtaskPositions.length - 1; i++) {
			const start = subtaskPositions[i];
			let end = subtaskPositions[i + 1] - 1;
			// skip one empty line
			if (end > start && lines[end].trim() === "") {
				end--;
			}
			result.push(new vscode.FoldingRange(start, end, vscode.FoldingRangeKind.Region));
		}
		return result;
	}
});

const diagnosticCollection = vscode.languages.createDiagnosticCollection("gengen")

export async function updateDiags(document: vscode.TextDocument, collection: vscode.DiagnosticCollection) {
	const text = document.getText();
	const lines = text.split("\n");

	const diagnostics = [];
	for (let lineNumber = 0; lineNumber < lines.length; lineNumber++) {
		const line = lines[lineNumber];

		if (line.startsWith("#ST:")) {
			const score = line.split("#ST:", 2)[1].trim();
			if (/^\d+$/.test(score)) continue; // Valid subtask score.

			const column = line.indexOf(score);
			const range = new vscode.Range(
				new vscode.Position(lineNumber, column),
				new vscode.Position(lineNumber, column + score.length)
			);
			const diag = new vscode.Diagnostic(range, "The subtask's score must be an integer.", vscode.DiagnosticSeverity.Error);
			diagnostics.push(diag);
		} else if (line.startsWith("#COPY:")) {
			const filePath = line.split("#COPY:", 2)[1].trim();
			const taskDir = path.dirname(path.dirname(document.uri.path));
			const fullFilePath = path.join(taskDir, filePath);
			try {
				await vscode.workspace.fs.stat(vscode.Uri.file(fullFilePath));
			} catch {
				const column = line.indexOf(filePath);
				const range = new vscode.Range(
					new vscode.Position(lineNumber, column),
					new vscode.Position(lineNumber, column + filePath.length)
				);
				const diag = new vscode.Diagnostic(range, `The file "${filePath}" does not exist in the task folder.`, vscode.DiagnosticSeverity.Error);
				diagnostics.push(diag);
			}
		}
	}
	collection.set(document.uri, diagnostics);
}

export function activate(context: vscode.ExtensionContext) {
	if (vscode.window.activeTextEditor) {
		updateDiags(vscode.window.activeTextEditor.document, diagnosticCollection);
	}

	context.subscriptions.push(diagnosticCollection);
	context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor((editor) => {
		if (editor) {
			updateDiags(editor.document, diagnosticCollection);
		}
	}))
	context.subscriptions.push(vscode.workspace.onDidChangeTextDocument((editor) => {
		updateDiags(editor.document, diagnosticCollection);
	}))
}
