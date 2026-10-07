use serde::{Serialize, de::DeserializeOwned};
use sha2::{Sha256, Digest};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::Read;
use log::info;

pub fn compute_source_hash(path: &str) -> String {
    let mut hasher = Sha256::new();
    let p = Path::new(path);

    if p.is_dir() {
        if let Ok(entries) = fs::read_dir(p) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .collect();
            paths.sort(); // Crucial for deterministic hash
            
            for subpath in paths {
                if let Some(s) = subpath.to_str() {
                    let sub_hash = compute_source_hash(s);
                    hasher.update(sub_hash.as_bytes());
                }
            }
        }
    } else if let Ok(mut file) = fs::File::open(path) {
        let mut buffer = Vec::new();
        if file.read_to_end(&mut buffer).is_ok() {
             hasher.update(&buffer);
        } else {
             hasher.update(path.as_bytes());
        }
    } else {
        // Fallback for strings/non-files
        hasher.update(path.as_bytes());
    }
    hex::encode(hasher.finalize())
}

pub struct CachedStep<'a, T> {
    id: &'a str,
    output_dir: PathBuf,
    deps_hash: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<'a, T> CachedStep<'a, T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn new(id: &'a str, output_dir: &Path) -> Self {
        Self {
            id,
            output_dir: output_dir.to_path_buf(),
            deps_hash: String::new(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn add_dependency_file(mut self, path: &str) -> Self {
        let hash = compute_source_hash(path);
        self.deps_hash.push_str(&hash);
        self
    }
    
    pub fn add_dependency_files(self, paths: &[&str]) -> Self {
        paths.iter().fold(self, |step, p| step.add_dependency_file(p))
    }

    pub fn add_dependency_hash(mut self, hash: &str) -> Self {
        self.deps_hash.push_str(hash);
        self
    }

    pub fn run<F>(self, generator: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.run_if(generator, |_| true)
    }

    /// As `run`, but a result `keep` rejects is not saved (a failure to retry next time).
    pub fn run_if<F, K>(self, generator: F, keep: K) -> T
    where
        F: FnOnce() -> T,
        K: FnOnce(&T) -> bool,
    {
        let final_hash = {
             let mut hasher = Sha256::new();
             hasher.update(self.deps_hash.as_bytes());
             hex::encode(hasher.finalize())
        };

        let cache_file = self.output_dir.join(format!("{}.json", self.id));
        let hash_file = self.output_dir.join(format!("{}.hash", self.id));

        if cache_file.exists() && hash_file.exists() {
            if let Ok(saved_hash) = fs::read_to_string(&hash_file) {
                if saved_hash == final_hash {
                    if let Ok(content) = fs::read_to_string(&cache_file) {
                        if let Ok(data) = serde_json::from_str::<T>(&content) {
                            info!("[Cache: {}] Hit.", self.id);
                            return data;
                        }
                    }
                }
            }
        }

        info!("[Cache: {}] Miss. Running generation...", self.id);
        let data = generator();

        if !keep(&data) {
            info!("[Cache: {}] Not saved (failed).", self.id);
            return data;
        }
        if let Ok(json) = serde_json::to_vec(&data) {
            let _ = fs::write(&cache_file, json);
            let _ = fs::write(&hash_file, final_hash.clone());
        }

        data
    }
    
    // For when we need the hash but the data is large/complex or we just want to return the hash too
    pub fn run_with_hash<F>(self, generator: F) -> (T, String)
    where
        F: FnOnce() -> T,
    {
        let final_hash = {
             let mut hasher = Sha256::new();
             hasher.update(self.deps_hash.as_bytes());
             hex::encode(hasher.finalize())
        };
        
        let data = self.run(generator);
        (data, final_hash)
    }

    /// As `run_with_hash`, with `run_if`'s `keep`.
    pub fn run_with_hash_if<F, K>(self, generator: F, keep: K) -> (T, String)
    where
        F: FnOnce() -> T,
        K: FnOnce(&T) -> bool,
    {
        let final_hash = {
             let mut hasher = Sha256::new();
             hasher.update(self.deps_hash.as_bytes());
             hex::encode(hasher.finalize())
        };
        let data = self.run_if(generator, keep);
        (data, final_hash)
    }
}
