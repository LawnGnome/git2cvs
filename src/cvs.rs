use std::{
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
};

use subprocess::Exec;
use tempfile::NamedTempFile;

trait ExecExt {
    fn log(self) -> Self;
}

impl ExecExt for Exec {
    fn log(self) -> Self {
        log::trace!("{:?}", self.to_cmdline_lossy());
        self
    }
}

#[derive(Debug, Clone)]
pub struct Context {
    cvs: OsString,
}

impl Context {
    pub fn new(cvs: &OsStr) -> Self {
        Self { cvs: cvs.into() }
    }

    pub fn checkout<P: AsRef<Path>>(
        &self,
        cvsroot: &OsStr,
        module: &str,
        target: P,
    ) -> anyhow::Result<Repository> {
        Exec::cmd(&self.cvs)
            .arg("-d")
            .arg(cvsroot)
            .arg("checkout")
            .arg("-d")
            .arg(target.as_ref())
            .arg("-R")
            .arg(module)
            .log()
            .join()?;

        let mut cwd = PathBuf::new();
        cwd.push(target);

        Ok(Repository {
            cvs: self.cvs.clone(),
            cwd,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Repository {
    cvs: OsString,
    cwd: PathBuf,
}

impl Repository {
    pub fn add(&self, path: &OsStr, binary: bool) -> anyhow::Result<()> {
        let mut exec = self.cmd().arg("add");

        if binary {
            exec = exec.arg("-kb");
        }

        exec.arg(path).log().join()?;

        Ok(())
    }

    pub fn commit(&self, message: &[u8]) -> anyhow::Result<()> {
        let mut msgfile = NamedTempFile::new()?;
        msgfile.write_all(message)?;
        msgfile.flush()?;

        self.cmd()
            .arg("commit")
            .arg("-F")
            .arg(msgfile.path())
            .log()
            .join()?;

        Ok(())
    }

    pub fn remove(&self, path: &OsStr) -> anyhow::Result<()> {
        self.cmd().arg("remove").arg(path).log().join()?;

        Ok(())
    }

    fn cmd(&self) -> Exec {
        Exec::cmd(&self.cvs).cwd(&self.cwd)
    }
}

pub fn sanitise_branch(name: &str) -> String {
    let mut out = String::new();

    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push_str(&format!("__u{:06x}", u32::from(c)));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitise_branch() {
        assert_eq!("foo", sanitise_branch("foo"));
        assert_eq!("foo-Bar_quux0", sanitise_branch("foo-Bar_quux0"));
        assert_eq!("__u000020", sanitise_branch(" "));
    }
}
