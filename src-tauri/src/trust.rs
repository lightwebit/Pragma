use std::collections::HashSet;

pub struct TrustedDirs(pub std::sync::Mutex<HashSet<String>>);

pub fn trusted_dirs_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        std::path::Path::new(&home)
            .join(".pragma")
            .join("trusted_dirs.json"),
    )
}

impl TrustedDirs {
    pub fn load() -> Self {
        let set = trusted_dirs_path()
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        TrustedDirs(std::sync::Mutex::new(set))
    }

    pub fn contains(&self, dir: &str) -> bool {
        self.0.lock().map(|s| s.contains(dir)).unwrap_or(false)
    }

    pub fn insert(&self, dir: String) {
        if let Ok(mut s) = self.0.lock() {
            s.insert(dir);
            let v: Vec<String> = s.iter().cloned().collect();
            if let Some(path) = trusted_dirs_path() {
                if let Ok(json) = serde_json::to_string(&v) {
                    let _ = std::fs::write(&path, json);
                }
            }
        }
    }

    pub fn remove(&self, dir: &str) {
        if let Ok(mut s) = self.0.lock() {
            s.remove(dir);
            let v: Vec<String> = s.iter().cloned().collect();
            if let Some(path) = trusted_dirs_path() {
                if let Ok(json) = serde_json::to_string(&v) {
                    let _ = std::fs::write(&path, json);
                }
            }
        }
    }

    pub fn list(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
}
