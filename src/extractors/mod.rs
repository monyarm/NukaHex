use crate::schema::RawMapData;

pub trait Extractor: Sync + Send {
    fn extract(&self) -> RawMapData;
    fn get_id(&self) -> &str;
    fn get_source_paths(&self) -> Vec<String> { Vec::new() }
    fn get_code_path(&self) -> &str;
    fn get_cluster_id(&self) -> String;
    /// Settings that change what `extract` produces beyond its code and files (worldspaces, placement
    /// options...): part of its cache key.
    fn get_config(&self) -> String { String::new() }
}

pub mod interplay;
pub mod manual;
pub mod bethesda;
pub mod lod;
pub mod van_buren;
pub mod tactics;
