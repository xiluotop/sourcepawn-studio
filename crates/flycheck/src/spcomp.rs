use lazy_static::lazy_static;
use paths::AbsPathBuf;
use regex::Regex;

/// Severity levels of spcomp errors.
#[derive(Debug, Clone)]
pub enum SpCompSeverity {
    Warning,
    Error,
    FatalError,
}

impl SpCompSeverity {
    /// Convert to a [LSP DiagnosticSeverity](lsp_types::DiagnosticSeverity).
    pub fn to_lsp_severity(&self) -> lsp_types::DiagnosticSeverity {
        match self {
            SpCompSeverity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            SpCompSeverity::Error => lsp_types::DiagnosticSeverity::ERROR,
            SpCompSeverity::FatalError => lsp_types::DiagnosticSeverity::ERROR,
        }
    }
}

/// Representation of an spcomp error.
#[derive(Debug, Clone)]
pub struct SpCompDiagnostic {
    /// [Path](AbsPathBuf) of the document where the error comes from.
    path: AbsPathBuf,

    /// Line index of the error.
    line_index: u32,

    /// Severity of the error.
    severity: SpCompSeverity,

    /// Code of the error.
    code: String,

    /// Message of the error.
    message: String,
}

impl SpCompDiagnostic {
    pub fn path(&self) -> &AbsPathBuf {
        &self.path
    }

    pub fn line_index(&self) -> u32 {
        self.line_index
    }

