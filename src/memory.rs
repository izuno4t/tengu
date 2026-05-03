use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MemoryFile {
    entries: Vec<MemoryEntry>,
}

pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        PathBuf::from(".").join(".tengu").join("memory.json")
    }

    pub fn add(&self, content: &str) -> Result<MemoryEntry> {
        let content = content.trim();
        if content.is_empty() {
            return Err(anyhow!("memory content is empty"));
        }
        let mut file = self.load_file()?;
        let now = Utc::now().to_rfc3339();
        let entry = MemoryEntry {
            id: memory_id(),
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        file.entries.push(entry.clone());
        self.save_file(&file)?;
        Ok(entry)
    }

    pub fn list(&self) -> Result<Vec<MemoryEntry>> {
        let mut entries = self.load_file()?.entries;
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(entries)
    }

    pub fn search(&self, query: &str) -> Result<Vec<MemoryEntry>> {
        let query = query.trim();
        if query.is_empty() {
            return self.list();
        }
        let tokens = tokenize(query);
        let mut scored = self
            .load_file()?
            .entries
            .into_iter()
            .filter_map(|entry| {
                let score = memory_score(&entry.content, &tokens);
                (score > 0).then_some((score, entry))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(scored.into_iter().map(|(_, entry)| entry).collect())
    }

    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut file = self.load_file()?;
        let original_len = file.entries.len();
        file.entries.retain(|entry| entry.id != id);
        let removed = file.entries.len() != original_len;
        if removed {
            self.save_file(&file)?;
        }
        Ok(removed)
    }

    fn load_file(&self) -> Result<MemoryFile> {
        if !self.path.exists() {
            return Ok(MemoryFile::default());
        }
        let data = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&data)?)
    }

    fn save_file(&self, file: &MemoryFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(file)?)?;
        Ok(())
    }
}

pub fn format_memory_entries(entries: &[MemoryEntry]) -> String {
    if entries.is_empty() {
        return "no memory entries".to_string();
    }
    entries
        .iter()
        .map(|entry| format!("{} {} {}", entry.id, entry.updated_at, entry.content))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_memory_context(entries: &[MemoryEntry], max_entries: usize) -> Option<String> {
    if entries.is_empty() || max_entries == 0 {
        return None;
    }
    let body = entries
        .iter()
        .take(max_entries)
        .map(|entry| format!("- [{}] {}", entry.id, entry.content))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!("Project memory:\n{body}"))
}

fn memory_score(content: &str, tokens: &[String]) -> usize {
    if tokens.is_empty() {
        return 1;
    }
    let haystack = content.to_ascii_lowercase();
    tokens
        .iter()
        .filter(|token| haystack.contains(token.as_str()))
        .count()
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

fn memory_id() -> String {
    format!(
        "mem-{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        uuid::Uuid::new_v4().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_lists_searches_and_removes_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join(".tengu/memory.json"));

        let rust = store.add("Project uses Rust and cargo test").unwrap();
        store.add("Deploy through GitHub Actions").unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|entry| entry.content.contains("Rust")));

        let found = store.search("rust cargo").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, rust.id);

        assert!(store.remove(&rust.id).unwrap());
        assert!(!store.remove(&rust.id).unwrap());
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn formats_context_for_prompt_insertion() {
        let entry = MemoryEntry {
            id: "mem-1".to_string(),
            content: "Use cargo test before completion".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let context = format_memory_context(&[entry], 3).unwrap();
        assert!(context.contains("Project memory:"));
        assert!(context.contains("Use cargo test"));
    }

    #[test]
    fn handles_empty_and_missing_memory_states() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join(".tengu/memory.json"));

        assert!(store.list().unwrap().is_empty());
        assert!(store.search("anything").unwrap().is_empty());
        assert!(store.add("   ").unwrap_err().to_string().contains("empty"));
        assert_eq!(format_memory_entries(&[]), "no memory entries");
        assert!(format_memory_context(&[], 3).is_none());
        assert!(format_memory_context(
            &[MemoryEntry {
                id: "mem-1".to_string(),
                content: "remember this".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            }],
            0,
        )
        .is_none());
    }

    #[test]
    fn empty_search_lists_existing_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join(".tengu/memory.json"));
        store.add("Alpha").unwrap();

        let found = store.search("  ").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].content, "Alpha");
    }
}
