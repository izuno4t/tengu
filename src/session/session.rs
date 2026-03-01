// Session module
// セッション管理

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationTurn {
    pub role: SessionConversationRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionLogRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionLogLine {
    pub role: SessionLogRole,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionImage {
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPendingInput {
    pub text: String,
    pub logged: bool,
    #[serde(default)]
    pub images: Vec<SessionImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPendingApproval {
    pub prompt: String,
    pub kind: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageRecord {
    pub provider: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub last_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub conversation: Vec<SessionConversationTurn>,
    #[serde(default)]
    pub log_lines: Vec<SessionLogLine>,
    #[serde(default)]
    pub queue: Vec<SessionPendingInput>,
    #[serde(default)]
    pub pending_images: Vec<SessionImage>,
    #[serde(default)]
    pub usage_records: Vec<SessionUsageRecord>,
    #[serde(default)]
    pub pending_approval: Option<SessionPendingApproval>,
}

impl Session {
    pub fn new() -> Self {
        Self::with_id(Uuid::new_v4().to_string())
    }

    pub fn with_id(id: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id,
            created_at: now.clone(),
            updated_at: now,
            conversation: Vec::new(),
            log_lines: Vec::new(),
            queue: Vec::new(),
            pending_images: Vec::new(),
            usage_records: Vec::new(),
            pending_approval: None,
        }
    }

    pub fn fork(&self) -> Self {
        let mut forked = Self::with_id(Uuid::new_v4().to_string());
        forked.conversation = self.conversation.clone();
        forked.log_lines = self.log_lines.clone();
        forked.queue = self.queue.clone();
        forked.pending_images = self.pending_images.clone();
        forked.usage_records = self.usage_records.clone();
        forked.pending_approval = self.pending_approval.clone();
        forked
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
        Self::save_to_path(&self.session_path(&session.id), session)
    }

    pub fn save_to_path(path: &Path, session: &Session) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(session)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        Self::load_from_path(&self.session_path(id))
    }

    pub fn load_from_path(path: &Path) -> Result<Session> {
        let data = fs::read_to_string(path)?;
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
