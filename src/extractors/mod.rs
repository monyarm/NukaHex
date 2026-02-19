use crate::schema::RawMapData;

pub trait Extractor: Sync + Send {
    fn extract(&self) -> RawMapData;
    fn get_id(&self) -> &str;
    fn get_source_paths(&self) -> Vec<String> { Vec::new() }
    fn get_code_path(&self) -> &str;
    fn get_cluster_id(&self) -> &str;
}

pub mod interplay;
pub mod manual;
