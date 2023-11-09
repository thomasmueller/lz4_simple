use std::env;

use crate::xxhash32::xxhash32_file;
use crate::compress::compress_file;
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
    if len == 4 && args[1] == "-d" {
        let input_file_name = &args[2];
        let output_file_name = &args[3];
        let result = decompress_file(&input_file_name, &output_file_name);
        match result {
            Ok(bytes) => {
                println!("Decompressed {bytes} bytes");
            },
            Err(e) => {
                eprintln!("Failed to decompress {input_file_name} to {output_file_name}: {e}");
            }
        };
    } else if len == 4 && (args[1] == "-1" || args[1] == "-2" || args[1] == "-3"
        || args[1] == "-4" || args[1] == "-5" || args[1] == "-6"
        || args[1] == "-7" || args[1] == "-8" || args[1] == "-9")  {
            let level: usize = args[1].chars().nth(1).unwrap() as usize - '0' as usize;
            let input_file_name = &args[2];
            let output_file_name = &args[3];
            let result = compress_file(&input_file_name, &output_file_name, level);
            match result {
                Ok(bytes) => {
                    println!("Compressed {bytes} bytes");
                },
                Err(e) => {
                    eprintln!("Failed to compress {input_file_name} to {output_file_name}: {e}");
                }
            };
    } else if len == 3 && args[1] == "-h" {
        let input_file_name = &args[2];
        let result = xxhash32_file(&input_file_name);
        match result {
            Ok(hash) => {
                println!("{:08x}", hash);
            },
            Err(e) => {
                eprintln!("Failed to read {input_file_name}: {e}");
            }
        };
    } else {
        eprintln!("Usage:");
        eprintln!("lz4_simple -1 <input> <output>   Compress the input file into the output file");
        eprintln!("lz4_simple -d <input> <output>   Decompress the input file into the output file");
        eprintln!("lz4_simple -h <input>            Calculate the XXHash32 checksum");
    }
}



