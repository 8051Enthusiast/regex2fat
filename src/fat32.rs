use regex_automata::DFA;
use std::collections::HashMap;
pub type UFat = u32;
pub const BLOCK_SIZE: usize = 512;
const BOOT_SECTOR: [u8; 90] = [
    /*  0 */ 0xeb, 0xfe, 0x90,                                  // jump to self (placeholder)
    /*  3 */ 0x72, 0x65, 0x67, 0x65, 0x78, 0x20, 0x20, 0x20,    // "regex   " as vendor name
    /* 11 */ 0x00, 0x02,                                        // bytes per sector (512)
    /* 13 */ 0x01,                                              // one sector per cluster, why not
    /* 14 */ 0x08, 0x00,                                        // 8 reserved sectors
    /* 16 */ 0x01,                                              // one fat sector (don't really need two)
    /* 17 */ 0x00, 0x00,                                        // zero for fat32
    /* 19 */ 0x00, 0x00,                                        // zero for fat32
    /* 21 */ 0xF8,                                              // pretend to be a non-removable device
    /* 22 */ 0x00, 0x00,                                        // zero for fat32
    /* 24 */ 0x01, 0x00,                                        // it is the year 2020, no one uses CHS
    /* 26 */ 0x01, 0x00,                                        // but the values are 1 to prevent divide by zero...
    /* 28 */ 0x00, 0x00, 0x00, 0x00,                            // I don't ever want to boot from this
    /* 32 */    0,    0,    0,    0,                            // total number of sectors, gets calculated later
    /* 36 */    0,    0,    0,    0,                            // number of sectors for FAT, gets calculated later
    /* 40 */ 0x00, 0x00,                                        // fat mirroring enabled
    /* 42 */ 0x00, 0x00,                                        // version 0
    /* 44 */ 0x02, 0x00, 0x00, 0x00,                            // first cluster of root directory is 2
    /* 48 */ 0x01, 0x00,                                        // FSINFO location
    /* 50 */ 0x06, 0x00,                                        // backup in sector 6
    /* 52 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,                // 12 zeros reserved
    /* 64 */ 0x80,                                              // sure hope no one ever uses this on a floppy
    /* 65 */ 0x00,                                              // reserved
    /* 66 */ 0x00,                                              // no volume label/serial
    /* 67 */ 0x00, 0x00, 0x00, 0x00,                            // no serial
    /* 71 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,                   // no volume label
    /* 82 */ 0x66, 0x61, 0x74, 0x33, 0x32, 0x20, 0x20, 0x20     // "FAT32   "
];
const BOOT_SECTOR_TOTAL_SEC_32: usize = 32;
const BOOT_SECTOR_FAT_SZ_32: usize = 36;

const FSINFO_HEAD: [u8; 4] = [0x52, 0x52, 0x61, 0x41];

const FSINFO_TAIL: [u8; 28] = [
    0x72, 0x72, 0x41, 0x61,             // required signature
    0x00, 0x00, 0x00, 0x00,             // ideally, we used all sectors (else we would just make the image smaller)
    0xff, 0xff, 0xff, 0xff,             // don't know where the first free sector is, if there is none
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // a bunch of zeros
    0x00, 0x00, 0x55, 0xaa              // classic IBM
];

const FAT32_EOF: [u8; 4] = [0xff, 0xff, 0xff, 0x0f];

pub struct StatePosInfo {
    pub block: UFat,        // position (in blocks) of state dir
    pub byte_sized: usize,   // size (in bytes) of state dir
}
pub struct StateFatMap<D: DFA> {
    pub blocks: UFat,
    pub order_list: Vec<D::ID>,
    pub pos_hash: HashMap<D::ID, StatePosInfo>,
}


fn write_u32_into(into: &mut [u8], pos: usize, val: u32) {
    // why use indexing when iterators do the job in triple the space
    // (what I would love is assigning to slices)
    for (x, &y) in into.iter_mut().skip(pos).take(4).zip(val.to_le_bytes().iter()) {
        *x = y;
    }
}

fn write_u16_into(into: &mut [u8], pos: usize, val: u16) {
    for (x, &y) in into.iter_mut().skip(pos).take(2).zip(val.to_le_bytes().iter()) {
        *x = y;
    }
}

pub fn len_to_block(size: usize) -> UFat {
    (size/BLOCK_SIZE + if size % BLOCK_SIZE != 0 {1} else {0}) as UFat
}

