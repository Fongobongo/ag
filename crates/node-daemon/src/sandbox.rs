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

/// Prefix args + program to run `program ...` inside the configured sandbox,
/// rooted at `workdir`. Used by the legacy wrapper-binary spawn path (Stage
/// 11.2 / line 358): `SpawnRequest { bin: program,
/// sandbox_prefix_args }` then appends `--prompt <prompt>`. `None` → no
/// prefix (passthrough as before); `Docker` → `docker run --rm -i -v … -- <image>`
/// with `program` placed inside the container after `--`.
///
/// `sandbox_command` (for the ACP path) keeps returning the fullwrapped
/// `(program, args)` already including `program`; this variant splits them
/// because the legacy ExecutionBackend appends its own `--prompt` after the
/// prefix.
pub fn sandbox_prefix(
    kind: SandboxKind,
    workdir: &std::path::Path,
    program: &str,
) -> (String, Vec<String>) {
    match kind {
        SandboxKind::None => (program.to_string(), vec![]),
        SandboxKind::Docker => {
            let image =
                std::env::var("AGENTGRID_SANDBOX_IMAGE").unwrap_or_else(|_| "ubuntu:24.04".into());
            let prefix = vec![
                "run".into(),
                "--rm".into(),
                "-i".into(),
                "-v".into(),
                format!("{}:/ag", workdir.display()),
                "-w".into(),
                "/ag".into(),
                "--".into(),
                image,
                program.into(),
            ];
            ("docker".into(), prefix)
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

    #[test]
    fn none_prefix_passthrough() {
        // Stage 11.2 / line 358: no sandbox → identity bin, empty prefix.
        let (p, a) = sandbox_prefix(SandboxKind::None, std::path::Path::new("/w"), "adapter-x");
        assert_eq!(p, "adapter-x");
        assert!(a.is_empty());
    }

    #[test]
    fn docker_prefix_wraps_program() {
        // Legacy wrapper path: program runs inside the image after `--`, with
        // an empty `args` slot (ProcessBackend appends `--prompt` itself).
        std::env::set_var("AGENTGRID_SANDBOX_IMAGE", "img:1");
        let (p, a) = sandbox_prefix(
            SandboxKind::Docker,
            std::path::Path::new("/w"),
            "adapter-claude",
        );
        std::env::remove_var("AGENTGRID_SANDBOX_IMAGE");
        assert_eq!(p, "docker");
        assert_eq!(a[0], "run");
        assert!(a.contains(&"-v".to_string()));
        assert_eq!(a[a.len() - 2], "img:1");
        assert_eq!(a[a.len() - 1], "adapter-claude");
    }
}
