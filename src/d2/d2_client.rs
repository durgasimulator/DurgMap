use crate::d2::d2_data::{get_act};
use crate::d2::d2_ptrs::{D2Pointers, define_offsets};
use crate::d2::d2_structs::*;
use crate::map::Map;
use crate::json::*;
use crate::d2::data::objects::{GameObject, GameObjectType, get_type, object_is_useless};
use crate::d2::data::blacha_objects::{blacha_object, blacha_keeps_npc};
use crate::d2::data::npcs::{GameNPC, get_npc_type, npc_is_useless};
use crate::d2::data::areas::LevelName;

use num_traits::FromPrimitive;
use std::ffi::CStr;
use std::os::raw::c_char;


pub struct D2Client {
    pub d2_client: D2ClientStruct,
    pub d2_dir: String,
    pub ptrs: D2Pointers,
    pub map: Map,
}

impl D2Client {
    pub fn new() -> Self {
        Self {
            d2_client: D2ClientStruct {
                dw_init: 0,
                _1: [0; 0x20D - 4],
                fp_init: 0,
            },
            d2_dir: String::new(),
            ptrs: D2Pointers::new(),
            map: Map::new(),
        }
    }

    pub unsafe fn initialize(&mut self, folder_name: &str) -> Result<(), String> {
        log::info!("Initializing D2 client from path: {}", folder_name);

        self.d2_dir = folder_name.to_string();

        // Set current directory
        std::env::set_current_dir(folder_name)
            .map_err(|e| format!("Failed to set directory: {}", e))?;

        // Define offsets
        define_offsets(&mut self.ptrs)?;
        log::debug!("Offsets defined");

        // Initialize D2
        self.init_pd2()?;

        // Restore directory
        std::env::set_current_dir(folder_name)
            .map_err(|e| format!("Failed to restore directory: {}", e))?;

        Ok(())
    }

    unsafe fn init_pd2(&mut self) -> Result<(), String> {
        // Initialize STORM MPQ hash table
        if !self.ptrs.p_storm_pd2_mpq_hash_table.is_null() {
            *self.ptrs.p_storm_pd2_mpq_hash_table = 0;
        }

        self.d2_client.dw_init = 1;
        self.d2_client.fp_init = self.d2client_interface() as DWORD;

        log::trace!("Initializing Fog.dll");
        if let Some(fog_10021) = self.ptrs.fog_10021 {
            let d2_str = b"D2\0";
            fog_10021(d2_str.as_ptr() as *const c_char);
        }

        if let Some(fog_10101) = self.ptrs.fog_10101 {
            fog_10101(1, 0);
        }

        if let Some(fog_10089) = self.ptrs.fog_10089 {
            fog_10089(1);
        }

        if let Some(fog_10218) = self.ptrs.fog_10218 {
            if fog_10218() == 0 {
                log::error!("Fog.dll initialization failed");
                return Err("Fog.dll initialization failed".to_string());
            }
        }
        log::debug!("Fog.dll initialized");

        log::trace!("Initializing D2Win.dll d2win_10086");
        if let Some(d2win_10086) = self.ptrs.d2win_10086 {
            if d2win_10086() == 0 {
                log::error!("d2win_10086 failed");
                // return Err("D2Win.dll initialization failed".to_string());
            }
        }

        // log::trace!("Initializing D2Win.dll d2win_10005");
        // if let Some(d2win_10005) = self.ptrs.d2win_10005 {
        //     if d2win_10005(0, 0, 0, &mut self.d2_client as *mut _) == 0 {
        //         log::error!("D2Win.dll setup failed");
        //         // return Err("D2Win.dll setup failed".to_string());
        //     }
        // }
        log::debug!("D2Win.dll initialized");

        log::trace!("Initializing D2Lang.dll");
        if let Some(d2lang_10008) = self.ptrs.d2lang_10008 {
            let eng_str = b"ENG\0";
            d2lang_10008(0, eng_str.as_ptr() as *const c_char, 0);
        }
        log::debug!("D2Lang.dll initialized");

        log::trace!("Initializing D2Client.dll");
        if let Some(d2common_init) = self.ptrs.d2common_pd2_init_data_tables {
            d2common_init(0, 0, 0);
        }
        log::debug!("D2Client.dll initialized");

        Ok(())
    }

