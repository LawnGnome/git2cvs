use std::{
    convert::TryFrom,
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
};

use subprocess::Exec;
use sysconf::SysconfVariable;
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

    pub fn add_multiple<I, OS>(&self, paths: I, binary: bool) -> anyhow::Result<()>
    where
        I: Iterator<Item = OS>,
        OS: AsRef<OsStr>,
    {
        let mut chunker =
            ArgChunker::new(|chunk| self.do_add_multiple(chunk, binary), *ARG_MAX - 12);

        for path in paths {
            chunker.push(path)?;
        }

        Ok(())
    }

    fn do_add_multiple(&self, paths: &Vec<OsString>, binary: bool) -> anyhow::Result<()> {
        let mut exec = self.cmd().arg("add");
        if binary {
            exec = exec.arg("-kb");
        }

        for path in paths {
            exec = exec.arg(path);
        }

        exec.log().join()?;
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

    pub fn remove_multiple<I, OS>(&self, paths: I) -> anyhow::Result<()>
    where
        I: Iterator<Item = OS>,
        OS: AsRef<OsStr>,
    {
        let mut chunker = ArgChunker::new(|chunk| self.do_remove_multiple(chunk), *ARG_MAX - 12);

        for path in paths {
            chunker.push(path)?;
        }

        Ok(())
    }

    fn do_remove_multiple(&self, paths: &Vec<OsString>) -> anyhow::Result<()> {
        let mut exec = self.cmd().arg("remove");

        for path in paths {
            exec = exec.arg(path);
        }

        exec.log().join()?;
        Ok(())
    }

    fn cmd(&self) -> Exec {
        Exec::cmd(&self.cvs).cwd(&self.cwd)
    }
}

struct ArgChunker<F: Fn(&Vec<OsString>) -> anyhow::Result<()>> {
    acc: Vec<OsString>,
    commit: F,
    limit: usize,
    size: usize,
}

impl<F: Fn(&Vec<OsString>) -> anyhow::Result<()>> ArgChunker<F> {
    fn new(commit: F, limit: usize) -> Self {
        Self {
            acc: Vec::new(),
            commit,
            limit,
            size: 0,
        }
    }

    fn do_commit(&mut self) -> anyhow::Result<()> {
        (self.commit)(&self.acc)?;

        self.size = 0;
        self.acc.clear();

        Ok(())
    }

    fn push<OS: AsRef<OsStr>>(&mut self, path: OS) -> anyhow::Result<()> {
        let owned = OsString::from(path.as_ref());

        if self.size + owned.len() > self.limit {
            self.do_commit()?;
        }

        self.size += owned.len();
        self.acc.push(owned);

        Ok(())
    }
}

impl<F: Fn(&Vec<OsString>) -> anyhow::Result<()>> Drop for ArgChunker<F> {
    fn drop(&mut self) {
        if self.acc.len() > 0 {
            self.do_commit().unwrap();
        }
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

lazy_static! {
    static ref ARG_MAX: usize =
        usize::try_from(sysconf::raw::sysconf(SysconfVariable::ScArgMax).unwrap()).unwrap();
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
