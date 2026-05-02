use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_SCAN_ENTRIES: usize = 5000;
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCompletionCandidate {
    pub replacement: String,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCompletionContext {
    pub start: usize,
    pub end: usize,
    pub query: String,
}

#[derive(Debug, Clone)]
struct ScoredPath {
    path: String,
    is_dir: bool,
    score: i64,
}

pub fn completion_context(input: &str) -> Option<FileCompletionContext> {
    let end = input.len();
    let start = input[..end]
        .rfind(char::is_whitespace)
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let token = &input[start..end];
    let query = token.strip_prefix('@')?;
    Some(FileCompletionContext {
        start,
        end,
        query: query.to_string(),
    })
}

pub fn replace_completion(
    input: &str,
    context: &FileCompletionContext,
    candidate: &FileCompletionCandidate,
) -> String {
    let mut next = String::with_capacity(input.len() + candidate.replacement.len());
    next.push_str(&input[..context.start]);
    next.push('@');
    next.push_str(&candidate.replacement);
    if !candidate.replacement.ends_with('/') {
        next.push(' ');
    }
    next.push_str(&input[context.end..]);
    next
}

pub fn extract_file_references(input: &str, root: &Path) -> Vec<String> {
    let mut refs = Vec::new();
    for token in input.split_whitespace() {
        let Some(raw) = token.strip_prefix('@') else {
            continue;
        };
        let cleaned = raw.trim_matches(|ch: char| matches!(ch, ',' | '.' | ':' | ';' | ')' | ']'));
        if cleaned.is_empty() {
            continue;
        }
        let path = root.join(cleaned);
        if path.exists() && !refs.iter().any(|existing| existing == cleaned) {
            refs.push(cleaned.to_string());
        }
    }
    refs
}

pub fn file_completions(
    root: &Path,
    query: &str,
    recent_files: &[String],
    limit: usize,
) -> Vec<FileCompletionCandidate> {
    if limit == 0 {
        return Vec::new();
    }
    let ignored = IgnoreRules::load(root);
    let recent_set = recent_files.iter().cloned().collect::<HashSet<_>>();
    let mut scanned = Vec::new();
    scan_dir(root, root, 0, &ignored, &mut scanned);

    let mut scored = scanned
        .into_iter()
        .filter_map(|(path, is_dir)| {
            let base = fuzzy_score(&path, query)?;
            let recent_boost = if recent_set.contains(&path) { 1000 } else { 0 };
            let dir_boost = if is_dir { 20 } else { 0 };
            Some(ScoredPath {
                path,
                is_dir,
                score: base + recent_boost + dir_boost,
            })
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| match b.score.cmp(&a.score) {
        Ordering::Equal => a.path.cmp(&b.path),
        other => other,
    });
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|item| {
            let replacement = if item.is_dir {
                format!("{}/", item.path.trim_end_matches('/'))
            } else {
                item.path
            };
            FileCompletionCandidate {
                display: replacement.clone(),
                replacement,
            }
        })
        .collect()
}

fn scan_dir(
    root: &Path,
    dir: &Path,
    depth: usize,
    ignored: &IgnoreRules,
    out: &mut Vec<(String, bool)>,
) {
    if depth > MAX_DEPTH || out.len() >= MAX_SCAN_ENTRIES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_SCAN_ENTRIES {
            return;
        }
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if ignored.matches(&relative) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();
        out.push((relative.clone(), is_dir));
        if is_dir {
            scan_dir(root, &path, depth + 1, ignored, out);
        }
    }
}

