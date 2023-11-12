use std::cmp::min;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::BufWriter;
use std::fs::File;
use std::io::Error;

use crate::xxhash32::write_vec_u32_le;
use crate::xxhash32::XXHash32;
use crate::xxhash32::error;

pub fn compress_file(input_file_name: &str, output_file_name: &str, level: usize) -> Result<usize, Error> {
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
    let mut comp = Compress::new(5 * 1024 * 1024, level);
    while remaining > 0 {
        let read = if remaining < block_size as u64 {
            remaining as usize
        } else {
            block_size
        };
        reader.read_exact(&mut block[0..read])?;
        let end = comp.compress_block(&block, read,&mut out_block, 4)?;
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

fn hash64(x: u64) -> u64 {
    let a = (x ^ (x >> 33)).wrapping_mul(0xff51afd7ed558ccd);
    let b = (a ^ (a >> 33)).wrapping_mul(0xc4ceb9fe1a85ec53);
    return b ^ (b >> 33);
}

fn hash(data: &Vec<u8>, pos: usize, mask: usize) -> usize {
    let x: usize = ((data[pos] as usize) << 24)
        | ((data[pos + 1] as usize) << 16)
        | ((data[pos + 2] as usize) << 8)
        | (data[pos + 3] as usize);
    return hash64(x as u64) as usize & mask as usize;
}

fn run_len_calc(a: &Vec<u8>, ai: usize, a_len: usize, bi: usize, b_len: usize) -> usize {
    let mut run_len = 0;
    while ai + run_len < a_len - 16 &&
        bi + run_len < b_len - 16 &&
        a[ai + run_len] == a[bi + run_len] {
        run_len += 1;
    }
    return run_len;
}

struct Compress {
    hash_tab: Vec<u32>,
    chain: Vec<u32>,
    len: usize,
    stop_at_match_len: usize,
    max_search: usize,
    mask: usize,
    step: usize,
    level: usize
}

impl Compress {
    pub fn new(len: usize, level: usize) -> Compress {
        let mask = (1 << (15 + level)) - 1;
        let mut hash: Vec<u32> = Vec::new();
        hash.resize(mask + 1, u32::MAX);
        let mut chain: Vec<u32> = Vec::new();
        chain.resize(len, u32::MAX);
        let stop_at_match_len = level * 20;
        let max_search = 5 * level - 4;
        let step = if level == 1 { 4 } else { 1 };
        Compress {
            hash_tab: hash,
            chain,
            len,
            stop_at_match_len,
            max_search,
            mask,
            step,
            level
        }
    }

    fn compress_block(&mut self, in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
        if self.level > 1 {
            return self.compress_block_slow(in_data, in_len, out_data, o);
        }
        if in_len > in_data.len() {
            return error("Input buffer too small");
        }
        if in_len > self.len {
            return error("Temporary buffer too small");
        }
        let mut out_pos = o;
        let mut literal_len = min(4, in_len);
        let mut in_pos = literal_len;
        loop {
            let mut run_len: usize;
            let candidate_pos: usize;
            if in_pos + 16 < in_len {
                let h = hash(in_data, in_pos, self.mask);
                candidate_pos = self.hash_tab[h] as usize;
                if in_pos & 3 == 0 {
                    self.hash_tab[h] = in_pos as u32;
                }
                if candidate_pos >= in_pos || candidate_pos < o || candidate_pos + 0xffff < in_pos {
                    literal_len += 1;
                    in_pos += 1;
                    continue;
                } else {
                    run_len = run_len_calc(in_data, in_pos, in_len, candidate_pos, in_len);
                    if run_len < 4 {
                        literal_len += 1;
                        in_pos += 1;
                        continue;
                    }
                    for i in (1..run_len).step_by(5) {
                        let p = in_pos + i;
                        let h = hash(in_data, p, self.mask);
                        self.hash_tab[h] = p as u32;
                    }
                }
            } else {
                // we reached the last few bytes in the block,
                // which are always encoded as literals
                literal_len += in_len - in_pos;
                in_pos = in_len;
                run_len = 4;
                candidate_pos = 0;
                // println!("last literal block in_len {in_len} in_pos {in_pos} literals: {literal_len} ");
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
                let x = in_data[in_pos - copy_len + i];
                out_data[out_pos + i] = x;
            }
            out_pos += copy_len;
            if in_pos < in_len {
                let offset = in_pos - candidate_pos;
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
            literal_len = 1;
            in_pos += 1;
        }
        return Ok(out_pos);
    }

    fn compress_block_slow(&mut self, in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
        if in_len > in_data.len() {
            return error("Input buffer too small");
        }
        if in_len > self.len {
            return error("Temporary buffer too small");
        }
        let mut out_pos = o;
        let mut literal_len = min(4, in_len);
        let mut in_pos = literal_len;
        loop {
            let mut run_len: usize;
            let mut candidate_pos: usize;
            if in_pos + 16 < in_len {
                let h = hash(in_data, in_pos, self.mask);
                let first_candidate = self.hash_tab[h];
                self.chain[in_pos] = first_candidate;
                self.hash_tab[h] = in_pos as u32;
                candidate_pos = first_candidate as usize;
                let mut best_candidate: usize = 0;
                let mut best_run_len: usize = 0;
                for _ in 0..self.max_search {
                    if candidate_pos >= in_pos || candidate_pos < o || candidate_pos + 0xffff < in_pos {
                        break;
                    } else {
                        run_len = run_len_calc(in_data, in_pos, in_len, candidate_pos, in_len);
                    }
                    if run_len > best_run_len {
                        best_run_len = run_len;
                        best_candidate = candidate_pos;
                        if run_len > self.stop_at_match_len {
                            // long enough
                            break;
                        }
                    }
                    candidate_pos = self.chain[candidate_pos] as usize;
                }
                candidate_pos = best_candidate;
                run_len = best_run_len;
                if run_len < 4 {
                    literal_len += 1;
                    in_pos += 1;
                    continue;
                } else {
                    for i in (1..run_len).step_by(self.step) {
                        let p = in_pos + i;
                        let h = hash(in_data, p, self.mask);
                        let c = self.hash_tab[h];
                        self.chain[p] = c;
                        self.hash_tab[h] = p as u32;
                    }
                }
            } else {
                // we reached the last few bytes in the block,
                // which are always encoded as literals
                literal_len += in_len - in_pos;
                in_pos = in_len;
                run_len = 4;
                candidate_pos = 0;
                // println!("last literal block in_len {in_len} in_pos {in_pos} literals: {literal_len} ");
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
                let x = in_data[in_pos - copy_len + i];
                out_data[out_pos + i] = x;
            }
            out_pos += copy_len;
            if in_pos < in_len {
                let offset = in_pos - candidate_pos;
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
            literal_len = 1;
            in_pos += 1;
        }
        return Ok(out_pos);
    }
}

#[cfg(test)]
mod tests {
    use crate::decompress::decompress_block;
    use super::*;

    #[test]
    fn compress_decompress() {
        let mut block: Vec<u8> = Vec::new();
        block.resize(1024, 0);
        for i in 0 .. 1024 {
            block[i] = (i & 0xf) as u8;
        }
        let mut out_block: Vec<u8> = Vec::new();
        out_block.resize(2 * 1024, 0);
        let mut comp = Compress::new(1024, 1);
        let end = comp.compress_block(&block, block.len(), &mut out_block, 0).unwrap();
        let mut test_block: Vec<u8> = Vec::new();
        test_block.resize(1024, 0);
        let test_end = decompress_block(&out_block, end, &mut test_block, 0).unwrap();
        assert_eq!(test_end, 1024);
        for i in 0 .. 1024 {
            assert_eq!(test_block[i], block[i]);
        }
    }

}