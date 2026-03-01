use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAgent {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

impl StoredAgent {
    pub fn scaffold(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Custom agent: {}", name),
            prompt: format!(
                "You are the `{}` agent. Help with this task while staying concise and practical.",
                name
            ),
        }
    }
}

pub struct AgentStore {
    global_root: PathBuf,
    local_root: PathBuf,
}

impl AgentStore {
    pub fn new() -> Self {
        let global_root = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".tengu")
            .join("agents");
        let local_root = PathBuf::from(".").join(".tengu").join("agents");
        Self::with_roots(global_root, local_root)
    }

    pub fn with_roots(global_root: PathBuf, local_root: PathBuf) -> Self {
        Self {
            global_root,
            local_root,
        }
    }

    pub fn list(&self) -> Result<Vec<StoredAgent>> {
        let mut agents = Vec::new();
        for root in [&self.global_root, &self.local_root] {
            if !root.exists() {
                continue;
            }
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|v| v.to_str()) != Some("json") {
                    continue;
                }
                let data = fs::read_to_string(&path)?;
                let agent: StoredAgent = serde_json::from_str(&data)?;
                if !agents
                    .iter()
                    .any(|existing: &StoredAgent| existing.name == agent.name)
                {
                    agents.push(agent);
                }
            }
        }
        agents.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(agents)
    }

    pub fn save_local(&self, agent: &StoredAgent) -> Result<PathBuf> {
        fs::create_dir_all(&self.local_root)?;
        let path = self.local_root.join(format!("{}.json", agent.name));
        let data = serde_json::to_string_pretty(agent)?;
        fs::write(&path, data)?;
        Ok(path)
    }

    pub fn load(&self, name: &str) -> Result<StoredAgent> {
        for path in [
            self.local_root.join(format!("{}.json", name)),
            self.global_root.join(format!("{}.json", name)),
        ] {
            if path.exists() {
                let data = fs::read_to_string(path)?;
                let agent: StoredAgent = serde_json::from_str(&data)?;
                return Ok(agent);
            }
        }
        Err(anyhow!("agent not found: {}", name))
    }

    pub fn remove(&self, name: &str) -> Result<bool> {
        for path in [
            self.local_root.join(format!("{}.json", name)),
            self.global_root.join(format!("{}.json", name)),
        ] {
            if path.exists() {
                fs::remove_file(path)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tengu-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn scaffold_contains_name() {
        let agent = StoredAgent::scaffold("reviewer");
        assert_eq!(agent.name, "reviewer");
        assert!(agent.prompt.contains("reviewer"));
    }

    #[test]
    fn saves_lists_loads_and_removes_local_agent() {
        let root = unique_temp_dir("agent-store");
        let store = AgentStore::with_roots(root.join("global"), root.join("local"));
        let agent = StoredAgent::scaffold("writer");

        let path = store.save_local(&agent).unwrap();
        assert!(path.exists());

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "writer");

        let loaded = store.load("writer").unwrap();
        assert_eq!(loaded.name, "writer");

        assert!(store.remove("writer").unwrap());
        assert!(store.list().unwrap().is_empty());
    }
}
