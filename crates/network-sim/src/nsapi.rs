//! Namespace management APIs (Linux only)

#![cfg(target_os = "linux")]

use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::os::fd::OwnedFd;

use nix::sched::{setns, CloneFlags};

/// Represents a Linux network namespace by name
#[derive(Debug, Clone)]
pub struct Namespace {
    name: String,
}

impl Namespace {
    /// Construct a Namespace handle for an existing namespace name (no creation)
    pub fn from_existing(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Create a namespace (idempotent: succeeds if it already exists)
    pub async fn ensure(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let out = tokio::process::Command::new("ip")
            .args(["netns", "add", &name])
            .output()
            .await?;
        if !out.status.success() {
            // tolerate "File exists"
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !(stderr.contains("File exists")
                || stderr.contains("RTNETLINK answers: File exists"))
            {
                return Err(Error::other(format!(
                    "ip netns add {} failed: {}",
                    name, stderr
                )));
            }
        }
        Ok(Self { name })
    }

    /// Delete the namespace (best effort)
    pub async fn delete(self) -> Result<()> {
        let _ = tokio::process::Command::new("ip")
            .args(["netns", "del", &self.name])
            .status()
            .await?;
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Execute a command inside the namespace via `ip netns exec`
    pub async fn exec(&self, cmd: &str, args: &[&str]) -> Result<std::process::Output> {
        let mut full = vec!["netns", "exec", &self.name, cmd];
        let extra: Vec<&str> = args.to_vec();
        full.extend(extra);
        let out = tokio::process::Command::new("ip")
            .args(&full)
            .output()
            .await?;
        Ok(out)
    }

    /// Enter the namespace on the current thread; guard restores on drop
    pub fn enter(&self) -> Result<NamespaceGuard> {
        let orig = File::open("/proc/self/ns/net")?;
        let target = open_netns_file(&self.name)?;
        // switch to target
        setns(target, CloneFlags::CLONE_NEWNET).map_err(|e| Error::other(e.to_string()))?;
        Ok(NamespaceGuard {
            original_ns: orig.into(),
            current_ns_name: self.name.clone(),
        })
    }
}

fn open_netns_file(ns: &str) -> Result<File> {
    let candidates = [
        format!("/run/netns/{}", ns),
        format!("/var/run/netns/{}", ns),
    ];
    let mut last: Option<Error> = None;
    for p in candidates {
        match File::open(&p) {
            Ok(f) => return Ok(f),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| Error::new(ErrorKind::NotFound, "netns path not found")))
}

/// RAII guard for a thread being inside a namespace
pub struct NamespaceGuard {
    original_ns: OwnedFd,
    #[allow(dead_code)]
    current_ns_name: String,
}

impl Drop for NamespaceGuard {
    fn drop(&mut self) {
        // Switch back to original namespace; best-effort
        let _ = setns(&self.original_ns, CloneFlags::CLONE_NEWNET);
    }
}
