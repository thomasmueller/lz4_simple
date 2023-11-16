use std::env;

use crate::xxhash32::xxhash32_file;
use crate::xxhash32::xxhash32_stream;
use crate::compress::compress_stream;
use crate::compress::compress_file;
use crate::decompress::decompress_stream;
use crate::decompress::decompress_file;

mod xxhash32;
mod compress;
mod decompress;

// See http://fastcompression.blogspot.com/2011/05/lz4-explained.html
// compile optimized:
// cargo build --release
// IN=...
// OUT=...
// time target/release/lz4_simple -d ${IN} ${OUT}
// time lz4 -d -f ${IN} ${OUT}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    let len = env::args().len();
    let mut success = false;
    if len > 2 && args[1] == "-d" {
        if len == 4 {
            let input_file_name = &args[2];
            let output_file_name = &args[3];
            let result = decompress_file(&input_file_name, &output_file_name);
            match result {
                Ok(bytes) => {
                    println!("Decompressed {bytes} bytes");
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to decompress {input_file_name} to {output_file_name}: {e}");
                }
            };
        } else if len == 3 && args[2] == "-" {
            let result = decompress_stream();
            match result {
                Ok(_bytes) => {
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to decompress stdin: {e}");
                }
            };
        }
    } else if len > 2 && (args[1] == "-1" || args[1] == "-2" || args[1] == "-3"
        || args[1] == "-4" || args[1] == "-5" || args[1] == "-6"
        || args[1] == "-7" || args[1] == "-8" || args[1] == "-9")  {
        let level: usize = args[1].chars().nth(1).unwrap() as usize - '0' as usize;
        if len == 4 {
            let input_file_name = &args[2];
            let output_file_name = &args[3];
            let result = compress_file(&input_file_name, &output_file_name, level);
            match result {
                Ok(bytes) => {
                    println!("Compressed {bytes} bytes");
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to compress {input_file_name} to {output_file_name}: {e}");
                }
            };
        } else if len == 3 && args[2] == "-" {
            let result = compress_stream(level);
            match result {
                Ok(_bytes) => {
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to compress: {e}");
                }
            };
        }
    } else if len == 3 && args[1] == "-h" {
        if args[2] == "-" {
            let result = xxhash32_stream();
            match result {
                Ok(hash) => {
                    println!("{:08x}", hash);
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to read: {e}");
                }
            };
        } else {
            let input_file_name = &args[2];
            let result = xxhash32_file(&input_file_name);
            match result {
                Ok(hash) => {
                    println!("{:08x}", hash);
                    success = true;
                },
                Err(e) => {
                    eprintln!("Failed to read {input_file_name}: {e}");
                }
            };
        }
    }
    if !success {
        eprintln!("Usage:");
        eprintln!("  lz4_simple [-1 .. -9] <input> <output>   Compress (1 fast,... 9 slow)");
        eprintln!("  lz4_simple -d         <input> <output>   Decompress");
        eprintln!("  lz4_simple -h         <input>            Calculate the XXHash32 checksum");
        eprintln!("Use '-' instead of <input> <output> to read from standard input and write to standard output");
    }
}
