use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct TiledMap {
    pub compressionlevel: i32,
    pub height: u32,
    pub width: u32,
    pub infinite: bool,
    pub layers: Vec<Layer>,
    pub nextlayerid: u32,
    pub nextobjectid: u32,
    pub orientation: String,
    pub renderorder: String,
    pub tiledversion: String,
    pub tileheight: u32,
    pub tilesets: Vec<TilesetRef>,
    pub tilewidth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexsidelength: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staggeraxis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staggerindex: Option<String>,
    #[serde(rename = "type")]
    pub map_type: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct TilesetRef {
    pub firstgid: u32,
    pub source: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")] 
pub enum Layer {
    #[serde(rename = "tilelayer")]
    TileLayer {
        id: u32,
        name: String,
        data: Vec<u32>,
        height: u32,
        width: u32,
        opacity: f32,
        visible: bool,
        x: i32,
        y: i32,
    },
    #[serde(rename = "objectgroup")]
    ObjectGroup {
        id: u32,
        name: String,
        objects: Vec<Object>,
        opacity: f32,
        visible: bool,
        x: i32,
        y: i32,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct Object {
    pub height: f32,
    pub width: f32,
    pub id: u32,
    pub name: String,
    pub rotation: f32,
    #[serde(rename = "type")]
    pub obj_type: String,
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct Text {
    pub text: String,
    pub color: String,
    pub pixelsize: f32,
    pub halign: String,
    pub valign: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
}

// Tileset Definition (for .tsj files)
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct Tileset {
    pub columns: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imageheight: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imagewidth: Option<u32>,
    pub margin: u32,
    pub name: String,
    pub spacing: u32,
    pub tilecount: u32,
    pub tileheight: u32,
    pub tilewidth: u32,
    #[serde(rename = "type")]
    pub tileset_type: String,
    pub version: String,
    pub tiledversion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiles: Option<Vec<TileDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tileoffset: Option<TileOffset>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct TileOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub struct TileDef {
    pub id: u32,
    pub image: String,
    pub imageheight: u32,
    pub imagewidth: u32,
}
