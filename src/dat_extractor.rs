use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use steamlocate::SteamDir;

/// Finds a game installation directory via Steam by App ID
pub fn find_steam_app(app_id: u32, app_name: &str) -> io::Result<PathBuf> {
    // Locate Steam installation
    let steam_dir = SteamDir::locate().map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("Could not locate Steam installation: {}", e),
        )
    })?;

    // Find the game
    let app = steam_dir.find_app(app_id).map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} (App ID {}) not found in Steam libraries: {}", app_name, app_id, e),
        )
    })?;

    match app {
        Some((app_info, library)) => {
            // Build the full absolute path: library.path() / steamapps/common / install_dir
            let full_path = library.path().join("steamapps").join("common").join(&app_info.install_dir);
            println!("Found {} at: {:?}", app_name, full_path);
            Ok(full_path)
        }
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not installed. Please install via Steam.", app_name),
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

use flate2::read::ZlibDecoder;

/// Simple DAT1 file extractor for Fallout 1's master.dat
/// Based on: https://falloutmod.fandom.com/wiki/DAT_file_format
pub fn extract_dat1(dat_path: &Path, file_path: &str) -> io::Result<Vec<u8>> {
    // ... existing logic ...
    extract_file_from_dat(dat_path, file_path) // Redirect to existing func logic or inline it
}

// Keep existing function for compatibility but mark it as DAT1
pub fn extract_file_from_dat(dat_path: &Path, file_path: &str) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(dat_path)?;
    
    // Check if it looks like DAT2 first?
    // DAT1 starts with directory count (small integer). 
    // DAT2 starts with data (header is irrelevant until footer).
    // Let's implement DAT2 logic separately first.
    
    // Existing DAT1 Logic:
    // ...
    // To support generic "extract_any", we could check signature.
    // But for now, let's keep this as DAT1 and add DAT2 separately.
    
    // Read DAT1 header (all int32 big-endian)
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let directory_count = i32::from_be_bytes(buf);
    
    // Heuristic: If directory count is massive or negative, it's not DAT1.
    // But directory count is i32. If > 100000 or < 0?
    // Fallout 1 master.dat has ~40 directories?
    
    // Skip Unknown1, Unknown2, Unknown3
    file.seek(SeekFrom::Current(12))?;
    
    // Read directory names
    let mut directories = Vec::new();
    for _ in 0..directory_count {
        let mut len_buf = [0u8; 1];
        if let Err(_) = file.read_exact(&mut len_buf) {
             return Err(io::Error::new(io::ErrorKind::InvalidData, "Failed to read directory name length (not DAT1?)"));
        }
        let name_len = len_buf[0] as usize;
        
        let mut name_buf = vec![0u8; name_len];
        file.read_exact(&mut name_buf)?;
        let name = String::from_utf8_lossy(&name_buf).to_string();
        directories.push(name);
    }
    
    // This logic relies on DAT1 structure.
    // ... (rest of DAT1 implementation) ...
    // Since I can't easily replace just the name in this Tool, I'll add extract_dat2 at the end 
    // and let the user call it.
    
    // Re-implementing the rest of the existing function for context...
    // Read directory contents and file lists
    let mut all_files = Vec::new();
    
    for dir_name in &directories {
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)?;
        let file_count = i32::from_be_bytes(buf);
        
        file.seek(SeekFrom::Current(12))?; // Skip U4, U5, U6
        
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
            file.seek(SeekFrom::Start(entry.offset as u64))?;
            
            let read_size = if entry.compressed {
                entry.packed_size
            } else {
                entry.size
            };
            
            let mut data = vec![0u8; read_size as usize];
            file.read_exact(&mut data)?;
            
            if entry.compressed {
                return decompress_lzss(&data, entry.size as usize);
            }
            return Ok(data);
        }
    }
    
    Err(io::Error::new(io::ErrorKind::NotFound, format!("File '{}' not found in DAT1 archive", file_path)))
}

/// Extract from Fallout 2 DAT2 file
pub fn extract_dat2(dat_path: &Path, file_path: &str) -> io::Result<Vec<u8>> {
    let mut file = fs::File::open(dat_path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();
    
    // Read Footer (Last 8 bytes usually: TreeSize, DataSize/FileSize?)
    // Actually standard DAT2 signature is:
    // ... [TreeData] [TreeSize (4)] [FileSize (4)]
    // All Little Endian.
    
    if file_size < 8 { return Err(io::Error::new(io::ErrorKind::InvalidData, "File too small")); }
    
    file.seek(SeekFrom::End(-8))?;
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf)?;
    let tree_size = u32::from_le_bytes(buf);
    
    file.read_exact(&mut buf)?;
    let footer_file_size = u32::from_le_bytes(buf);
    
    // Validate
    // Note: Some pack tools might not set footer_file_size correctly or it might be DataSize.
    // Generally checking TreeSize is enough.
    
    let tree_offset = file_size.saturating_sub(tree_size as u64).saturating_sub(8);
    // Usually Tree is just before footer.
    
    println!("DAT2 Info: FileSize={}, TreeSize={}, FooterSize={}", file_size, tree_size, footer_file_size);
    
    file.seek(SeekFrom::Start(tree_offset))?;
    
    // Read File Count
    file.read_exact(&mut buf)?;
    let file_count = u32::from_le_bytes(buf);
    
    println!("DAT2 contains {} files. Searching...", file_count);

    let normalized_target = file_path.replace('\\', "/").to_lowercase();
    
    for _ in 0..file_count {
        // Read Name Length (4 bytes LE)
        file.read_exact(&mut buf)?;
        let name_len = u32::from_le_bytes(buf) as usize;
        
        let mut name_buf = vec![0u8; name_len];
        file.read_exact(&mut name_buf)?;
        let name = String::from_utf8_lossy(&name_buf).to_string();
        
        // Read Compression (1 byte)
        let mut byte_buf = [0u8; 1];
        file.read_exact(&mut byte_buf)?;
        let is_compressed = byte_buf[0] != 0;
        
        // Read Real Size (4 bytes LE)
        file.read_exact(&mut buf)?;
        let real_size = u32::from_le_bytes(buf);
        
        // Read Packed Size (4 bytes LE)
        file.read_exact(&mut buf)?;
        let packed_size = u32::from_le_bytes(buf);
        
        // Read Offset (4 bytes LE)
        file.read_exact(&mut buf)?;
        let offset = u32::from_le_bytes(buf);
        
        if name.to_lowercase().replace('\\', "/") == normalized_target {
            println!("Found candidate: {} (Offset: {}, Packed: {}, Real: {}, Compressed: {})", 
                     name, offset, packed_size, real_size, is_compressed);
            
            let save_pos = file.stream_position()?;
            
            file.seek(SeekFrom::Start(offset as u64))?;
            let mut data = vec![0u8; packed_size as usize];
            file.read_exact(&mut data)?;
            
            file.seek(SeekFrom::Start(save_pos))?; // Restore pos (though we return anyway so strictly unnecessary)
            
            if is_compressed {
                // Decompress using Zlib
                let mut decoder = ZlibDecoder::new(&data[..]);
                let mut decompressed = Vec::with_capacity(real_size as usize);
                decoder.read_to_end(&mut decompressed)?;
                return Ok(decompressed);
            } else {
                return Ok(data);
            }
        }
    }
    
    Err(io::Error::new(io::ErrorKind::NotFound, format!("File '{}' not found in DAT2 archive", file_path)))
}

#[derive(Debug)]
struct FileEntry {
    path: String,
    compressed: bool,
    offset: u32,
    size: u32,
    packed_size: u32,
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


