use crate::schema::RawMapData;

pub trait Extractor {
    fn extract(&self) -> RawMapData;
    fn get_id(&self) -> &str;
}

pub mod fallout1;
pub mod fallout2;
