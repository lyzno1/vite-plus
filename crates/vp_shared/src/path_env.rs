//! PATH environment variable manipulation utilities.
//!
//! This module provides functions for prepending directories to the PATH
//! environment variable with various deduplication strategies.

use std::{collections::BTreeSet, env, ffi::OsString, io, path::Path};

use vt_path::AbsolutePath;

use crate::env_vars;

/// PATH and the tools whose real binary directories Vite+ has injected into it.
/// Keep both values together when preparing a child process environment.
#[derive(Debug, Clone)]
pub struct ToolPathEnv {
    path: OsString,
    tools: BTreeSet<String>,
}

impl ToolPathEnv {
    pub fn new(path: OsString, tools: &str) -> Self {
        Self {
            path,
            tools: tools.split(',').filter(|tool| !tool.is_empty()).map(str::to_owned).collect(),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            env::var_os("PATH").unwrap_or_default(),
            &env::var(env_vars::VP_PATH_INJECTED_TOOLS).unwrap_or_default(),
        )
    }

    pub fn contains(&self, tool: &str) -> bool {
        self.tools.contains(tool)
    }

    /// Only pass tools supplied by this directory, never the names of vp shims.
    /// Use `dedupe_anywhere` to retain an existing directory's PATH precedence;
    /// explicit version selection instead moves the directory to the front.
    pub fn prepend(
        &mut self,
        dir: impl AsRef<Path>,
        tools: &[&str],
        options: PrependOptions,
    ) -> io::Result<()> {
        let dir = dir.as_ref();
        if !options.dedupe_anywhere || !env::split_paths(&self.path).any(|path| path == dir) {
            let mut paths = vec![dir.to_path_buf()];
            paths.extend(env::split_paths(&self.path).filter(|path| path != dir));
            self.path = env::join_paths(paths)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        }
        self.tools.extend(tools.iter().map(|tool| (*tool).to_owned()));
        Ok(())
    }

