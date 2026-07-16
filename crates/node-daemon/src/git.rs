//! Git worktree preparation/finalization for an attempt (Stage 2.5).
//!
//! Git-backed tasks keep one clone per (node, repository) under
//! `repository_root/<name>`; each attempt gets a dedicated worktree on a
//! branch `agent/<task-id>/<n>`. Plain-dir tasks (empty `git_url`) just get a
//! fresh directory and no commit.

use std::path::{Path, PathBuf};
use std::process::Command;

use agentgrid_common::Assignment;
use anyhow::{Context, Result};

pub struct Workspace {
    /// Directory the adapter runs in.
    pub path: PathBuf,
    /// Local clone dir (None for plain-dir tasks).
    pub repo_dir: Option<PathBuf>,
    /// Attempt branch (None for plain-dir tasks).
    pub branch: Option<String>,
    pub default_branch: String,
    pub is_git: bool,
}

fn sh(dir: Option<&Path>, cmd: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir.unwrap_or_else(|| Path::new(".")))
        .status()
        .context("failed to spawn sh")?;
    if !status.success() {
        anyhow::bail!("command failed: {cmd}");
    }
    Ok(())
}

fn output(dir: &Path, cmd: &str) -> Result<String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(dir)
        .output()
        .context("failed to spawn sh")?;
    if !out.status.success() {
        anyhow::bail!("command failed: {cmd}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Ensure the repo clone exists and create a per-attempt worktree.
pub fn prepare_workspace(
    repository_root: &Path,
    workspace_root: &Path,
    assignment: &Assignment,
) -> Result<Workspace> {
    let ws = workspace_root.join(&assignment.attempt_id);
    std::fs::create_dir_all(&ws)?;
    if assignment.git_url.is_empty() {
        return Ok(Workspace {
            path: ws,
            repo_dir: None,
            branch: None,
            default_branch: String::new(),
            is_git: false,
        });
    }
    let repo_dir = repository_root.join(&assignment.repository);
    let branch = format!("agent/{}/{}", assignment.task_id, assignment.number);
    if repo_dir.join(".git").exists() {
        sh(
            Some(&repo_dir),
            &format!("git fetch origin {}", assignment.default_branch),
        )?;
    } else {
        std::fs::create_dir_all(repository_root)?;
        sh(
            Some(repository_root),
            &format!("git clone {} {}", assignment.git_url, assignment.repository),
        )?;
    }
    sh(
        Some(&repo_dir),
        &format!(
            "git checkout -B {} origin/{}",
            assignment.default_branch, assignment.default_branch
        ),
    )?;
    sh(
        Some(&repo_dir),
        &format!(
            "git worktree add {} -b {} {}",
            ws.display(),
            branch,
            assignment.default_branch
        ),
    )?;
    Ok(Workspace {
        path: ws,
        repo_dir: Some(repo_dir),
        branch: Some(branch),
        default_branch: assignment.default_branch.clone(),
        is_git: true,
    })
}

/// Commit any staged changes and write a binary diff (`changes.patch`) into the
/// workspace. Returns the commit SHA (or current HEAD for no-op), None for
/// plain-dir tasks.
pub fn finalize_workspace(ws: &Workspace, committer_email: &str) -> Result<Option<String>> {
    let (repo_dir, branch) = match (&ws.repo_dir, &ws.branch) {
        (Some(r), Some(b)) => (r, b),
        _ => return Ok(None),
    };
    sh(Some(&ws.path), "git add -A")?;
    let has_changes = !Command::new("sh")
        .arg("-c")
        .arg("git diff --cached --quiet")
        .current_dir(&ws.path)
        .status()?
        .success();
    let sha = if has_changes {
        sh(
            Some(&ws.path),
            &format!(
                "git -c user.name=agentgrid -c user.email={email} commit -m 'agentgrid: {branch}'",
                email = committer_email
            ),
        )?;
        output(&ws.path, "git rev-parse HEAD")?
    } else {
        output(&ws.path, "git rev-parse HEAD")?
    };
    let patch = output(
        repo_dir,
        &format!("git diff {} {} --binary", ws.default_branch, branch),
    )?;
    std::fs::write(ws.path.join("changes.patch"), patch)?;
    Ok(Some(sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentgrid_common::Assignment;

    fn make_assignment(git_url: &str, default_branch: &str) -> Assignment {
        Assignment {
            attempt_id: "attempt-test".into(),
            task_id: "task-test".into(),
            repository: "repo".into(),
            prompt: "x".into(),
            adapter: "mock".into(),
            number: 1,
            timeout_secs: 60,
            git_url: git_url.into(),
            default_branch: default_branch.into(),
            validation_command: None,
        }
    }

    #[test]
    fn plain_dir_has_no_commit() {
        let dir = std::env::temp_dir().join(format!("ag-git-plain-{}", uuid::Uuid::new_v4()));
        let ws_root = dir.join("ws");
        let a = make_assignment("", "main");
        let ws = prepare_workspace(&dir.join("repos"), &ws_root, &a).unwrap();
        assert!(!ws.is_git);
        assert!(ws.path.exists());
        assert!(finalize_workspace(&ws, "n@x").unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn worktree_commit_and_patch() {
        let dir = std::env::temp_dir().join(format!("ag-git-{}", uuid::Uuid::new_v4()));
        let origin = dir.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        sh(Some(&origin), "git init -q -b main").unwrap();
        std::fs::write(origin.join("base.txt"), "base").unwrap();
        sh(Some(&origin), "git add -A").unwrap();
        sh(
            Some(&origin),
            "git -c user.name=t -c user.email=t@x commit -q -m init",
        )
        .unwrap();

        let a = make_assignment(origin.to_str().unwrap(), "main");
        let ws = prepare_workspace(&dir.join("repos"), &dir.join("ws"), &a).unwrap();
        assert!(ws.is_git);
        // Agent writes a new file in the worktree.
        std::fs::write(ws.path.join("new.txt"), "hello").unwrap();

        let sha = finalize_workspace(&ws, "agent@agentgrid").unwrap();
        assert!(sha.is_some());
        let patch = std::fs::read_to_string(ws.path.join("changes.patch")).unwrap();
        assert!(patch.contains("new.txt"), "patch missing new file: {patch}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
