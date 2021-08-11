use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs,
    path::PathBuf,
};

use database::Database;
use filetime::FileTime;
use git::Repository;
use git2::Oid;
use structopt::StructOpt;
use tempfile::tempdir;

mod cvs;
mod database;
mod git;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(short, long, help = "the branch to push")]
    branch: String,

    #[structopt(long, default_value = "cvs", help = "cvs binary to use")]
    cvs: OsString,

    #[structopt(short, long, env = "CVSROOT", help = "CVSROOT")]
    cvsroot: OsString,

    #[structopt(short, long, help = "metadata database")]
    database: OsString,

    #[structopt(short, long, help = "git repository")]
    git: OsString,

    #[structopt(
        short,
        long,
        default_value = ".",
        help = "cvs module to check out, if any"
    )]
    module: String,

    #[structopt(short, long, help = "use a remote branch")]
    remote: bool,

    #[structopt(
        short,
        long,
        default_value = "src",
        help = "the target directory within the cvs checkout; can be . to write at the top level"
    )]
    target: OsString,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opt = Opt::from_args();
    let cvs_ctx = cvs::Context::new(&opt.cvs);
    let mut db = Database::open(&opt.database)?;
    let repo = Repository::open(&opt.git)?;

    let branch = match repo.branch(&opt.branch, opt.remote)? {
        Some(branch) => branch,
        None => anyhow::bail!("cannot find branch {}", &opt.branch),
    };

    if db.get_cvs_branch(&opt.branch)?.is_some() {
        anyhow::bail!("TODO: support updating existing branches");
    }
    let cvs_branch = cvs::sanitise_branch(&opt.branch);

    let commits = branch.linear_history()?;
    db.write_branch(&opt.branch, &cvs_branch, commits.iter())?;

    let tempdir = tempdir()?;
    let cvs_repo = cvs_ctx.checkout(&opt.cvsroot, &opt.module, tempdir.path())?;

    // Ensure we have a target directory.
    // TODO: handle an already existing directory.
    let mut target = PathBuf::new();
    target.push(tempdir.path());
    target.push(&opt.target);
    log::trace!("target: {:?}", &target);
    fs::create_dir_all(&target)?;
    cvs_repo.add(&opt.target, false)?;

    let mut files: HashMap<OsString, Oid> = HashMap::new();
    for (i, oid) in commits.iter().enumerate() {
        let commit = repo.commit(oid)?;
        let mut files_seen = HashSet::new();

        commit
            .tree()?
            .walk(git2::TreeWalkMode::PreOrder, |path, entry| {
                // TODO: refactor into a real function we can Try on.

                let mut git_path = PathBuf::from(path);
                if let Some(name) = entry.name() {
                    git_path.push(name);
                }

                let mut cvs_path = PathBuf::from(&opt.target);
                cvs_path.push(&git_path);

                let mut absolute = target.clone();
                absolute.push(&git_path);

                match entry.kind() {
                    Some(git2::ObjectType::Blob) => {
                        let oid = entry.id();
                        let blob = repo.blob(&oid).unwrap();

                        // Figure out if we need to write this.
                        match files.get(cvs_path.as_os_str().into()) {
                            Some(last_oid) if &oid == last_oid => {
                                // No action required.
                            }
                            _ => {
                                // We need to write the file.
                                fs::write(&absolute, blob.content()).unwrap();

                                // CVS uses the modification time, so let's set
                                // that.
                                let time = FileTime::from_unix_time(commit.time().seconds(), 0);
                                filetime::set_file_times(&absolute, time, time).unwrap();

                                cvs_repo
                                    .add(cvs_path.as_os_str(), blob.is_binary())
                                    .unwrap();

                                files.insert(cvs_path.as_os_str().into(), oid);
                            }
                        };

                        files_seen.insert(cvs_path.into_os_string());

                        git2::TreeWalkResult::Ok
                    }
                    Some(git2::ObjectType::Tree) => {
                        if fs::metadata(&absolute).is_err() {
                            fs::create_dir_all(absolute).unwrap();
                            cvs_repo.add(cvs_path.as_os_str(), false).unwrap();
                        }

                        git2::TreeWalkResult::Ok
                    }
                    _ => {
                        log::trace!("unknown kind: {:?}", entry.kind());
                        git2::TreeWalkResult::Skip
                    }
                }
            })?;

        // Remove files that have been removed.
        let mut files_removed = HashSet::new();
        for file in files.keys() {
            if !files_seen.contains(file) {
                log::trace!("removing file {:?}", &file);

                let mut path = PathBuf::new();
                path.push(tempdir.path());
                path.push(file);
                fs::remove_file(path)?;

                cvs_repo.remove(file.as_os_str())?;
                files_removed.insert(file.clone());
            }
        }
        files.retain(|file, _| !files_removed.contains(file));

        // Actually commit.
        cvs_repo.commit(commit.message_raw_bytes())?;

        log::trace!("commit {}/{}: {}", i + 1, commits.len(), oid);
    }

    Ok(())
}
