mod fat32;
use clap::{App, Arg};
use fat32::{StateFatMap, StatePosInfo, UFat};
use rand::{thread_rng, seq::SliceRandom};
use regex_automata::{dense, DFA};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::process::exit;

const FORBIDDEN_PRINT_ASCII: [u8; 17] = [
    0x22, 0x2a, 0x2b, 0x2c, 0x2e, 0x2f, 0x3a, 0x3b, 0x3c, 0x3c, 0x3d, 0x3e, 0x3f, 0x5b, 0x5c, 0x5d,
    0x7c,
];

// precalculate the position of every dfa state inside the fat table so we can
// later replace the referenced state numbers by the fat entry when writing
// directories
fn determine_state_positions<D: DFA>(
    dfa: &D,
    validlist: &[u8],
    nomatch: bool,
) -> Result<StateFatMap<D>, &'static str> {
    let nomatch_len = (validlist.len() + if nomatch { 1 } else { 0 }) * 32;
    let match_len = (validlist.len() + 1) * 32;
    // root directory starts at 2
    let mut current_block: UFat = 2;
    let mut current_index: usize = 0;
    // vector of visited states in order of visit
    let mut state_vec = Vec::new();
    // map state numbers to StatePosInfos
    let mut state_pos_hash = HashMap::new();
    // keep track of visited states
    let mut state_set = HashSet::new();
    state_vec.push(dfa.start_state());

    while let Some(&current_state) = state_vec.get(current_index) {
        // queue all unvisited states from current state
        for &next_byte in validlist {
            let next_state = dfa.next_state(current_state, next_byte);
            if state_set.insert(next_state) {
                state_vec.push(next_state);
            }
        }
        current_index += 1;
    }

    state_vec[1..].shuffle(&mut thread_rng());

    for &current_state in &state_vec {
        // relevant for size of directory (but mostly not because it's constant
        // and they're both the same)
        let size = if dfa.is_match_state(current_state) {
            match_len
        } else {
            nomatch_len
        };
        state_pos_hash.insert(
            current_state,
            StatePosInfo {
                block: current_block,
                byte_sized: size,
            },
        );
        match current_block.checked_add(fat32::len_to_block(size)) {
            Some(val) => {
                current_block = val;
            }
            None => return Err("State machine exceeds Fate32 capacity!"),
        }
    }
    Ok(StateFatMap {
        blocks: current_block - 2,
        order_list: state_vec,
        pos_hash: state_pos_hash,
    })
}

fn regex_to_fat32<D: DFA, W: Write>(
    dfa: &D,
    validlist: &[u8],
    mut vol: W,
    nomatch: bool,
) -> Result<(), Box<dyn Error>> {
    let state_blocks = determine_state_positions(&dfa, &validlist, nomatch)?;
    // pad until at least 65536 blocks, since otherwise ideologically
    // I would have to implement fat12/fat16
    // also keep at least one free block for match file (which is 0 bytes,
    // but I'm not sure if it needs to reference a valid block)
    let pad = 1isize.max(65536 - state_blocks.blocks as isize) as UFat;
    vol.write_all(&fat32::generate_header(state_blocks.blocks + pad))?;
    vol.write_all(&fat32::generate_fat(&state_blocks, pad)?)?;
    for &state in &state_blocks.order_list {
        let mut current_dir = Vec::<u8>::new();
        // generate directories for each possible character
        for &c in validlist {
            let next_state = dfa.next_state(state, c);
            // maps the state to the block where the state directory is
            let &state_block = &state_blocks.pos_hash[&next_state].block;
            current_dir.append(&mut fat32::generate_dir_short(c, state_block));
        }
        // if accepting state, put match file into dir
        if dfa.is_match_state(state) {
            current_dir.append(&mut fat32::generate_file(*b"MATCH      ", state_blocks.blocks + 2))
        } else if nomatch {
            current_dir.append(&mut fat32::generate_file(*b"NOMATCH    ", state_blocks.blocks + 2))
        }
        if current_dir.len() % fat32::BLOCK_SIZE == 0 {
            vol.write_all(&current_dir)?;
            continue;
        }
        // fill up current block to multiple of BLOCK_SIZE
        current_dir.extend(
            std::iter::repeat(0u8).take(fat32::BLOCK_SIZE - current_dir.len() % fat32::BLOCK_SIZE),
        );
        vol.write_all(&current_dir)?;
    }
    let emptyblock = &[0u8; fat32::BLOCK_SIZE];
    // make space for one more (match file)
    for _ in 0..pad {
        vol.write_all(emptyblock)?;
    }
    Ok(())
}

fn main() {
    let matches =
        App::new("regex2fat")
            .version("0.1.0")
            .author("8051Enthusiast")
            .about("Convert regex DFAs to FAT32 file systems")
            .arg(
                Arg::with_name("anchor")
                    .short("a")
                    .long("anchor")
                    .help("Anchor regex at beginning (off by default)"),
            )
            .arg(
                Arg::with_name("pattern")
                    .required(true)
                    .index(1)
                    .help("The regex pattern to match"),
            )
            .arg(
                Arg::with_name("outfile")
                    .required(true)
                    .index(2)
                    .help("The file to write the fat fs to"),
            )
            .arg(
                Arg::with_name("nomatch")
                    .short("n")
                    .long("nomatch")
                    .help("Generate NOMATCH files (off by default)"),
            )
            .arg(
                Arg::with_name("randomize")
                    .short("r")
                    .long("randomize")
                    .help("Randomize cluster numbers for the states (off by default)"),
            )
            .get_matches();
    let pattern = matches.value_of("pattern").unwrap();
    let dfa = dense::Builder::new()
        // fat32 is case insensitive
        .case_insensitive(true)
        .anchored(matches.is_present("anchor"))
        .build(pattern)
        .unwrap_or_else(|err| {
            eprintln!("Could not compile regex '{}': {}", pattern, err);
            exit(1);
        });
    let validlist: Vec<u8> = (0x20..0x61)
        .chain(0x7b..0x7e)
        .filter(|c| !FORBIDDEN_PRINT_ASCII.contains(c))
        .collect();
    let outfile = matches.value_of("outfile").unwrap();
    let file = File::create(outfile).unwrap_or_else(|err| {
        eprintln!("Could not open file '{}': {}", outfile, err);
        exit(1);
    });
    let nomatch = matches.is_present("nomatch");
    regex_to_fat32(&dfa, &validlist, file, nomatch).unwrap_or_else(|err| {
        eprintln!("Could not write DFA to '{}': {}", outfile, err);
        exit(1);
    });
}
