# git2cvs

git2cvs can convert a branch of a Git repository into a CVS repository.

## Building

git2cvs is a fairly normal Rust project, albeit with a couple of (fairly
mundane) shared library dependencies. Assuming you have the required libraries,
then you can build a release build with `cargo build --release`, which will give
you a binary in `target/release/git2cvs`.

### Build requirements

- Rust 1.53+ (it may work on older versions; I haven't checked)
- libgit2
- libsqlite3

### Runtime requirements

- CVS

## Usage

The key thing you'll need is a CVSROOT that's ready to receive a directory. You
can create a new one in `/tmp/cvsroot` with the following:

```sh
cvs -d :local:/tmp/cvsroot init
```

Alternatively, you can use a remote CVSROOT. It'll probably work just fine.

Once you have a CVSROOT, you can convert a Git branch into a new directory in
that root with:

```sh
git2cvs -r PATH_TO_GIT_REPO -b GIT_BRANCH -c CVSROOT -d DATABASE_PATH
```

The database path points to an SQLite 3 database that contains some useful
branch and commit tracking metadata, which is currently tragically unused, but
will be once multiple branches and incremental branch updates are supported.

## FAQ

(not that anyone has asked questions yet, but I can see them coming)

### WHY.

Because CVS is clearly the futu... no, I can't do it. But CVS support _is_
necessary for certain source code indexing tools, and it's hard to find good
sample repositories nowadays of the right size and scale, so here we are.

(I probably wouldn't have written this if OpenBSD's CVS repository wasn't broken
when using any CVS client other than OpenBSD's fork of CVS.)

### How do I speed this up?

Yeah, it's slow.

The main problem right now appears to be `cvs commit`. I suspect that operating
against a pserver instead of a local CVSROOT would probably be faster, but
haven't tested it yet. I suspect that implementing a CVS client library in Rust
and avoiding shelling out would be faster still.

But, fundamentally, CVS is _slow_. I'd honestly forgotten how slow. There are a
lot of round trips, and it's very 20th century. To some extent, it's just always
going to be slow.

(Well, without bypassing CVS entirely and writing directly to a CVSROOT,
which... yeah, is tempting, but clearly I don't have time to reverse engineer
the format of a valid CVSROOT.)

### Are remote Git repositories directly supported?

If libgit2 can handle it, probably? But you might want to use a local clone just
for performance reasons.

### Are there known bugs?

Yep!

Oh, you wanted details? Specifically, files that become binary will probably do
bad things, and commit timestamps are completely busted right now.

### What's planned for the future?

Support for converting multiple branches at once would be obviously nice: it's
_real_ expensive to try to put that together after the fact, because you have to
re-checkout the state of the CVS tree at that point. Unfortunately, creating a
sensible DAG to calculate that at runtime was beyond my 11 pm Rust skills.

Incremental updates would also be nice, since then you could run this repeatedly
over a repository to keep the CVS repository up to date.