    pub fn severity(&self) -> &SpCompSeverity {
        &self.severity
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn try_from_line(line: &str) -> Option<Self> {
        lazy_static! {
            static ref RE: Regex = Regex::new(
                r"([:/\\A-Za-z\-_0-9. ]*)\((\d+)+\) : (?:(error|fatal error|warning) ([0-9]*)):\s+(.*)"
            )
            .expect("Failed to compile spcomp error regex.");
        }
        let capture = RE.captures(line)?;
        Some(Self {
            path: AbsPathBuf::try_from(capture.get(1)?.as_str()).ok()?,
            line_index: capture.get(2)?.as_str().parse::<u32>().ok()? - 1,
            severity: match capture.get(3)?.as_str() {
                "warning" => SpCompSeverity::Warning,
                "error" => SpCompSeverity::Error,
                "fatal error" => SpCompSeverity::FatalError,
                _ => unreachable!(),
            },
            code: capture.get(4)?.as_str().to_string(),
            message: capture.get(5)?.as_str().to_string(),
        })
    }
}

/// Return a [vector](Vec) of [strings](String) of the arguments to run spcomp.
pub fn build_args(
    root_path: &AbsPathBuf,
    out_path: &AbsPathBuf,
    includes_directories: &[AbsPathBuf],
    linter_arguments: &[String],
) -> Vec<String> {
    let mut args = vec![root_path.to_string()];
    if let Some(parent_path) = root_path.parent() {
        args.push(format!("-i{}", parent_path));
        let include_path = parent_path.join("include");
        if std::fs::metadata(&include_path).is_ok() {
            args.push(format!("-i{}", include_path));
        }
    }
    args.extend(
        includes_directories
            .iter()
            .map(|includes_directory| format!("-i{}", includes_directory)),
    );

    args.push(format!("-o{}", out_path));
    args.push("--syntax-only".to_string());

    args.extend_from_slice(linter_arguments);

    args
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::*;

    fn abs(path: PathBuf) -> AbsPathBuf {
        AbsPathBuf::assert(path)
    }

    /// Sets up a `<tmp>/project/plugin.sp` root path inside a fresh temp directory.
    /// Returns the temp dir (kept alive for the duration of the test), the root path
    /// and the project directory (i.e. the root path's parent).
    fn setup_root() -> (tempfile::TempDir, AbsPathBuf, PathBuf) {
        let tmp = tempdir().expect("failed to create temp dir");
        let project_dir = tmp.path().join("project");
        let root_path = abs(project_dir.join("plugin.sp"));
        (tmp, root_path, project_dir)
    }

    #[test]
    fn adds_project_directory_as_include_when_no_local_include_folder_exists() {
        let (_tmp, root_path, project_dir) = setup_root();
        let out_path = root_path.parent().unwrap().join("plugin.smx");

        let args = build_args(&root_path, &out_path, &[], &[]);

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    #[test]
    fn adds_local_include_folder_when_it_exists_on_disk() {
        let (_tmp, root_path, project_dir) = setup_root();
        let include_dir = project_dir.join("include");
        fs::create_dir_all(&include_dir).expect("failed to create include dir");
        let out_path = root_path.parent().unwrap().join("plugin.smx");

        let args = build_args(&root_path, &out_path, &[], &[]);

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-i{}", include_dir.display()),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    #[test]
    fn does_not_add_local_include_folder_when_it_does_not_exist_on_disk() {
        let (_tmp, root_path, project_dir) = setup_root();
        // Note: `<project_dir>/include` is intentionally not created on disk.
        let out_path = root_path.parent().unwrap().join("plugin.smx");

        let args = build_args(&root_path, &out_path, &[], &[]);

        let include_dir_arg = format!("-i{}", project_dir.join("include").display());
        assert!(
            !args.contains(&include_dir_arg),
            "should not add a nonexistent include folder, got: {args:?}"
        );
    }

    /// Regression test for the fix reordering `-i` flags: project-local include
    /// directories (the root's parent and its `include` subfolder, if any) must be
    /// searched *before* user-configured include directories, so that local project
    /// files take precedence over files of the same name found in a user's globally
    /// configured include directories.
    #[test]
    fn user_configured_includes_are_appended_after_project_includes() {
        let (tmp, root_path, project_dir) = setup_root();
        let out_path = root_path.parent().unwrap().join("plugin.smx");
        let user_include = abs(tmp.path().join("shared_includes"));

        let args = build_args(
            &root_path,
            &out_path,
            std::slice::from_ref(&user_include),
            &[],
        );

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-i{user_include}"),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    #[test]
    fn user_configured_includes_are_appended_after_local_include_folder() {
        let (tmp, root_path, project_dir) = setup_root();
        let include_dir = project_dir.join("include");
        fs::create_dir_all(&include_dir).expect("failed to create include dir");
        let out_path = root_path.parent().unwrap().join("plugin.smx");
        let user_include = abs(tmp.path().join("shared_includes"));

        let args = build_args(
            &root_path,
            &out_path,
            std::slice::from_ref(&user_include),
            &[],
        );

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-i{}", include_dir.display()),
                format!("-i{user_include}"),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    #[test]
    fn preserves_relative_order_of_multiple_user_configured_includes() {
        let (tmp, root_path, project_dir) = setup_root();
        let out_path = root_path.parent().unwrap().join("plugin.smx");
        let user_include_a = abs(tmp.path().join("includes_a"));
        let user_include_b = abs(tmp.path().join("includes_b"));
        let user_include_c = abs(tmp.path().join("includes_c"));

        let args = build_args(
            &root_path,
            &out_path,
            &[
                user_include_a.clone(),
                user_include_b.clone(),
                user_include_c.clone(),
            ],
            &[],
        );

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-i{user_include_a}"),
                format!("-i{user_include_b}"),
                format!("-i{user_include_c}"),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    /// Edge case: a root path that has no parent directory (e.g. sitting at the
    /// filesystem root) must not panic, must not contribute a project include, and
    /// must still append user-configured include directories.
    #[test]
    fn handles_root_path_without_a_parent_directory() {
        let tmp = tempdir().expect("failed to create temp dir");
        let fs_root = tmp.path().ancestors().last().unwrap().to_path_buf();
        let root_path = abs(fs_root);
        assert!(
            root_path.parent().is_none(),
            "test setup requires a root path without a parent"
        );
        let out_path = abs(tmp.path().join("plugin.smx"));
        let user_include = abs(tmp.path().join("shared_includes"));

        let args = build_args(
            &root_path,
            &out_path,
            std::slice::from_ref(&user_include),
            &[],
        );

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{user_include}"),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
            ]
        );
    }

    #[test]
    fn appends_linter_arguments_after_include_and_output_flags() {
        let (tmp, root_path, project_dir) = setup_root();
        let out_path = root_path.parent().unwrap().join("plugin.smx");
        let user_include = abs(tmp.path().join("shared_includes"));
        let linter_arguments = vec!["-w3".to_string(), "--verbose=2".to_string()];

        let args = build_args(
            &root_path,
            &out_path,
            std::slice::from_ref(&user_include),
            &linter_arguments,
        );

        assert_eq!(
            args,
            vec![
                root_path.to_string(),
                format!("-i{}", project_dir.display()),
                format!("-i{user_include}"),
                format!("-o{out_path}"),
                "--syntax-only".to_string(),
                "-w3".to_string(),
                "--verbose=2".to_string(),
            ]
        );
    }
}
