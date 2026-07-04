use git2::Repository;
use serde::Serialize;
use crate::errors::AppError;

#[derive(Debug, Serialize, Clone)]
pub struct RepoInfo {
    pub branch: String,
    pub commit_hash: String,
    pub commit_message: String,
    pub is_dirty: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: String, // "added", "modified", "deleted", "renamed"
}

pub fn get_repo_info(path: &str) -> Result<RepoInfo, AppError> {
    let repo = Repository::open(path)?;

    let head = repo.head()?;
    let branch = head
        .shorthand()
        .unwrap_or("HEAD")
        .to_string();

    let commit = head.peel_to_commit()?;
    let commit_hash_full = commit.id().to_string();
    let commit_hash = if commit_hash_full.len() >= 8 {
        commit_hash_full[..8].to_string()
    } else {
        commit_hash_full
    };
    let commit_message = commit
        .message()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();

    let statuses = repo.statuses(None)?;
    let is_dirty = statuses.iter().any(|s| {
        s.status() != git2::Status::CURRENT && s.status() != git2::Status::IGNORED
    });

    Ok(RepoInfo {
        branch,
        commit_hash,
        commit_message,
        is_dirty,
    })
}

pub fn get_changed_files(path: &str, since_commit: Option<&str>) -> Result<Vec<ChangedFile>, AppError> {
    let repo = Repository::open(path)?;
    let mut changed = Vec::new();

    if let Some(commit_hash) = since_commit {
        let oid = git2::Oid::from_str(commit_hash)?;
        let old_commit = repo.find_commit(oid)?;
        let old_tree = old_commit.tree()?;

        let head = repo.head()?;
        let new_commit = head.peel_to_commit()?;
        let new_tree = new_commit.tree()?;

        let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

        diff.foreach(
            &mut |delta, _| {
                let path = delta.new_file().path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                let status = match delta.status() {
                    git2::Delta::Added => "added",
                    git2::Delta::Deleted => "deleted",
                    git2::Delta::Modified => "modified",
                    git2::Delta::Renamed => "renamed",
                    _ => "modified",
                };
                changed.push(ChangedFile {
                    path,
                    status: status.to_string(),
                });
                true
            },
            None, None, None,
        )?;
    } else {
        // No base commit — show working directory changes
        let statuses = repo.statuses(None)?;
        for entry in statuses.iter() {
            if let Ok(path) = entry.path() {
                let st = entry.status();
                let status = if st.contains(git2::Status::WT_NEW) || st.contains(git2::Status::INDEX_NEW) {
                    "added"
                } else if st.contains(git2::Status::WT_DELETED) || st.contains(git2::Status::INDEX_DELETED) {
                    "deleted"
                } else if st.contains(git2::Status::WT_RENAMED) || st.contains(git2::Status::INDEX_RENAMED) {
                    "renamed"
                } else {
                    "modified"
                };
                changed.push(ChangedFile {
                    path: path.to_string(),
                    status: status.to_string(),
                });
            }
        }
    }

    Ok(changed)
}