pub fn generate_header(n_state_sector: UFat) -> Vec<u8> {

    // boot block
    let mut boot_and_fsinfo = BOOT_SECTOR.to_vec();
    let fatsize: u32 = len_to_block((2+n_state_sector as usize)*(std::mem::size_of::<UFat>()));
    write_u32_into(&mut boot_and_fsinfo, BOOT_SECTOR_FAT_SZ_32, fatsize);
    write_u32_into(&mut boot_and_fsinfo, BOOT_SECTOR_TOTAL_SEC_32, n_state_sector + 8 + fatsize);
    boot_and_fsinfo.extend_from_slice(&[0u8; BLOCK_SIZE - 2 - BOOT_SECTOR.len()]);
    boot_and_fsinfo.push(0x55);
    boot_and_fsinfo.push(0xaa);

    // fsinfo
    boot_and_fsinfo.extend_from_slice(&FSINFO_HEAD);
    boot_and_fsinfo.extend_from_slice(&[0u8; BLOCK_SIZE - FSINFO_HEAD.len() - FSINFO_TAIL.len()]);
    boot_and_fsinfo.extend_from_slice(&FSINFO_TAIL);
    let mut volume = boot_and_fsinfo.clone();

    volume.extend_from_slice(&[0u8; 4*BLOCK_SIZE]);

    // backup copy in block 6 and 7
    volume.append(&mut boot_and_fsinfo);
    volume
}

pub fn generate_fat<D: DFA>(state_blocks: &StateFatMap<D>, pad: UFat) -> Result<Vec<u8>, &'static str> {
    let mut fat = Vec::new();
    fat.extend_from_slice(&FAT32_EOF);
    fat.extend_from_slice(&FAT32_EOF);
    let mut current_cluster: UFat = 2;
    for state in &state_blocks.order_list {
        let pl = match state_blocks.pos_hash.get(&state) {
            Some(x) => x,
            None => return Err("Refernce to invalid state")
        };
        let size = len_to_block(pl.byte_sized);
        if size == 0 {
            return Err("Zero size state");
        }
        for i in 0..size {
            current_cluster += 1;
            if i == size - 1 {
                fat.extend_from_slice(&FAT32_EOF);
            }
            else {
                fat.extend_from_slice(&current_cluster.to_le_bytes());
            }
        }
    }
    for _ in 0..pad {
        fat.extend_from_slice(&[0xffu8, 0xff, 0xff, 0x0f]);
    }
    if fat.len() % BLOCK_SIZE != 0 {
        fat.extend(
            std::iter::repeat(0u8)
            .take(BLOCK_SIZE - fat.len() % BLOCK_SIZE)
        );
    }
    Ok(fat)
}

const ENTRY_TEMPLATE: [u8; 32] = [
    /*  0 */ 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,   // to be filled in (8.3 name)
    /* 11 */    0,                              // attributes (to be filled in)
    /* 12 */ 0x00,                              // reserved
    /* 13 */ 0x00,                              // creation time deciseconds (0)
    /* 14 */ 0x00, 0x00,                        // creation time
    /* 16 */ 0x00, 0x00,                        // creation date
    /* 18 */ 0x00, 0x00,                        // access date
    /* 20 */    0,    0,                        // to be filled in (cluster high word)
    /* 22 */ 0x00, 0x00,                        // write time
    /* 24 */ 0x21, 0x00,                        // write date (1980-01-01)
    /* 26 */    0,    0,                        // to be filled in (cluster low word)
    /* 28 */    0,    0,    0,    0,            // size for directory is zero
];

pub fn generate_dir_short(letter: u8, target: UFat) -> Vec<u8> {
    let name_8_3: [u8; 11] = if letter == b' ' {
        *b"SPACE      "
    }
    else {
        // fat32 entries are padded with space
        [letter, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20]
    };
    let mut dir_entry = ENTRY_TEMPLATE.to_vec();
    for (x, &y) in dir_entry.iter_mut().take(11).zip(name_8_3.iter()) {
        *x = y;
    }
    dir_entry[11] = 0x11; // read-only (defunct but I'll use it anyway), directory
    write_u16_into(&mut dir_entry, 20, (target >> 16) as u16);
    write_u16_into(&mut dir_entry, 26, (target & 0xffff) as u16);
    // directories have size of zero
    write_u32_into(&mut dir_entry, 28, 0);
    dir_entry
}

pub fn generate_file(name_8_3: [u8; 11], target: UFat) -> Vec<u8> {
    let mut dir_entry = ENTRY_TEMPLATE.to_vec();
    for (x, &y) in dir_entry.iter_mut().take(11).zip(name_8_3.iter()) {
        *x = y;
    }
    dir_entry[11] = 0;
    write_u16_into(&mut dir_entry, 20, (target >> 16) as u16);
    write_u16_into(&mut dir_entry, 26, (target & 0xffff) as u16);
    // just make it a 0-length file, idc
    write_u32_into(&mut dir_entry, 28, 0);
    dir_entry
}
