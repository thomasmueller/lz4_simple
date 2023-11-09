use std::io::prelude::*;
use std::io::BufReader;
use std::io::BufWriter;
use std::fs::File;
use std::io::Error;

use crate::xxhash32::read_vec_u32_le;
use crate::xxhash32::XXHash32;
use crate::xxhash32::error;

pub fn decompress_file(input_file_name: &str, output_file_name: &str) -> Result<usize, Error> {
    let in_file = File::open(input_file_name)?;
    let mut reader = BufReader::new(in_file);
    let mut header: Vec<u8> = Vec::new();
    header.resize(7, 0);
    reader.read_exact(&mut header)?;
    let magic = read_vec_u32_le(&header, 0);
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
        // TODO currently, the content checksum flag is ignored
        // return error("Unsupported content checksum flag");
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
    let mut hash = XXHash32::new(0);
    let xxhash = (hash.update(&header, 4, 2)? >> 8) & 0xff;
    if xxhash as u8 != header_checksum {
        return error("Header checksum mismatch");
    }
    let mut block: Vec<u8> = Vec::new();
    block.resize(4 * 1024 * 1024, 0);
    let mut out_block: Vec<u8> = Vec::new();
    out_block.resize(4 * 1024 * 1024, 0);
    let out_file = File::create(output_file_name)?;
    let mut writer = BufWriter::new(out_file);
    let mut output_file_size = 0;
    loop {
        reader.read_exact(&mut header[0..4])?;
        let mut block_size = read_vec_u32_le(&header, 0) as usize;
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
            let size = decompress_block(&block, block_size, &mut out_block, 0)?;
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

pub fn decompress_block(in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
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
        // println!("    out_pos={out_pos} literal_len={literal_len}");
        p += literal_len;
        if p >= in_len - 1 {
            // println!("end out_pos={out_pos} p={p} literal_len={literal_len}");
            break;
        }
        let offset = ((in_data[p] as u32) |
            ((in_data[p + 1] as u32) << 8)) as usize;
        // println!("    offset = {offset}");
        if offset == 0 {
            return error("Offset 0");
        }
        if offset > out_pos {
            return error("Offset too large");
        }
        p += 2;
        if p >= in_len - 1 {
            // println!("end2 out_pos={out_pos} p={p} literal_len={literal_len}");
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
        for i in 0..run_len {
            out_data[out_pos + i] = out_data[out_pos + i - offset];
        }
        out_pos += run_len;
        // println!("    p={p} out_pos={out_pos} run_len={run_len} offset={offset}");
    }
    return Ok(out_pos);
}
