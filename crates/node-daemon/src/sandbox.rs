//! Agent isolation (idea: sandcastle-style sandbox abstraction).
//!
//! A `Sandbox` wraps the command the node would run the agent as, so an agent
//! can be confined to a container (Docker/Podman/microVM) instead of sharing
//! the node's full environment. The default `NoSandbox` runs the agent
//! directly in the worktree (legacy behavior). Configured via
//! `AGENTGRID_SANDBOX` (`none` | `docker`) and `AGENTGRID_SANDBOX_IMAGE`.
//!
//! `sandbox_command` returns the `(program, args)` to spawn: either the raw
//! command, or a `docker run --rm -i -v <workdir>:/ag -w /ag <image> -- <cmd>`
//! prefix. Both the wrapper path and the ACP path route through it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxKind {
    None,
    Docker,
}

impl SandboxKind {
    pub fn from_env() -> Self {
        match std::env::var("AGENTGRID_SANDBOX")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "docker" | "podman" => SandboxKind::Docker,
            _ => SandboxKind::None,
        }
    }
}

/// Wrap `(program, args)` for the configured sandbox, rooted at `workdir`.
/// `None` returns the command unchanged. `Docker` prefixes with
/// `docker run --rm -i -v <workdir>:/ag -w /ag <image> --`.
// ponytail: binds the whole workdir read-write; a stricter mount policy
// (read-only + separate artifact dir) is the upgrade path if an agent needs
// less FS access.
pub fn sandbox_command(
    kind: SandboxKind,
    program: &str,
    args: &[String],
    workdir: &std::path::Path,
) -> (String, Vec<String>) {
    match kind {
        SandboxKind::None => (program.to_string(), args.to_vec()),
        SandboxKind::Docker => {
            let image = std::env::var("AGENTGRID_SANDBOX_IMAGE")
                .unwrap_or_else(|_| "ubuntu:24.04".to_string());
            let mut out = vec![
                "run".to_string(),
                "--rm".to_string(),
                "-i".to_string(),
                "-v".to_string(),
                format!("{}:/ag", workdir.display()),
                "-w".to_string(),
                "/ag".to_string(),
                "--".to_string(),
            ];
            out.push(image);
            out.push(program.to_string());
            out.extend(args.iter().cloned());
            ("docker".to_string(), out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_passthrough() {
        let (p, a) = sandbox_command(
            SandboxKind::None,
            "claude",
            &["--acp".into()],
            std::path::Path::new("/w"),
        );
        assert_eq!(p, "claude");
        assert_eq!(a, vec!["--acp"]);
    }

    #[test]
    fn docker_wraps_command() {
        std::env::set_var("AGENTGRID_SANDBOX_IMAGE", "img:1");
        let (p, a) = sandbox_command(
            SandboxKind::Docker,
            "claude",
            &["--acp".into()],
            std::path::Path::new("/w"),
        );
        assert_eq!(p, "docker");
        assert_eq!(a[0], "run");
        assert!(a.contains(&"-v".to_string()));
        assert_eq!(a[a.len() - 3], "img:1");
        assert_eq!(a[a.len() - 2], "claude");
        assert_eq!(a[a.len() - 1], "--acp");
        std::env::remove_var("AGENTGRID_SANDBOX_IMAGE");
    }
}