    pub fn into_envs(self) -> [(&'static str, OsString); 2] {
        [
            ("PATH", self.path),
            (
                env_vars::VP_PATH_INJECTED_TOOLS,
                self.tools.into_iter().collect::<Vec<_>>().join(",").into(),
            ),
        ]
    }
}

/// Inject tools at existing process-wide PATH initialization boundaries.
/// Child-process-only callers should use `ToolPathEnv::into_envs` instead.
///
/// The caller must satisfy the same environment access requirements as
/// `prepend_to_path_env`.
pub fn prepend_tools_to_path_env(
    dir: &AbsolutePath,
    tools: &[&str],
    options: PrependOptions,
) -> io::Result<()> {
    let mut env = ToolPathEnv::from_env();
    env.prepend(dir, tools, options)?;
    for (key, value) in env.into_envs() {
        // SAFETY: Caller ensures exclusive environment access, as for PATH initialization.
        unsafe { std::env::set_var(key, value) };
    }
    Ok(())
}

/// Options for deduplication behavior when prepending to PATH.
#[derive(Debug, Clone, Copy, Default)]
pub struct PrependOptions {
    /// If `false`, only check if the directory is first in PATH (faster).
    /// If `true`, check if the directory exists anywhere in PATH.
    pub dedupe_anywhere: bool,
}

/// Result of a PATH prepend operation.
#[derive(Debug)]
pub enum PrependResult {
    /// The directory was prepended successfully.
    Prepended(OsString),
    /// The directory is already present in PATH (based on dedup strategy).
    AlreadyPresent,
    /// Failed to join paths (invalid characters in path).
    JoinError,
}

/// Format PATH with the given directory prepended.
///
/// This returns a new PATH value without modifying the environment.
/// Use this when you need to set PATH on a `Command` via `cmd.env()`.
///
/// # Arguments
/// * `dir` - The directory to prepend to PATH
/// * `options` - Deduplication options
///
/// # Returns
/// * `PrependResult::Prepended(new_path)` - The new PATH value with directory prepended
/// * `PrependResult::AlreadyPresent` - Directory already exists in PATH (based on options)
/// * `PrependResult::JoinError` - Failed to join paths
pub fn format_path_with_prepend(dir: impl AsRef<Path>, options: PrependOptions) -> PrependResult {
    let dir = dir.as_ref();
    let current_path = env::var_os("PATH").unwrap_or_default();
    let paths: Vec<_> = env::split_paths(&current_path).collect();

    // Check for duplicates based on strategy
    if options.dedupe_anywhere {
        if paths.iter().any(|p| p == dir) {
            return PrependResult::AlreadyPresent;
        }
    } else if let Some(first) = paths.first()
        && first == dir
    {
        return PrependResult::AlreadyPresent;
    }

    // Prepend the directory
    let mut new_paths = vec![dir.to_path_buf()];
    new_paths.extend(paths);

    match env::join_paths(new_paths) {
        Ok(new_path) => PrependResult::Prepended(new_path),
        Err(_) => PrependResult::JoinError,
    }
}

/// Prepend a directory to the global PATH environment variable.
///
/// This modifies the process environment using `std::env::set_var`.
///
/// # Safety
/// This function uses `unsafe` to call `std::env::set_var`, which is unsafe
/// in multi-threaded contexts. Only call this before spawning threads or
/// when you're certain no other threads are reading environment variables.
///
/// # Arguments
/// * `dir` - The directory to prepend to PATH
/// * `options` - Deduplication options
///
/// # Returns
/// * `true` if PATH was modified
/// * `false` if the directory was already present or join failed
#[must_use]
pub fn prepend_to_path_env(dir: &AbsolutePath, options: PrependOptions) -> bool {
    match format_path_with_prepend(dir.as_path(), options) {
        PrependResult::Prepended(new_path) => {
            // SAFETY: Caller ensures this is safe (single-threaded or before exec)
            unsafe { env::set_var("PATH", new_path) };
            true
        }
        PrependResult::AlreadyPresent | PrependResult::JoinError => false,
    }
}

/// Format PATH with the given directory prepended (simple version).
///
/// This always prepends without deduplication. Use it when a child process
/// needs an explicit PATH value without mutating the parent process.
///
/// # Arguments
/// * `bin_prefix` - The directory to prepend to PATH
///
/// # Returns
/// The new PATH value as a String
pub fn format_path_prepended(bin_prefix: impl AsRef<Path>) -> String {
    let mut paths = env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    paths.insert(0, bin_prefix.as_ref().to_path_buf());
    env::join_paths(paths).unwrap().to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn injected_tools_accumulate_without_changing_other_environments() {
        let original = ToolPathEnv::new(OsString::new(), "node,npm,npx,node");
        let mut child = original.clone();
        child.prepend("/pnpm/bin", &["pnpm", "pnpx"], PrependOptions::default()).unwrap();
        assert!(child.contains("node"));
        assert!(child.contains("pnpx"));
        assert!(!child.contains("npmx"));
        assert!(!original.contains("pnpm"));
        assert_eq!(child.into_envs()[1].1, "node,npm,npx,pnpm,pnpx");
    }

    #[test]
    fn reinjecting_a_directory_moves_it_ahead_of_other_versions() {
        let path = env::join_paths(["/system/bin", "/selected/bin", "/vp/bin"]).unwrap();
        let mut child = ToolPathEnv::new(path, "");
        child.prepend("/selected/bin", &["node"], PrependOptions::default()).unwrap();
        child.prepend("/selected/bin", &["npm", "npx"], PrependOptions::default()).unwrap();
        let envs = child.into_envs();
        let paths: Vec<_> = env::split_paths(&envs[0].1).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from("/selected/bin"),
                PathBuf::from("/system/bin"),
                PathBuf::from("/vp/bin")
            ]
        );
        assert_eq!(envs[1].1, "node,npm,npx");
    }

    #[test]
    fn recording_an_existing_runtime_preserves_package_manager_precedence() {
        let path = env::join_paths(["/npm/bin", "/node/bin", "/vp/bin"]).unwrap();
        let mut child = ToolPathEnv::new(path.clone(), "npm,npx");
        child.prepend("/node/bin", &["node"], PrependOptions { dedupe_anywhere: true }).unwrap();
        let envs = child.into_envs();
        assert_eq!(envs[0].1, path);
        assert_eq!(envs[1].1, "node,npm,npx");
    }

    #[test]
    fn failed_path_injection_does_not_record_tools_or_change_path() {
        let mut child = ToolPathEnv::new("/original/bin".into(), "node");
        let original = child.clone().into_envs();
        let invalid = if cfg!(windows) { "/invalid\"path" } else { "/invalid:path" };
        assert!(child.prepend(invalid, &["pnpm", "pnpx"], PrependOptions::default()).is_err());
        assert_eq!(child.into_envs(), original);
    }

    #[test]
    fn test_prepend_options_default() {
        let options = PrependOptions::default();
        assert!(!options.dedupe_anywhere);
    }

    #[test]
    fn test_format_path_prepended() {
        let result = format_path_prepended("/test/bin");
        assert!(result.starts_with("/test/bin"));
    }

    #[test]
    fn test_format_path_with_prepend_dedupe_first() {
        // With dedupe_anywhere = false, should check first element only
        let options = PrependOptions { dedupe_anywhere: false };
        let result = format_path_with_prepend(PathBuf::from("/new/path"), options);
        assert!(matches!(result, PrependResult::Prepended(_)));
    }

    #[test]
    fn test_format_path_with_prepend_dedupe_anywhere() {
        let options = PrependOptions { dedupe_anywhere: true };
        let result = format_path_with_prepend(PathBuf::from("/new/path"), options);
        assert!(matches!(result, PrependResult::Prepended(_)));
    }

    #[test]
    #[serial_test::serial]
    fn test_format_path_prepended_always_prepends() {
        // Even if the directory exists somewhere in PATH, it should be prepended
        let test_dir = "/test/node/bin";

        // Set PATH to include test_dir in the middle
        // SAFETY: This test runs in isolation
        unsafe {
            std::env::set_var("PATH", format!("/other/bin:{}:/another/bin", test_dir));
        }

        let result = format_path_prepended(test_dir);

        // Should start with test_dir regardless of existing PATH entries
        assert!(
            result.starts_with(test_dir),
            "Directory should always be first in PATH, got: {}",
            result
        );

        // Restore PATH
        unsafe {
            std::env::remove_var("PATH");
        }
    }
}
