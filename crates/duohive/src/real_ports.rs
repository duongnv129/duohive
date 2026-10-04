//! Real implementations of the core ports: wall clock, random tokens, directory probe.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use duohive_core::launch::Token;
use duohive_core::ports::{Clock, Ids};
use duohive_core::time::Millis;
use duohive_core::validate::{DirProbe, DirStatus};

/// Bytes of entropy in a per-launch token (D1).
const TOKEN_BYTES: usize = 32;

/// The system wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Millis {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        Millis(ms)
    }
}

/// Per-launch tokens from the OS entropy source.
pub struct RandomIds;

impl Ids for RandomIds {
    /// A fresh 64-character hex token. If the OS has no entropy source this
    /// aborts the process (no terminal restore), rather than return a weak token.
    fn new_token(&mut self) -> Token {
        let mut bytes = [0u8; TOKEN_BYTES];
        if let Err(e) = getrandom::fill(&mut bytes) {
            // A guessable token would break hook authentication: never fall back.
            tracing::error!("no OS entropy source, aborting: {e}");
            std::process::abort();
        }
        let mut hex = String::with_capacity(TOKEN_BYTES * 2);
        for b in bytes {
            hex.push(char::from(HEX[usize::from(b >> 4)]));
            hex.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
        Token(hex)
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Stat-only directory probe. It walks up the ancestors, so a directory inside
/// a repository is a `GitRepo`: no `git` process, no threads. A stat on a hung
/// network mount can block the caller; that risk is accepted (see Task 26 report).
pub struct FsDirProbe;

impl DirProbe for FsDirProbe {
    fn probe(&self, dir: &Path) -> DirStatus {
        if !dir.is_dir() {
            return DirStatus::Missing;
        }
        // `.git` is a directory in a clone and a file in a worktree or submodule.
        if dir.ancestors().any(|d| d.join(".git").exists()) {
            DirStatus::GitRepo
        } else {
            DirStatus::Found
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use duohive_core::ports::{Clock, Ids};
    use duohive_core::validate::{DirProbe, DirStatus};

    use super::*;

    #[test]
    fn random_ids_are_64_lowercase_hex() {
        let t = RandomIds.new_token().0;
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')), "{t}");
        assert_ne!(t, RandomIds.new_token().0);
    }

    #[test]
    fn system_clock_is_unix_ms() {
        let sys = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(SystemClock.now().0.abs_diff(sys) < 1_000);
    }

    #[test]
    fn dir_probe_detects_git_repos_and_worktrees() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        fs::create_dir_all(p.join("repo/.git")).unwrap();
        fs::create_dir_all(p.join("wt")).unwrap();
        fs::write(p.join("wt/.git"), "gitdir: /elsewhere\n").unwrap();
        fs::create_dir_all(p.join("plain")).unwrap();
        fs::write(p.join("file.txt"), "x").unwrap();
        assert_eq!(FsDirProbe.probe(&p.join("repo")), DirStatus::GitRepo);
        assert_eq!(FsDirProbe.probe(&p.join("wt")), DirStatus::GitRepo);
        // Walk-up means a `.git` above the temp dir turns `plain` into a GitRepo.
        if !p.ancestors().any(|d| d.join(".git").exists()) {
            assert_eq!(FsDirProbe.probe(&p.join("plain")), DirStatus::Found);
        }
        assert_eq!(FsDirProbe.probe(&p.join("file.txt")), DirStatus::Missing);
        assert_eq!(FsDirProbe.probe(&p.join("none")), DirStatus::Missing);
    }
}
