use std::env;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::BufWriter;
use std::fs::File;
use std::io::Error;
use std::io::ErrorKind;

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
    if len != 4 || args[1] != "-d" {
        eprintln!("Usage: mytest -d <input> <output>");
        return;
    }
    let input_file_name = &args[2];
    let output_file_name = &args[3];
    let result = decompress(&input_file_name, &output_file_name);
    match result { Ok(bytes) => {
        println!("Decompressed {bytes} bytes");
    }, Err(e) => {
        eprintln!("Failed to decompress {input_file_name} to {output_file_name}: {e}");
    } };
}

fn decompress(input_file_name: &str, output_file_name: &str) -> Result<usize, Error> {
    let in_file = File::open(input_file_name)?;
    let mut reader = BufReader::new(in_file);
    let mut header: Vec<u8> = Vec::new();
    header.resize(7, 0);
    reader.read_exact(&mut header)?;
    let magic = read_u32_le(&header, 0);
    if magic != 0x184D2204 {
        return error(format!("Incorrect magic {magic}").as_str());
    }
    let flags = header[4];
    let version = flags >> 6;
    if version != 1 {
        return error(format!("Unsupported version {version}").as_str());
    }
    let block_independance_flag = ((flags >> 5) & 1) == 1;
    if !block_independance_flag {
        return error("Unsupported block dependence");
    }
    let block_checksum_flag = ((flags >> 4) & 1) == 1;
    if block_checksum_flag {
        return error("Unsupported block checksum flag");
    }
    let content_size_flag = ((flags >> 3) & 1) == 1;
    if content_size_flag {
        return error("Unsupported content size flag");
    }
    let content_checksum_flag = ((flags >> 2) & 1) == 1;
    if content_checksum_flag {
        return error("Unsupported content checksum flag");
    }
    if (flags >> 1) & 1 != 0 {
        return error("Unsupported reserved");
    }
    if (flags & 1) == 1 {
        return error("Unsupported dict flag");
    }
    let bd = header[5];
    let block_max_size = (bd >> 4) & 0x7;
    if block_max_size < 4 || block_max_size > 7 {
        return error(format!("Unsupported block max size {block_max_size}").as_str());
    }
    let header_checksum = header[6];
    let xxhash = (xxhash32(&header, 4, 2, 0) >> 8) & 0xff;
    if xxhash as u8 != header_checksum {
        return error("Header checksum mismatch");
    }
    let mut block: Vec<u8> = Vec::new();
    block.resize(8 * 1024 * 1024, 0);
    let mut out_block: Vec<u8> = Vec::new();
    out_block.resize(8 * 1024 * 1024, 0);
    let out_file = File::create(output_file_name)?;
    let mut writer = BufWriter::new(out_file);
    let mut output_file_size = 0;
    loop {
        reader.read_exact(&mut header[0..4])?;
        let mut block_size = read_u32_le(&header, 0) as usize;
        if block_size == 0 {
            break;
        }
        let uncompressed = ((block_size >> 31) & 1) == 1;
        block_size &= 0x7fffffff;
        if block_size > 4 * 1024 * 1024 {
            return error(format!("Unsupported block size {block_size}").as_str());
        }
        reader.read_exact(&mut block[0..block_size])?;
        if uncompressed {
            writer.write_all(&block)?;
            output_file_size += block_size;
        } else {
            let size = expand(&block, block_size, &mut out_block, 0)?;
            writer.write_all(&out_block[0..size])?;
            output_file_size += size;
        }
    }
    drop(block);
    drop(reader);
    writer.flush()?;
    drop(out_block);
    drop(writer);
    return Ok(output_file_size);
}

fn error(message: &str) -> Result<usize, Error> {
    return Err(Error::new(ErrorKind::Other, message));
}

fn read_u32_le(data: &Vec<u8>, pos: usize) -> u32 {
    return (data[pos] as u32) |
        ((data[pos + 1] as u32) << 8) |
        ((data[pos + 2] as u32) << 16) |
        ((data[pos + 3] as u32) << 24);
}

