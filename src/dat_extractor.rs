use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use steamlocate::SteamDir;

/// Finds the Fallout 1 installation directory via Steam
pub fn find_fallout1_install() -> io::Result<PathBuf> {
    // Locate Steam installation
    let steam_dir = SteamDir::locate().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Could not locate Steam installation: {}", e),
        )
    })?;

    // Fallout 1 App ID is 38400
    let app_id = 38400u32;
    
    // Find the game
    let app = steam_dir.find_app(app_id).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Fallout 1 (App ID {}) not found in Steam libraries: {}", app_id, e),
        )
    })?;

    match app {
        Some((app_info, library)) => {
            // Build the full absolute path: library.path() / steamapps/common / install_dir
            let full_path = library.path().join("steamapps").join("common").join(&app_info.install_dir);
            println!("Found Fallout 1 at: {:?}", full_path);
            Ok(full_path)
        }
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Fallout 1 is not installed. Please install via Steam.",
        )),
    }
}

/// Find a file case-insensitively in a directory
pub fn find_file_case_insensitive(dir: &Path, filename: &str) -> io::Result<PathBuf> {
    let filename_lower = filename.to_lowercase();
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let entry_name = entry.file_name();
        let entry_name_str = entry_name.to_string_lossy();
        
        if entry_name_str.to_lowercase() == filename_lower {
            return Ok(entry.path());
        }
    }
    
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("File '{}' not found in {:?}", filename, dir),
    ))
}

/// Simple DAT1 file extractor for Fallout 1's master.dat
/// Based on: https://falloutmod.fandom.com/wiki/DAT_file_format
pub fn extract_file_from_dat(dat_path: &Path, file_path: &str) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(dat_path)?;
    
    // Read DAT1 header (all int32 big-endian)
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let directory_count = i32::from_be_bytes(buf);
    
    // Skip Unknown1, Unknown2, Unknown3
    file.seek(SeekFrom::Current(12))?;
    
    // Read directory names
    let mut directories = Vec::new();
    for _ in 0..directory_count {
        let mut len_buf = [0u8; 1];
        file.read_exact(&mut len_buf)?;
        let name_len = len_buf[0] as usize;
        
        let mut name_buf = vec![0u8; name_len];
        file.read_exact(&mut name_buf)?;
        let name = String::from_utf8_lossy(&name_buf).to_string();
        directories.push(name);
    }
    
    // Read directory contents and file lists
    let mut all_files = Vec::new();
    
    for dir_name in &directories {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        let file_count = i32::from_be_bytes(buf);
        
        // Skip Unknown4, Unknown5, Unknown6
        file.seek(SeekFrom::Current(12))?;
        
        // Read file list for this directory
        for _ in 0..file_count {
            let mut len_buf = [0u8; 1];
            file.read_exact(&mut len_buf)?;
            let name_len = len_buf[0] as usize;
            
            let mut name_buf = vec![0u8; name_len];
            file.read_exact(&mut name_buf)?;
            let file_name = String::from_utf8_lossy(&name_buf).to_string();
            
            let mut buf = [0u8; 4];
            file.read_exact(&mut buf)?;
            let attributes = i32::from_be_bytes(buf);
            file.read_exact(&mut buf)?;
            let offset = i32::from_be_bytes(buf);
            file.read_exact(&mut buf)?;
            let size = i32::from_be_bytes(buf);
            file.read_exact(&mut buf)?;
            let packed_size = i32::from_be_bytes(buf);
            
            let full_path = if dir_name == "." {
                file_name.clone()
            } else {
                format!("{}/{}", dir_name, file_name)
            };
            
            all_files.push(FileEntry {
                path: full_path,
                compressed: attributes == 0x40,
                offset: offset as u32,
                size: size as u32,
                packed_size: packed_size as u32,
            });
        }
    }
    
    // Find the requested file
    let normalized_path = file_path.replace('\\', "/");
    for entry in &all_files {
        if entry.path.eq_ignore_ascii_case(&normalized_path) {
            // Seek to file data
            file.seek(SeekFrom::Start(entry.offset as u64))?;
            
            let read_size = if entry.compressed {
                entry.packed_size
            } else {
                entry.size
            };
            
            let mut data = vec![0u8; read_size as usize];
            file.read_exact(&mut data)?;
            
            // Handle decompression if needed
            if entry.compressed {
                return decompress_lzss(&data, entry.size as usize);
            }
            
            return Ok(data);
        }
    }
    
    // Debug: print files that might contain location data
    println!("\nSearching DAT archive for location data files...");
    
    let keywords = vec!["city", "map", "world", "location", "area"];
    for keyword in &keywords {
        let matching: Vec<&FileEntry> = all_files.iter()
            .filter(|f| {
                let path_lower = f.path.to_lowercase();
                path_lower.contains(keyword) && !path_lower.contains(".map") && !path_lower.contains(".int")
            })
            .collect();
        
        if !matching.is_empty() {
            println!("\nFiles containing '{}' (excluding .MAP and .INT):", keyword);
            for entry in &matching {
                println!("  {} (compressed: {}, size: {})", entry.path, entry.compressed, entry.size);
            }
        }
    }
    
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("File '{}' not found in DAT archive (searched {} files)", file_path, all_files.len()),
    ))
}

/// Decompress LZSS-compressed data from Fallout 1 DAT files
/// Implementation based on https://falloutmod.fandom.com/wiki/DAT_file_format
fn decompress_lzss(compressed: &[u8], uncompressed_size: usize) -> io::Result<Vec<u8>> {
    const DICT_SIZE: usize = 4096;
    const MIN_MATCH: usize = 3;
    const MAX_MATCH: usize = 18;
    
    let mut output = Vec::with_capacity(uncompressed_size);
    let mut dict = [0x20u8; DICT_SIZE]; // Fill with spaces
    let mut di = DICT_SIZE - MAX_MATCH; // Dictionary index
    
    let mut pos = 0;
    
    while pos < compressed.len() {
        // Read control byte
        let mut flags = compressed[pos];
        pos += 1;
        
        // Process 8 bits
        for _ in 0..8 {
            if pos >= compressed.len() {
                break;
            }
            
            if flags & 1 == 0 {
                // Literal byte
                let byte = compressed[pos];
                pos += 1;
                output.push(byte);
                dict[di] = byte;
                di = (di + 1) % DICT_SIZE;
            } else {
                // Match reference
                if pos + 1 >= compressed.len() {
                    break;
                }
                
                let b1 = compressed[pos] as usize;
                let b2 = compressed[pos + 1] as usize;
                pos += 2;
                
                let dict_offset = b1 | ((b2 & 0xF0) << 4);
                let match_len = (b2 & 0x0F) + MIN_MATCH;
                
                for _ in 0..match_len {
                    let byte = dict[dict_offset % DICT_SIZE];
                    output.push(byte);
                    dict[di] = byte;
                    di = (di + 1) % DICT_SIZE;
                }
            }
            
            flags >>= 1;
        }
    }
    
    Ok(output)
}

#[derive(Debug)]
struct FileEntry {
    path: String,
    compressed: bool,
    offset: u32,
    size: u32,
    packed_size: u32,
}
