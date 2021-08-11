use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    hash::Hash,
    path::PathBuf,
    rc::Rc,
};

use git2::Oid;

#[derive(Debug)]
struct Environment {
    absolute_base: PathBuf,
    cvs_base: PathBuf,
}

#[derive(Debug)]
pub struct Global {
    environment: Rc<Environment>,
    known_files: HashMap<File, Oid>,
}

impl Global {
    pub fn new<P: Into<PathBuf>, OS: AsRef<OsStr>>(tempdir: P, cvs_base: OS) -> Self {
        Self {
            environment: Rc::new(Environment {
                absolute_base: tempdir.into(),
                cvs_base: cvs_base.as_ref().into(),
            }),
            known_files: HashMap::new(),
        }
    }

    pub fn file<P: AsRef<OsStr>>(&self, path: P) -> File {
        File {
            environment: self.environment.clone(),
            relative_path: path.as_ref().into(),
        }
    }

    pub fn get_oid(&self, file: &File) -> Option<&Oid> {
        self.known_files.get(file)
    }

    pub fn save_oid(&mut self, file: File, oid: &Oid) {
        self.known_files.insert(file, oid.clone());
    }

    pub fn remove_files_unseen_in_commit(&mut self, commit: &Commit) -> HashSet<File> {
        // This would be _much_ cleaner (and wouldn't require the clone) with
        // drain_filter(), but that's currently unstable.
        let mut removed = HashSet::new();
        self.known_files.retain(|file, _| {
            if !commit.seen.contains(file) {
                removed.insert(file.clone());
                false
            } else {
                true
            }
        });

        removed
    }
}

#[derive(Debug)]
pub struct Commit {
    // These are Vecs because order matters here: we walk the Git tree in
    // pre-order, which is important because we need directories before files
    // within their directories when running cvs add.
    binary: Vec<File>,
    non_binary: Vec<File>,

    // seen, however, is just used to figure out which files were removed in the
    // commit, and ordering is unimportant here. We do need to be able to easily
    // access individual elements, though, so a set is appropriate.
    seen: HashSet<File>,
}

impl Commit {
    pub fn new() -> Self {
        Self {
            binary: Vec::new(),
            non_binary: Vec::new(),
            seen: HashSet::new(),
        }
    }

    pub fn iter_new_binary_files(&self) -> impl Iterator<Item = &File> {
        self.binary.iter()
    }

    pub fn iter_new_non_binary_files(&self) -> impl Iterator<Item = &File> {
        self.non_binary.iter()
    }

    pub fn new_file(&mut self, file: File, binary: bool) {
        if binary {
            self.binary.push(file);
        } else {
            self.non_binary.push(file);
        }
    }

    pub fn seen_file(&mut self, file: File) {
        self.seen.insert(file);
    }
}

#[derive(Debug, Clone)]
pub struct File {
    environment: Rc<Environment>,
    relative_path: PathBuf,
}

impl File {
    pub fn absolute_path(&self) -> PathBuf {
        [
            &self.environment.absolute_base,
            &self.environment.cvs_base,
            &self.relative_path,
        ]
        .iter()
        .collect()
    }

    pub fn cvs_relative_path(&self) -> PathBuf {
        [&self.environment.cvs_base, &self.relative_path]
            .iter()
            .collect()
    }
}

impl Hash for File {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.relative_path.hash(state)
    }
}

impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.relative_path == other.relative_path
    }
}

impl Eq for File {}
