#[macro_use]
extern crate lazy_static;

use std::{
    ffi::OsString,
    fs::{self, Permissions},
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
};

use database::Database;
use filetime::FileTime;
use git::Repository;
use git2::{Commit, ObjectType, TreeEntry, TreeWalkResult};
use structopt::StructOpt;
use tempfile::tempdir;

mod cvs;
mod database;
mod git;
mod state;

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
    let target: PathBuf = [tempdir.path().as_os_str(), &opt.target].iter().collect();
    log::trace!("target: {:?}", &target);
    fs::create_dir_all(&target)?;

    let mut state = state::Global::new(tempdir.path(), &opt.target);

    // We have to add the target directory to the CVS repository before we can
    // do anything.
    cvs_repo.add(&opt.target, false)?;

    for (i, oid) in commits.iter().enumerate() {
        let commit = repo.commit(oid)?;
        let mut commit_state = state::Commit::new();

        commit.tree()?.walk(
            git2::TreeWalkMode::PreOrder,
            |path, entry| match walk_tree_entry(
                path,
                entry,
                &commit,
                &mut state,
                &mut commit_state,
                &repo,
            ) {
                Ok(result) => result,
                Err(e) => {
                    log::error!(
                        "error walking entry with path {} and {:?}: {:?}",
                        path,
                        entry.name(),
                        e
                    );
                    TreeWalkResult::Abort
                }
            },
        )?;

        // Remove files that have been removed.
        cvs_repo.remove_multiple(
            state
                .remove_files_unseen_in_commit(&commit_state)
                .into_iter()
                .map(|file| file.cvs_relative_path()),
        )?;

        // Add files that have been added.
        cvs_repo.add_multiple(
            commit_state
                .iter_new_non_binary_files()
                .map(|file| file.cvs_relative_path()),
            false,
        )?;
        cvs_repo.add_multiple(
            commit_state
                .iter_new_binary_files()
                .map(|file| file.cvs_relative_path()),
            true,
        )?;

        // Actually commit.
        cvs_repo.commit(commit.message_raw_bytes())?;

        log::trace!("commit {}/{}: {}", i + 1, commits.len(), oid);
    }

    Ok(())
}

fn walk_tree_entry(
    path: &str,
    entry: &TreeEntry,
    commit: &Commit,
    state: &mut state::Global,
    commit_state: &mut state::Commit,
    repo: &Repository,
) -> anyhow::Result<TreeWalkResult> {
    let mut git_path = PathBuf::from(path);
    if let Some(name) = entry.name() {
        git_path.push(name);
    }
    let file = state.file(git_path);
    let absolute = file.absolute_path();

    match entry.kind() {
        Some(ObjectType::Blob) => {
            let oid = entry.id();
            let blob = repo.blob(&oid)?;

            // Figure out if we need to write this: does the blob OID match the
            // previously written OID for this file?
            let maybe_oid = state.get_oid(&file);
            match maybe_oid {
                Some(last_oid) if &oid == last_oid => {
                    // It does match, so we don't need to do anything.
                }
                _ => {
                    // We need to write the file, either because it doesn't
                    // exist or has new content.
                    fs::write(&absolute, blob.content())?;

                    // CVS uses the modification time, so let's set
                    // that.
                    let time = FileTime::from_unix_time(commit.time().seconds(), 0);
                    filetime::set_file_times(&absolute, time, time)?;

                    // The file may be executable, so let's check.
                    if (entry.filemode() & 0o111) != 0 {
                        let perm = fs::metadata(&absolute)?.permissions().mode() | 0o111;

                        fs::set_permissions(&absolute, Permissions::from_mode(perm))?;
                    }

                    // If it's a new file, we need to inform CVS.
                    if maybe_oid.is_none() {
                        commit_state.new_file(file.clone(), blob.is_binary());
                    }

                    // Finally, we'll store the OID that we just wrote to the
                    // filesystem.
                    state.save_oid(file.clone(), &oid);
                }
            };

            commit_state.seen_file(file);
            Ok(TreeWalkResult::Ok)
        }
        Some(ObjectType::Tree) => {
            if fs::metadata(&absolute).is_err() {
                fs::create_dir_all(absolute)?;

                // We do need to add the directory to the new file tracking for
                // this commit, because it has to be included in "cvs add".
                commit_state.new_file(file, false);
            }

            Ok(TreeWalkResult::Ok)
        }
        _ => {
            log::trace!("unknown kind: {:?}", entry.kind());
            Ok(TreeWalkResult::Skip)
        }
    }
}
