use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub created_at: String,
    pub reason: String,
    pub files: Vec<CheckpointFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointFile {
    pub path: String,
    pub existed: bool,
    pub content: Option<String>,
}

pub struct CheckpointStore {
    root: PathBuf,
}

impl CheckpointStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> PathBuf {
        PathBuf::from(".").join(".tengu").join("checkpoints")
    }

    pub fn create_for_paths(&self, paths: &[PathBuf], reason: &str) -> Result<Checkpoint> {
        let files = paths
            .iter()
            .map(|path| snapshot_file(path))
            .collect::<Result<Vec<_>>>()?;
        self.save_checkpoint(files, reason)
    }

    fn save_checkpoint(&self, files: Vec<CheckpointFile>, reason: &str) -> Result<Checkpoint> {
        fs::create_dir_all(&self.root)?;
        let id = checkpoint_id();
        let checkpoint = Checkpoint {
            id: id.clone(),
            created_at: Utc::now().to_rfc3339(),
            reason: reason.to_string(),
            files,
        };
        let path = self.checkpoint_path(&id);
        fs::write(path, serde_json::to_string_pretty(&checkpoint)?)?;
        Ok(checkpoint)
    }

    pub fn list(&self) -> Result<Vec<Checkpoint>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut checkpoints = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(checkpoint) = serde_json::from_str::<Checkpoint>(&data) {
                    checkpoints.push(checkpoint);
                }
            }
        }
        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(checkpoints)
    }

    pub fn latest(&self) -> Result<Option<Checkpoint>> {
        Ok(self.list()?.into_iter().next())
    }

    pub fn load(&self, id: &str) -> Result<Checkpoint> {
        let path = self.checkpoint_path(id);
        let data = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn restore(&self, id: &str) -> Result<Checkpoint> {
        let checkpoint = self.load(id)?;
        restore_checkpoint(&checkpoint)?;
        Ok(checkpoint)
    }

    pub fn restore_latest(&self) -> Result<Option<Checkpoint>> {
        let Some(checkpoint) = self.latest()? else {
            return Ok(None);
        };
        restore_checkpoint(&checkpoint)?;
        Ok(Some(checkpoint))
    }

    fn checkpoint_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

pub fn format_checkpoint_list(checkpoints: &[Checkpoint]) -> String {
    if checkpoints.is_empty() {
        return "no checkpoints".to_string();
    }
    checkpoints
        .iter()
        .map(|checkpoint| {
            format!(
                "{} {} files={} reason={}",
                checkpoint.id,
                checkpoint.created_at,
                checkpoint.files.len(),
                checkpoint.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_checkpoint_diff(checkpoint: &Checkpoint) -> String {
    if checkpoint.files.is_empty() {
        return "checkpoint has no files".to_string();
    }
    checkpoint
        .files
        .iter()
        .map(|file| {
            let current = fs::read_to_string(&file.path).ok();
            let before = file.content.as_deref().unwrap_or("");
            let after = current.as_deref().unwrap_or("");
            crate::tools::build_diff(Path::new(&file.path), before, after)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn snapshot_file(path: &Path) -> Result<CheckpointFile> {
    let existed = path.exists();
    let content = if existed {
        Some(fs::read_to_string(path).map_err(|err| {
            anyhow!(
                "checkpoint cannot snapshot non-UTF-8 or unreadable file {}: {}",
                path.display(),
                err
            )
        })?)
    } else {
        None
    };
    Ok(CheckpointFile {
        path: path.display().to_string(),
        existed,
        content,
    })
}

fn restore_checkpoint(checkpoint: &Checkpoint) -> Result<()> {
    for file in &checkpoint.files {
        let path = Path::new(&file.path);
        if file.existed {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(
                path,
                file.content
                    .as_ref()
                    .ok_or_else(|| anyhow!("checkpoint missing content for {}", file.path))?,
            )?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn checkpoint_id() -> String {
    format!(
        "{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        uuid::Uuid::new_v4().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_restores_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join(".tengu/checkpoints"));
        let file = dir.path().join("notes.txt");
        fs::write(&file, "before").unwrap();

        let checkpoint = store
            .create_for_paths(&[file.clone()], "test checkpoint")
            .unwrap();
        fs::write(&file, "after").unwrap();

        store.restore(&checkpoint.id).unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), "before");
    }

    #[test]
    fn restore_removes_file_that_did_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join(".tengu/checkpoints"));
        let file = dir.path().join("created.txt");

        let checkpoint = store
            .create_for_paths(&[file.clone()], "before create")
            .unwrap();
        fs::write(&file, "created").unwrap();

        store.restore(&checkpoint.id).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn lists_latest_checkpoint_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join(".tengu/checkpoints"));
        let file = dir.path().join("notes.txt");
        fs::write(&file, "one").unwrap();
        let first = store.create_for_paths(&[file.clone()], "first").unwrap();
        fs::write(&file, "two").unwrap();
        let second = store.create_for_paths(&[file], "second").unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, second.id);
        assert_eq!(listed[1].id, first.id);
    }

    #[test]
    fn handles_empty_checkpoint_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join(".tengu/checkpoints"));

        assert!(store.list().unwrap().is_empty());
        assert!(store.latest().unwrap().is_none());
        assert!(store.restore_latest().unwrap().is_none());
        assert_eq!(format_checkpoint_list(&[]), "no checkpoints");
    }

    #[test]
    fn skips_invalid_checkpoint_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".tengu/checkpoints");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("bad.json"), "{not-json").unwrap();
        fs::write(root.join("ignored.txt"), "{}").unwrap();
        let store = CheckpointStore::new(root);

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn formats_checkpoint_diff_for_empty_and_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, "after\n").unwrap();
        let empty = Checkpoint {
            id: "cp-empty".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: "empty".to_string(),
            files: Vec::new(),
        };
        assert_eq!(format_checkpoint_diff(&empty), "checkpoint has no files");

        let checkpoint = Checkpoint {
            id: "cp-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: "before edit".to_string(),
            files: vec![CheckpointFile {
                path: file.display().to_string(),
                existed: true,
                content: Some("before\n".to_string()),
            }],
        };

        let diff = format_checkpoint_diff(&checkpoint);
        assert!(diff.contains("-before"));
        assert!(diff.contains("+after"));
    }

    #[test]
    fn restore_recreates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested/notes.txt");
        let checkpoint = Checkpoint {
            id: "cp-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: "restore nested".to_string(),
            files: vec![CheckpointFile {
                path: file.display().to_string(),
                existed: true,
                content: Some("restored".to_string()),
            }],
        };

        restore_checkpoint(&checkpoint).unwrap();
        assert_eq!(fs::read_to_string(file).unwrap(), "restored");
    }

    #[test]
    fn restore_latest_and_format_list_cover_present_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path().join(".tengu/checkpoints"));
        let file = dir.path().join("notes.txt");
        fs::write(&file, "before").unwrap();
        let checkpoint = store.create_for_paths(&[file.clone()], "latest").unwrap();
        fs::write(&file, "after").unwrap();

        let restored = store.restore_latest().unwrap().unwrap();
        assert_eq!(restored.id, checkpoint.id);
        assert_eq!(fs::read_to_string(file).unwrap(), "before");
        let formatted = format_checkpoint_list(&[checkpoint]);
        assert!(formatted.contains("files=1"));
        assert!(formatted.contains("reason=latest"));
    }

    #[test]
    fn restore_missing_file_snapshot_when_file_is_still_absent_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("never-created.txt");
        let checkpoint = Checkpoint {
            id: "cp-absent".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: "absent".to_string(),
            files: vec![CheckpointFile {
                path: file.display().to_string(),
                existed: false,
                content: None,
            }],
        };

        restore_checkpoint(&checkpoint).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn restore_existing_snapshot_requires_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("missing-content.txt");
        let checkpoint = Checkpoint {
            id: "cp-invalid".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            reason: "invalid".to_string(),
            files: vec![CheckpointFile {
                path: file.display().to_string(),
                existed: true,
                content: None,
            }],
        };

        let error = restore_checkpoint(&checkpoint).unwrap_err().to_string();
        assert!(error.contains("missing content"));
    }
}