    fn d2client_interface(&self) -> usize {
        self.d2_client.dw_init as usize
    }

    pub unsafe fn get_level(&self, misc: *mut ActMisc, level_code: u32) -> *mut Level {
        if let Some(get_level_text) = self.ptrs.d2common_pd2_get_level_text {
            let level_data = get_level_text(level_code);
            if level_data.is_null() {
                return std::ptr::null_mut();
            }
        } else {
            return std::ptr::null_mut();
        }

        if misc.is_null() {
            return std::ptr::null_mut();
        }

        let mut p_level = (*misc).p_level_first;
        while !p_level.is_null() {
            if (*p_level).dw_level_no == level_code {
                return p_level;
            }
            p_level = (*p_level).p_next_level;
        }

        if let Some(get_level) = self.ptrs.d2common_pd2_get_level {
            get_level(misc, level_code)
        } else {
            std::ptr::null_mut()
        }
    }

    unsafe fn add_collision_data(&mut self, p_col: *mut CollMap, origin_x: i32, origin_y: i32) {
        if p_col.is_null() {
            return;
        }

        let x = (*p_col).dw_pos_game_x as i32 - origin_x;
        let y = (*p_col).dw_pos_game_y as i32 - origin_y;
        let cx = (*p_col).dw_size_game_x as i32;
        let cy = (*p_col).dw_size_game_y as i32;

        let n_limit_x = x + cx;
        let n_limit_y = y + cy;

        let mut p = (*p_col).p_map_start;
        for j in y..n_limit_y {
            for i in x..n_limit_x {
                if !p.is_null() {
                    let mut p_val = *p as i32;
                    if p_val == 1024 {
                        p_val = 1;
                    }
                    self.map.set(i, j, p_val);
                    p = p.add(1);
                }
            }
        }
    }

    fn is_good_exit(&self, p_act: *mut Act, p_level: *mut Level, exit_id: u32) -> bool {
        unsafe {
            let level_no = if !p_level.is_null() {
                (*p_level).dw_level_no
            } else {
                return false;
            };

            // Act 1
            if level_no == 2 && exit_id == 8 {
                return true;
            } // Blood Moor -> Den of Evil
            if level_no == 7 && exit_id == 12 {
                return true;
            } // Tamoe Highlands -> Pit
            if level_no == 6 && exit_id == 20 {
                return true;
            } // Black Marsh -> Forgotten Tower

            // Act 2
            if !p_act.is_null() && !(*p_act).p_misc.is_null() {
                let staff_tomb = (*(*p_act).p_misc).dw_staff_tomb_level;
                if exit_id == staff_tomb {
                    return true;
                }
            }
            if level_no == 43 && exit_id == 62 {
                return true;
            } // Far Oasis -> Maggot Lair
            if level_no == 45 && exit_id == 58 {
                return true;
            } // Valley of Snakes -> Claw Viper
            if level_no == 41 && exit_id == 55 {
                return true;
            } // Rocky Waste -> Stony Tomb
            if level_no == 44 && exit_id == 65 {
                return true;
            } // Lost City -> Ancient Tunnels

            // Act 3
            if level_no == 76 && exit_id == 85 {
                return true;
            } // Spider Forest -> Spider Cavern
            if level_no == 78 && exit_id == 88 {
                return true;
            } // Flayer Jungle -> Flayer Dungeon
            if level_no == 80 && exit_id == 94 {
                return true;
            } // Kurast Bazaar -> Ruined Temple

            // Act 5
            if level_no == 113 && exit_id == 114 {
                return true;
            } // Crystalline Passage -> Frozen River

            false
        }
    }

