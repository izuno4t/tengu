// Session module
// セッション管理

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Session {
    pub fn new() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> Result<PathBuf> {
        let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
        Ok(PathBuf::from(home).join(".tengu").join("sessions"))
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        self.ensure()?;
        let path = self.session_path(&session.id);
        let data = serde_json::to_string_pretty(session)?;
        fs::write(&path, data)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.session_path(id);
        let data = fs::read_to_string(&path)?;
        let session = serde_json::from_str(&data)?;
        Ok(session)
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        if !self.root.exists() {
            return Ok(sessions);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("json")
                && path.file_name().and_then(|n| n.to_str()) != Some("sessions.db")
            {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&data) {
                        sessions.push(session);
                    }
                }
            }
        }
        Ok(sessions)
    }

    pub fn latest(&self) -> Result<Option<Session>> {
        let mut sessions = self.list()?;
        sessions.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        Ok(sessions.pop())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.session_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        if self.root.exists() {
            for entry in fs::read_dir(&self.root)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    fs::remove_file(path)?;
                }
            }
            let index_path = self.index_path();
            if index_path.exists() {
                fs::remove_file(index_path)?;
            }
        }
        Ok(())
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{}.json", id))
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("sessions.db")
    }
}