fn fuzzy_score(path: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(1);
    }
    let path_lower = path.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    if path_lower.starts_with(&query_lower) {
        return Some(5000 - path.len() as i64);
    }
    if path_lower.contains(&query_lower) {
        return Some(3000 - path.len() as i64);
    }

    let mut score = 0i64;
    let mut last_match: Option<usize> = None;
    let mut search_from = 0usize;
    for query_char in query_lower.chars() {
        let slice = &path_lower[search_from..];
        let found = slice.find(query_char)?;
        let absolute = search_from + found;
        score += 80;
        if let Some(last) = last_match {
            if absolute == last + 1 {
                score += 35;
            }
        }
        if absolute == 0 || path_lower.as_bytes().get(absolute.saturating_sub(1)) == Some(&b'/') {
            score += 25;
        }
        last_match = Some(absolute);
        search_from = absolute + query_char.len_utf8();
    }
    Some(score - path.len() as i64)
}

#[derive(Debug, Default)]
struct IgnoreRules {
    patterns: Vec<String>,
}

impl IgnoreRules {
    fn load(root: &Path) -> Self {
        let mut patterns = vec![
            ".git".to_string(),
            "target".to_string(),
            "node_modules".to_string(),
        ];
        if let Ok(text) = fs::read_to_string(root.join(".gitignore")) {
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                    continue;
                }
                patterns.push(trimmed.trim_end_matches('/').to_string());
            }
        }
        Self { patterns }
    }

    fn matches(&self, relative: &str) -> bool {
        self.patterns.iter().any(|pattern| {
            let pattern = pattern.trim_start_matches('/');
            wildcard_match(pattern, relative)
                || relative == pattern
                || relative.starts_with(&format!("{}/", pattern))
                || relative.ends_with(&format!("/{}", pattern))
                || relative.contains(&format!("/{}/", pattern))
        })
    }
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_idx, mut text_idx) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while text_idx < text.len() {
        if pattern_idx < pattern.len()
            && (pattern[pattern_idx] == b'?' || pattern[pattern_idx] == text[text_idx])
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            match_idx = text_idx;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            match_idx += 1;
            text_idx = match_idx;
        } else {
            return false;
        }
    }

    while pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
        pattern_idx += 1;
    }
    pattern_idx == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_at_completion_context() {
        assert_eq!(
            completion_context("review @src/ma").unwrap(),
            FileCompletionContext {
                start: 7,
                end: 14,
                query: "src/ma".to_string()
            }
        );
        assert!(completion_context("review src/ma").is_none());
    }

    #[test]
    fn replaces_current_file_reference() {
        let input = "review @src/ma";
        let context = completion_context(input).unwrap();
        let candidate = FileCompletionCandidate {
            replacement: "src/main.rs".to_string(),
            display: "src/main.rs".to_string(),
        };
        assert_eq!(
            replace_completion(input, &context, &candidate),
            "review @src/main.rs "
        );
    }

    #[test]
    fn returns_fuzzy_matches_and_honors_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/tools")).unwrap();
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("src/tools/mod.rs"), "").unwrap();
        fs::write(dir.path().join("target/hidden.rs"), "").unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored\n*.log\n").unwrap();
        fs::create_dir_all(dir.path().join("ignored")).unwrap();
        fs::write(dir.path().join("ignored/file.rs"), "").unwrap();
        fs::write(dir.path().join("debug.log"), "").unwrap();

        let matches = file_completions(dir.path(), "stm", &[], 10);
        assert!(matches
            .iter()
            .any(|candidate| candidate.replacement == "src/tools/mod.rs"));
        assert!(!matches
            .iter()
            .any(|candidate| candidate.replacement.contains("target")));
        assert!(!matches
            .iter()
            .any(|candidate| candidate.replacement.contains("ignored")));
        assert!(!matches
            .iter()
            .any(|candidate| candidate.replacement.contains("debug.log")));
    }

    #[test]
    fn recent_files_rank_first() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        fs::write(dir.path().join("src/mod.rs"), "").unwrap();

        let matches = file_completions(dir.path(), "rs", &["src/mod.rs".to_string()], 10);
        assert_eq!(matches.first().unwrap().replacement, "src/mod.rs");
    }

    #[test]
    fn extracts_existing_file_references() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();

        assert_eq!(
            extract_file_references("check @src/main.rs, and @missing.rs", dir.path()),
            vec!["src/main.rs".to_string()]
        );
    }
}
