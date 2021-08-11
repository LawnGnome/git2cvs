use std::{collections::VecDeque, path::Path};

use git2::{ErrorCode, Oid};

pub struct Repository {
    repo: git2::Repository,
}

impl Repository {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        Ok(Self {
            repo: git2::Repository::open(path)?,
        })
    }

    pub fn blob(&self, oid: &Oid) -> anyhow::Result<git2::Blob> {
        Ok(self.repo.find_blob(*oid)?)
    }

    pub fn branch(&self, name: &str, remote: bool) -> anyhow::Result<Option<Branch>> {
        match self.repo.find_branch(
            name,
            if remote {
                git2::BranchType::Remote
            } else {
                git2::BranchType::Local
            },
        ) {
            Ok(branch) => Ok(Some(Branch { branch })),
            Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn commit(&self, oid: &Oid) -> anyhow::Result<git2::Commit> {
        Ok(self.repo.find_commit(*oid)?)
    }
}

pub struct Branch<'repo> {
    branch: git2::Branch<'repo>,
}

impl Branch<'_> {
    pub fn linear_history(&self) -> anyhow::Result<VecDeque<Oid>> {
        // We'll build a linear history here: a set of commit OIDs that, in
        // order, will provide a plausible representation of the history of the
        // branch. We'll do that by only following the first parent, and
        // essentially treating merge commits as simple squash commits.
        let mut commits = VecDeque::new();

        let mut commit = self.branch.get().peel_to_commit()?;
        loop {
            let next = commit.parent(0);
            commits.push_front(commit.id());

            match next {
                Ok(parent) => {
                    commit = parent;
                }
                Err(e) if e.code() == ErrorCode::NotFound => {
                    break;
                }
                Err(e) => {
                    return Err(e.into());
                }
            };
        }

        Ok(commits)
    }

    pub fn name(&self) -> anyhow::Result<&str> {
        // We'll unwrap because process_branches filters down to only branches
        // that have names.
        Ok(self.branch.name()?.unwrap())
    }
}