    unsafe fn dump_objects(
        &self,
        p_act: *mut Act,
        p_level: *mut Level,
        p_room2: *mut Room2,
    ) -> Vec<Object> {
        let mut objects = Vec::new();

        if p_level.is_null() || p_room2.is_null() {
            return objects;
        }

        let offset_x = (*p_level).dw_pos_x as i32 * 5;
        let offset_y = (*p_level).dw_pos_y as i32 * 5;

        let room_offset_x = (*p_room2).dw_pos_x as i32 * 5 - offset_x;
        let room_offset_y = (*p_room2).dw_pos_y as i32 * 5 - offset_y;

        let mut p_preset_unit = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room2).p_preset));
        while !p_preset_unit.is_null() {
            // Copy values from packed struct to avoid alignment issues
            let unit_type = std::ptr::read_unaligned(std::ptr::addr_of!((*p_preset_unit).dw_type));
            let txt_file_no = std::ptr::read_unaligned(std::ptr::addr_of!((*p_preset_unit).dw_txt_file_no));
            let pos_x = std::ptr::read_unaligned(std::ptr::addr_of!((*p_preset_unit).dw_pos_x));
            let pos_y = std::ptr::read_unaligned(std::ptr::addr_of!((*p_preset_unit).dw_pos_y));

            let coord_x = room_offset_x + pos_x as i32;
            let coord_y = room_offset_y + pos_y as i32;

            if unit_type == UNIT_TYPE_NPC {
                // Match blacha exactly: emit only the npc ids blacha keeps.
                if blacha_keeps_npc(txt_file_no) {
                    objects.push(Object {
                        id: txt_file_no,
                        object_type: "npc".to_string(),
                        x: coord_x as u32,
                        y: coord_y as u32,
                        name: String::new(),
                        op: 0,
                        class: String::new(),
                        is_good_exit: None,
                    });
                }
            } else if unit_type == UNIT_TYPE_OBJECT {
                // Match blacha exactly: only ids blacha emits, with blacha's name/class/op.
                if let Some((name, class, op)) = blacha_object(txt_file_no) {
                    objects.push(Object {
                        id: txt_file_no,
                        object_type: "object".to_string(),
                        x: coord_x as u32,
                        y: coord_y as u32,
                        name: name.to_string(),
                        op,
                        class: class.to_string(),
                        is_good_exit: None,
                    });
                }
            } else if unit_type == UNIT_TYPE_TILE {
                let mut p_room_tile = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room2).p_room_tiles));
                while !p_room_tile.is_null() {
                    let n_num = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room_tile).n_num));
                    if !n_num.is_null()
                        && *n_num == txt_file_no
                    {
                        let p_room2_tile = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room_tile).p_room2));
                        if !p_room2_tile.is_null() {
                            let exit_level = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room2_tile).p_level));
                            if !exit_level.is_null() {
                                let object_id = (*exit_level).dw_level_no;
                                let mut obj = Object {
                                    id: object_id,
                                    object_type: "exit".to_string(),
                                    x: coord_x as u32,
                                    y: coord_y as u32,
                                    name: String::new(),
                                    op: 0,
                                    class: String::new(),
                                    is_good_exit: None,
                                };

                                if self.is_good_exit(p_act, p_level, object_id) {
                                    obj.is_good_exit = Some(true);
                                }
                                objects.push(obj);
                            }
                        }
                    }
                    p_room_tile = std::ptr::read_unaligned(std::ptr::addr_of!((*p_room_tile).p_next));
                }
            }

            p_preset_unit = std::ptr::read_unaligned(std::ptr::addr_of!((*p_preset_unit).p_preset_next));
        }

        objects
    }

    /// Load an act for a given seed/difficulty. Returns a null pointer on failure.
    /// The caller is responsible for unloading the act with `unload_act`.
    pub unsafe fn load_act(&self, act_id: i32, seed: u32, difficulty: u32) -> *mut Act {
        if act_id < 0 {
            return std::ptr::null_mut();
        }
        if let Some(load_act) = self.ptrs.d2common_load_act {
            load_act(act_id as u32, seed, 0, 0, difficulty, 0, 0, 0, 0)
        } else {
            std::ptr::null_mut()
        }
    }

    /// Unload a previously loaded act. Safe to call with a null pointer.
    /// Required for a long-lived (resident) server to avoid leaking acts.
    pub unsafe fn unload_act(&self, p_act: *mut Act) {
        if p_act.is_null() {
            return;
        }
        if let Some(unload_act) = self.ptrs.d2common_pd2_unload_act {
            unload_act(p_act);
        }
    }

    /// Dump a single level using an already-loaded act. Does NOT load or unload the act,
    /// so the same act can be reused to dump every level it contains (one load per act).
    pub unsafe fn dump_map_with_act(
        &mut self,
        p_act: *mut Act,
        level_code: u32,
    ) -> Result<LevelData, String> {
        if p_act.is_null() {
            return Err("Null act".to_string());
        }

        let get_level_text = match self.ptrs.d2common_pd2_get_level_text {
            Some(f) => f,
            None => return Err("Level text function not available".to_string()),
        };

        let level_data = get_level_text(level_code);
        if level_data.is_null() {
            return Err("Level not found".to_string());
        }

        let level_name = CStr::from_ptr((*level_data).sz_name.as_ptr())
            .to_str()
            .unwrap_or("Unknown");

        let p_level = self.get_level((*p_act).p_misc, level_code);
        if p_level.is_null() {
            return Err("Failed to get level".to_string());
        }

        if (*p_level).p_room2_first.is_null() {
            if let Some(init_level) = self.ptrs.d2common_pd2_init_level {
                init_level(p_level);
            }
        }

        if (*p_level).p_room2_first.is_null() {
            return Err("Failed to initialize rooms".to_string());
        }

        let origin_x = (*p_level).dw_pos_x as i32 * 5;
        let origin_y = (*p_level).dw_pos_y as i32 * 5;
        let map_width = (*p_level).dw_size_x as i32 * 5;
        let map_height = (*p_level).dw_size_y as i32 * 5;

        log::trace!(
            "Map initialized: level={}, origin=({},{}), size={}x{}",
            level_code,
            origin_x,
            origin_y,
            map_width,
            map_height
        );

        self.map.reset();

        // Collect objects and collision data
        let mut all_objects: Vec<Object> = Vec::new();
        let mut p_room2 = (*p_level).p_room2_first;

        while !p_room2.is_null() {
            let b_added = (*p_room2).p_room1.is_null();

            if b_added {
                if let Some(add_room) = self.ptrs.d2common_pd2_add_room_data {
                    add_room(p_act, level_code as i32, (*p_room2).dw_pos_x as i32, (*p_room2).dw_pos_y as i32, (*p_room2).p_room1);
                }
            }

            let objects: Vec<Object> = self.dump_objects(p_act, p_level, p_room2);
            all_objects.extend(objects);

            if !(*p_room2).p_room1.is_null() {
                self.add_collision_data((*(*p_room2).p_room1).coll, origin_x, origin_y);
            }

            if b_added {
                if let Some(remove_room) = self.ptrs.d2common_pd2_remove_room_data {
                    remove_room(p_act, level_code as i32, 0, 0, (*p_room2).p_room1);
                }
            }

            p_room2 = (*p_room2).p_room2_next;
        }

        // Build collision map
        let mut map_data = Vec::new();
        let max_y = self.map.max_y();
        for y in 0..=max_y {
            let mut row = Vec::new();
            let mut last = 'X';
            let mut count = 0;

            for x in 0..map_width {
                let map_val = if self.map.get(x, y) % 2 != 0 { 'X' } else { ' ' };
                if map_val == last {
                    count += 1;
                } else {
                    // blacha convention: the first run is the leading *blocked* count and is
                    // emitted even when zero, so walkable-first rows begin with a 0. Push on
                    // every transition (no `count > 0` guard).
                    row.push(count);
                    count = 1;
                    last = map_val;
                }
            }
            row.push(count);
            // blacha omits the final run of every row; the map decoder fills the remainder.
            row.pop();
            map_data.push(row);
        }

        Ok(LevelData {
            level_type: "map".to_string(),
            id: level_code,
            name: level_name.to_string(),
            offset: Offset {
                x: origin_x as u32,
                y: origin_y as u32,
            },
            size: Size {
                width: map_width as u32,
                height: map_height as u32,
            },
            objects: all_objects,
            map: map_data,
        })
    }

    /// Dump a single level, loading and unloading the act around it.
    /// Convenience wrapper used by the CLI and the single-map HTTP endpoint.
    pub unsafe fn dump_map(
        &mut self,
        seed: u32,
        difficulty: u32,
        level_code: u32,
    ) -> Result<LevelData, String> {
        let act_id = get_act(level_code);
        if act_id < 0 {
            return Err("Invalid act".to_string());
        }
        let p_act = self.load_act(act_id, seed, difficulty);
        if p_act.is_null() {
            return Err("Failed to load act".to_string());
        }
        let result = self.dump_map_with_act(p_act, level_code);
        self.unload_act(p_act);
        result
    }
}
