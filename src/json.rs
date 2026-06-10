
use serde::{Deserialize, Serialize};

#[derive(Serialize, Default, Deserialize, Debug)]
pub struct SeedData {
    pub seed: u32,
    pub difficulty: u32,
    pub levels: Vec<LevelData>,
}

/// blacha-format level: serializes as
/// {"type":"map", "id":.., "name":.., "offset":{..}, "size":{..}, "objects":[..], "map":[..]}
#[derive(Serialize, Default, Deserialize, Debug)]
pub struct LevelData {
    #[serde(rename = "type")]
    pub level_type: String, // always "map"
    pub id: u32,
    pub name: String,
    pub offset: Offset,
    pub size: Size,
    pub objects: Vec<Object>,
    pub map: Vec<Vec<u64>>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Offset {
    pub x: u32,
    pub y: u32,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// blacha-format object. npc/exit serialize compactly as {"id","type","x","y"}; "object" adds
/// {"name","op","class"}. Field order matches blacha so the output is byte-similar.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Object {
    pub id: u32,
    #[serde(rename = "type")]
    pub object_type: String, // "npc" | "object" | "exit"
    pub x: u32,
    pub y: u32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub name: String,
    #[serde(skip_serializing_if = "is_zero", default)]
    pub op: u32,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub class: String,
    #[serde(rename = "isGoodExit", skip_serializing_if = "Option::is_none")]
    pub is_good_exit: Option<bool>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}
