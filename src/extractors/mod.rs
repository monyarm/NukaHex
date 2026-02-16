use crate::schema::RawMapData;

pub trait Extractor {
    fn extract(&self) -> RawMapData;
    fn get_id(&self) -> &str;
    fn get_source_paths(&self) -> Vec<String> { Vec::new() }
}

pub mod interplay;
pub mod manual;
