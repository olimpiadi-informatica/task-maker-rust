use crate::{bind_exec_callbacks, ui::UIMessage, EvaluationData};
use failure::Error;
use std::path::PathBuf;
use task_maker_dag::{Execution, ExecutionCommand, File};

pub struct AsyFile;

impl AsyFile {
    /// Compile the asy file and return the handle to the compiled and cropped pdf file.
    pub fn compile<P: Into<PathBuf>>(
        source: P,
        eval: &mut EvaluationData,
        booklet_name: &str,
    ) -> Result<File, Error> {
        let source_path = source.into();
        let booklet = booklet_name.to_string();
        let name = source_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let source_file = File::new(format!("Source of {}", name));

        let mut comp = Execution::new(
            format!("Compilation of {}", name),
            ExecutionCommand::System("asy".into()),
        );
        comp.args(vec!["-f", "pdf", "-o", "output.pdf", "source.asy"]);
        comp.limits_mut()
            .read_only(false)
            .nproc(1000)
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        comp.input(&source_file, "source.asy", false);
        eval.dag.provide_file(source_file, &source_path)?;
        bind_exec_callbacks!(
            eval,
            comp.uuid,
            |status, booklet, name| UIMessage::IOIBookletDependency {
                booklet,
                name,
                step: 0,
                num_steps: 2,
                status
            },
            booklet,
            name
        );
        // TODO find deps
        let compiled = comp.output("output.pdf");
        eval.dag.add_execution(comp);

        let mut crop = Execution::new(
            format!("Crop of {}", name),
            ExecutionCommand::System("pdfcrop".into()),
        );
        crop.limits_mut()
            .read_only(false)
            .nproc(1000)
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        crop.args(vec!["source.pdf"]);
        crop.input(compiled, "source.pdf", false);
        bind_exec_callbacks!(
            eval,
            crop.uuid,
            |status, booklet, name| UIMessage::IOIBookletDependency {
                booklet,
                name,
                step: 1,
                num_steps: 2,
                status
            },
            booklet,
            name
        );
        let cropped = crop.output("source-crop.pdf");
        eval.dag.add_execution(crop);

        Ok(cropped)
    }
}
