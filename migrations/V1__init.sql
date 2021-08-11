CREATE TABLE IF NOT EXISTS branch_mappings (
    git TEXT NOT NULL PRIMARY KEY,
    cvs TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS commit_branches (
    oid TEXT NOT NULL,
    branch TEXT NOT NULL,
    branch_index INTEGER NOT NULL,
    PRIMARY KEY (oid, branch)
);