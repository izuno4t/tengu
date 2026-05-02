// Session module
// セッション管理

use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// Structured message history for agentic loop (Message[] serialized as JSON)
    #[serde(default)]
    pub messages: Vec<Value>,
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
            messages: Vec::new(),
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
        forked.messages = self.messages.clone();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_new_has_uuid_id() {
        let s = Session::new();
        assert!(!s.id.is_empty());
        // UUID v4 format: 8-4-4-4-12
        assert_eq!(s.id.len(), 36);
        assert_eq!(s.id.chars().filter(|c| *c == '-').count(), 4);
    }

    #[test]
    fn session_new_has_timestamps() {
        let s = Session::new();
        assert!(!s.created_at.is_empty());
        assert!(!s.updated_at.is_empty());
        assert_eq!(s.created_at, s.updated_at);
    }

    #[test]
    fn session_new_has_empty_collections() {
        let s = Session::new();
        assert!(s.conversation.is_empty());
        assert!(s.log_lines.is_empty());
        assert!(s.queue.is_empty());
        assert!(s.pending_images.is_empty());
        assert!(s.usage_records.is_empty());
        assert!(s.pending_approval.is_none());
        assert!(s.messages.is_empty());
    }

    #[test]
    fn session_with_id_uses_given_id() {
        let s = Session::with_id("custom-id".to_string());
        assert_eq!(s.id, "custom-id");
    }

    #[test]
    fn session_default_creates_new() {
        let s = Session::default();
        assert!(!s.id.is_empty());
    }

    #[test]
    fn session_fork_copies_data_with_new_id() {
        let mut s = Session::new();
        s.conversation.push(SessionConversationTurn {
            role: SessionConversationRole::User,
            content: "hello".to_string(),
        });
        s.log_lines.push(SessionLogLine {
            role: SessionLogRole::System,
            text: "system init".to_string(),
        });

        let forked = s.fork();
        assert_ne!(forked.id, s.id);
        assert_eq!(forked.conversation.len(), 1);
        assert_eq!(forked.conversation[0].content, "hello");
        assert_eq!(forked.log_lines.len(), 1);
    }

    #[test]
    fn session_serialization_roundtrip() {
        let mut s = Session::new();
        s.conversation.push(SessionConversationTurn {
            role: SessionConversationRole::User,
            content: "test message".to_string(),
        });
        s.conversation.push(SessionConversationTurn {
            role: SessionConversationRole::Assistant,
            content: "response".to_string(),
        });

        let json = serde_json::to_string(&s).unwrap();
        let loaded: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.id, s.id);
        assert_eq!(loaded.conversation.len(), 2);
        assert_eq!(loaded.conversation[0].content, "test message");
        assert_eq!(loaded.conversation[1].content, "response");
    }

    #[test]
    fn session_store_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let mut s = Session::new();
        s.conversation.push(SessionConversationTurn {
            role: SessionConversationRole::User,
            content: "hello".to_string(),
        });

        store.save(&s).unwrap();
        let loaded = store.load(&s.id).unwrap();
        assert_eq!(loaded.id, s.id);
        assert_eq!(loaded.conversation.len(), 1);
    }

    #[test]
    fn session_store_list() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let s1 = Session::new();
        let s2 = Session::new();
        store.save(&s1).unwrap();
        store.save(&s2).unwrap();

        let sessions = store.list().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn session_store_list_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        let sessions = store.list().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn session_store_list_nonexistent_dir() {
        let store = SessionStore::new(PathBuf::from("/nonexistent/dir/sessions"));
        let sessions = store.list().unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn session_store_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let s = Session::new();
        store.save(&s).unwrap();
        assert!(store.load(&s.id).is_ok());

        store.delete(&s.id).unwrap();
        assert!(store.load(&s.id).is_err());
    }

    #[test]
    fn session_store_delete_nonexistent_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());
        // Should not error
        store.delete("nonexistent-id").unwrap();
    }

    #[test]
    fn session_store_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        store.save(&Session::new()).unwrap();
        store.save(&Session::new()).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);

        store.clear().unwrap();
        assert_eq!(store.list().unwrap().len(), 0);
    }

    #[test]
    fn session_store_latest() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        assert!(store.latest().unwrap().is_none());

        let s1 = Session::with_id("s1".to_string());
        store.save(&s1).unwrap();

        // Create s2 with a later timestamp
        let mut s2 = Session::with_id("s2".to_string());
        s2.updated_at = "9999-12-31T23:59:59+00:00".to_string();
        store.save(&s2).unwrap();

        let latest = store.latest().unwrap().unwrap();
        assert_eq!(latest.id, "s2");
    }

    #[test]
    fn session_store_save_to_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("session.json");
        let s = Session::new();
        SessionStore::save_to_path(&path, &s).unwrap();

        let loaded = SessionStore::load_from_path(&path).unwrap();
        assert_eq!(loaded.id, s.id);
    }

    #[test]
    fn session_usage_record_serialization() {
        let usage = SessionUsageRecord {
            provider: "anthropic".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            reasoning_tokens: 0,
            requests: 1,
            last_raw: None,
        };
        let json = serde_json::to_string(&usage).unwrap();
        let loaded: SessionUsageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.provider, "anthropic");
        assert_eq!(loaded.input_tokens, 100);
        assert_eq!(loaded.output_tokens, 50);
    }

    #[test]
    fn session_pending_approval_serialization() {
        let approval = SessionPendingApproval {
            prompt: "Allow Bash?".to_string(),
            kind: "tool".to_string(),
            tool: Some("Bash".to_string()),
            paths: vec!["/tmp".to_string()],
            args: vec!["ls".to_string()],
            message: None,
        };
        let json = serde_json::to_string(&approval).unwrap();
        let loaded: SessionPendingApproval = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.prompt, "Allow Bash?");
        assert_eq!(loaded.tool.unwrap(), "Bash");
    }
}
