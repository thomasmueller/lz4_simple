use std::cmp::min;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::BufWriter;
use std::fs::File;
use std::io::Error;

use crate::xxhash32::write_vec_u32_le;
use crate::xxhash32::XXHash32;
use crate::xxhash32::error;

pub fn compress_file(input_file_name: &str, output_file_name: &str) -> Result<usize, Error> {
    let in_file = File::open(input_file_name)?;
    let mut remaining = in_file.metadata().unwrap().len();
    let file_size = remaining as usize;
    let mut reader = BufReader::new(in_file);
    let mut header: Vec<u8> = Vec::new();
    header.resize(7, 0);
    let magic = 0x184D2204;
    write_vec_u32_le(&mut header, 0, magic);
    header[4] = (1 << 6) | (1 << 5);
    let bd = 7 << 4;
    header[5] = bd;
    let mut hash = XXHash32::new(0);
    let xxhash = (hash.update(&header, 4, 2)? >> 8) & 0xff;
    header[6] = xxhash as u8;
    let out_file = File::create(output_file_name)?;
    let mut writer = BufWriter::new(out_file);
    writer.write_all(&header)?;
    let mut block: Vec<u8> = Vec::new();
    let block_size = 4 * 1024 * 1024;
    block.resize(block_size, 0);
    let mut out_block: Vec<u8> = Vec::new();
    out_block.resize(5 * 1024 * 1024, 0);
    while remaining > 0 {
        let read = if remaining < block_size as u64 {
            remaining as usize
        } else {
            block_size
        };
        reader.read_exact(&mut block[0..read])?;
        let end = compress_block(&block, read,&mut out_block, 4)?;
        if end >= read {
            // can not compress
            let mut write_block_size = 1 << 31;
            write_block_size |= read;
            write_vec_u32_le(&mut out_block, 0, write_block_size as u32);
            out_block[4..4 + read].copy_from_slice(&block[0..read]);
            writer.write_all(&out_block[0..read + 4])?;
        } else {
            write_vec_u32_le(&mut out_block, 0, (end - 4) as u32);
            writer.write_all(&out_block[0..end])?;
        }
        remaining -= read as u64;
    }
    write_vec_u32_le(&mut out_block, 0, 0);
    writer.write_all(&out_block[0..4])?;
    drop(block);
    drop(reader);
    writer.flush()?;
    drop(out_block);
    drop(writer);
    return Ok(file_size);
}

const HASH_SIZE: usize = 1 << 14;

fn hash64(x: u64) -> u64 {
    let a = (x ^ (x >> 33)).wrapping_mul(0xff51afd7ed558ccd);
    let b = (a ^ (a >> 33)).wrapping_mul(0xc4ceb9fe1a85ec53);
    return b ^ (b >> 33);
}

fn hash(data: &Vec<u8>, pos: usize) -> usize {
    let x: usize = ((data[pos] as usize) << 24)
        | ((data[pos + 1] as usize) << 16)
        | ((data[pos + 2] as usize) << 8)
        | (data[pos + 3] as usize);
    return hash64(x as u64) as usize & (HASH_SIZE - 1) as usize;
}

fn run_len_calc(a: &Vec<u8>, ai: usize, a_len: usize, b: &Vec<u8>, bi: usize, b_len: usize) -> usize {
    let mut run_len = 0;
    while ai + run_len < a_len - 5 &&
        bi + run_len < b_len - 5 &&
        a[ai + run_len] == b[bi + run_len] {
        run_len += 1;
    }
    return run_len;
}

fn compress_block(in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
    // println!("compress_block inlen {in_len} o {o}");
    if in_len > in_data.len() {
        return error("Input buffer too small");
    }
    let mut hash_tab: Vec<usize> = Vec::new();
    hash_tab.resize(HASH_SIZE, 0);
    let mut out_pos = o;
    let mut literal_len = min(4, in_len);
    let mut in_pos = literal_len;
    // println!("start {in_pos} o {o}");
    loop {
        let mut run_len: usize;
        let candidate_pos: usize;
        if in_pos < in_len {
            let h = hash(in_data, in_pos - 4);
            candidate_pos = hash_tab[h];
            if candidate_pos >= in_pos - 4 || candidate_pos < o || candidate_pos < in_pos - 0xffff {
                run_len = 0;
            } else {
                run_len = run_len_calc(in_data, in_pos, in_len,in_data, candidate_pos + 4, in_len);
            }
            hash_tab[h] = in_pos - 4;
            if run_len < 4 {
                literal_len += 1;
                in_pos += 1;
                continue;
            }
        } else {
            run_len = 4;
            candidate_pos = 0;
        }
        let tag_pos = out_pos;
        out_pos += 1;
        let copy_len = literal_len;
        if literal_len >= 0xf {
            while literal_len - 0xf >= 0xff {
                out_data[out_pos] = 0xff;
                out_pos += 1;
                literal_len -= 0xff;
            }
            out_data[out_pos] = (literal_len - 0xf) as u8;
            out_pos += 1;
            literal_len = 0xf;
        }
        for i in 0..copy_len {
            out_data[out_pos] = in_data[in_pos - copy_len + i];
            out_pos += 1;
        }
        let offset = in_pos - (candidate_pos + 4);
        if in_pos < in_len {
            out_data[out_pos] = offset as u8;
            out_data[out_pos + 1] = (offset >> 8) as u8;
            out_pos += 2;
        } else {
            let tag = literal_len << 4;
            out_data[tag_pos] = tag as u8;
            // println!("end in_pos {in_pos} tag {tag}: cp {copy_len} off {offset} {candidate_pos}");
            break;
        }
        let skip_len = run_len;
        run_len -= 4;
        if run_len >= 0xf {
            while run_len - 0xf >= 0xff {
                out_data[out_pos] = 0xff as u8;
                out_pos += 1;
                run_len -= 0xff;
            }
            out_data[out_pos] = (run_len - 0xf) as u8;
            out_pos += 1;
            run_len = 0xf;
        }
        let tag = (literal_len << 4) | run_len;
        out_data[tag_pos] = tag as u8;
        // println!("in_pos {in_pos} tag {tag}: cp {copy_len} skip {skip_len} off {offset} {candidate_pos}");
        in_pos += skip_len;
        literal_len = 4;
        in_pos += 4;
    }
    return Ok(out_pos);
}