fn expand(in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
    if in_len > in_data.len() {
        return error("Input buffer too small");
    }
    let mut out_pos: usize = o;
    let mut p = 0;
    loop {
        let tag = in_data[p];
        p += 1;
        let mut literal_len: usize = tag as usize >> 4;
        if literal_len == 0xf {
            loop {
                let x = in_data[p] as usize;
                p += 1;
                literal_len += x;
                if x != 0xff {
                    break;
                }
            }
        }
        for i in 0..literal_len {
             out_data[out_pos + i] = in_data[p + i];
        }
        out_pos += literal_len;
        p += literal_len;
        if p >= in_len - 1 {
            break;
        }
        let offset = ((in_data[p] as u32) |
            ((in_data[p + 1] as u32) << 8)) as usize;
        if offset == 0 {
            return error("Offset 0");
        }
        if offset > out_pos {
            return error("Offset too large");
        }
        p += 2;
        if p >= in_len - 1 {
            break;
        }
        let mut run_len = tag as usize & 0xf;
        if run_len == 0xf {
            loop {
                let x = in_data[p] as usize;
                p += 1;
                run_len += x;
                if x != 0xff {
                    break;
                }
            }
        }
        run_len += 4;
        // copy_within is slightly faster in my test than the loop:
        // for i in 0..run_len {
        //     out_data[out_pos + i] = out_data[out_pos + i - offset];
        // }
        out_data.copy_within(out_pos..out_pos + run_len, out_pos - offset);
        out_pos += run_len;
    }
    return Ok(out_pos);
}

fn xxhash32(buf: &Vec<u8>, start: usize, len: usize, seed: u32) -> u32 {
    let prime1: u32 = 2654435761;
    let prime2: u32 = 2246822519;
    let prime3: u32 = 3266489917;
    let prime4: u32 = 668265263;
    let prime5 : u32 = 374761393;
    let end = start + len;
    let mut pos = start;
    let mut h32;
    if len >= 16 {
        let limit = end - 16;
        let mut v1 = seed.wrapping_add(prime1).wrapping_add(prime2);
        let mut v2 = seed.wrapping_add(prime2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(prime1);
        loop {
            v1 = read_u32_le(buf, pos).
                wrapping_mul(prime2).wrapping_add(v1).
                rotate_left(13).wrapping_mul(prime1);
            v2 = read_u32_le(buf, pos + 4).
                wrapping_mul(prime2).wrapping_add(v2).
                rotate_left(13).wrapping_mul(prime1);
            v3 = read_u32_le(buf, pos + 8).
                wrapping_mul(prime2).wrapping_add(v3).
                rotate_left(13).wrapping_mul(prime1);
            v4 = read_u32_le(buf, pos + 12).
                wrapping_mul(prime2).wrapping_add(v4).
                rotate_left(13).wrapping_mul(prime1);
            pos += 16;
            if pos > limit {
                break;
            }
        }
        h32 = v1.rotate_left(1).
            wrapping_add(v2.rotate_left(7)).
            wrapping_add(v3.rotate_left(12)).
            wrapping_add(v4.rotate_left(18));
    } else {
        h32 = seed.wrapping_add(prime5);
    }
    h32 = h32.wrapping_add(len as u32);
    while pos + 4 <= end {
        h32 = read_u32_le(buf, pos).
            wrapping_mul(prime3).wrapping_add(h32).
            rotate_left(17).wrapping_mul(prime4);
        pos += 4;
    }
    while pos < end {
        h32 = (buf[pos] as u32).
            wrapping_mul(prime5).wrapping_add(h32).
            rotate_left(11).wrapping_mul(prime1);
        pos += 1;
    }
    h32 = (h32 ^ (h32 >> 15)).wrapping_mul(prime2);
    h32 = (h32 ^ (h32 >> 13)).wrapping_mul(prime3);
    return h32 ^ (h32 >> 16);
}