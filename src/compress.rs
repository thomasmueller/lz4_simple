use std::cmp::min;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::BufWriter;
use std::fs::File;
use std::io::Error;

use crate::xxhash32::read_vec_u32_le;
use crate::xxhash32::read_u64_le;
use crate::xxhash32::write_vec_u32_le;
use crate::xxhash32::XXHash32;
use crate::xxhash32::error;
use crate::xxhash32::read_fully;

use std::cmp::Ordering;

pub fn compress_stream(level: usize) -> Result<usize, Error> {
    return compress(std::io::stdin(), std::io::stdout(), level);
}

pub fn compress_file(input_file_name: &str, output_file_name: &str, level: usize) -> Result<usize, Error> {
    let in_file = File::open(input_file_name)?;
    let out_file = File::create(output_file_name)?;
    return compress(in_file, out_file, level);
}

pub fn compress<R: Read, W: Write>(read: R, write: W, level: usize) -> Result<usize, Error> {
    let mut reader = BufReader::new(read);
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
    let mut writer = BufWriter::new(write);
    writer.write_all(&header)?;
    let mut block: Vec<u8> = Vec::new();
    let block_size = 4 * 1024 * 1024;
    block.resize(block_size, 0);
    let mut out_block: Vec<u8> = Vec::new();
    out_block.resize(5 * 1024 * 1024, 0);
    let mut comp = Compress::new(5 * 1024 * 1024, level);
    let mut total_size = 0;
    loop {
        let read = read_fully(&mut reader,&mut block[0..block_size])?;
        if read == 0 {
            break;
        }
        total_size += read;
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
    }
    write_vec_u32_le(&mut out_block, 0, 0);
    writer.write_all(&out_block[0..4])?;
    drop(block);
    drop(reader);
    writer.flush()?;
    drop(out_block);
    drop(writer);
    return Ok(total_size);
}

fn hash64(x: u64) -> u64 {
    let a = (x ^ (x >> 33)).wrapping_mul(0xff51afd7ed558ccd);
    let b = (a ^ (a >> 33)).wrapping_mul(0xc4ceb9fe1a85ec53);
    return b ^ (b >> 33);
}

fn hash(data: &Vec<u8>, pos: usize, mask: usize) -> usize {
    let x = read_vec_u32_le(data, pos);
    return hash64(x as u64) as usize & mask as usize;
}

fn hash5(data: &Vec<u8>, pos: usize) -> usize {
    let x: u64 = read_u64_le(data, pos);
    let prime5bytes: u64 = 889523592379;
    return ((x << 24).wrapping_mul(prime5bytes) >> (64 - 12)) as usize;
}

fn compare_at(data: &Vec<u8>, a: &usize, b: &usize) -> Ordering {
    let mut max = min(100000, data.len() - a);
    max = min(max, data.len() - b);
    for i in 0..max {
        let r = data[a + i].cmp(&data[b + i]);
        if r != Ordering::Equal {
            return r;
        }
    }
    return a.cmp(&b);
}

fn run_len_count(a: &Vec<u8>, a_len: usize, ai: usize, bi: usize) -> usize {
    let mut run_len = 0;
    while ai + run_len < a_len - 24 {
        let ax =  read_u64_le(a, ai + run_len);
        let bx =  read_u64_le(a, bi + run_len);
        let diff = ax ^ bx;
        if diff == 0 {
            run_len += 8;
        } else {
            run_len += (diff.trailing_zeros() >> 3) as usize;
            return run_len;
        }
    }
    while ai + run_len < a_len - 16 &&
        a[ai + run_len] == a[bi + run_len] {
        run_len += 1;
    }
    return run_len;
}

fn run_len_backwards(a: &Vec<u8>, a_len: usize, ai: usize, bi: usize, min: usize) -> usize {
    //return run_len_count(a, a_len, ai, bi);
    if ai + min + 1 >= a_len - 32 {
        return run_len_count(a, a_len, ai, bi);
    }
    let mut run_len = min + 1;
    while run_len != 0 {
        if a[ai + run_len] != a[bi + run_len] {
            return 0;
        }
        run_len -= 1;
    }
    if a[ai] != a[bi] {
        return 0;
    }
    run_len = min + 1;
    while ai + run_len < a_len - 16 &&
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
        let mask = (1 << (12 + level)) - 1;
        let mut hash: Vec<u32> = Vec::new();
        hash.resize(mask + 1, u32::MAX);
        let mut chain: Vec<u32> = Vec::new();
        chain.resize(len, u32::MAX);
        let stop_at_match_len = level * 10;
        let max_search = 1 << level;
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
        if self.level >= 9 {
            return self.compress_block_very_slow(in_data, in_len, out_data, o);
        } else if self.level > 1 {
            return self.compress_block_slow(in_data, in_len, out_data, o);
        }
        if in_len > in_data.len() {
            return error("Input buffer too small");
        }
        if in_len > self.len {
            return error("Temporary buffer too small");
        }
        let mut out_pos = o;
        let mut literal_len = 0;
        let mut in_pos = 0;
        let skip_trigger = 6;
        let mut search_match = 1 << skip_trigger;
        loop {
            let mut run_len: usize;
            let mut candidate_pos: usize;
            if in_pos + 16 < in_len {
                let h = hash5(in_data, in_pos);
                candidate_pos = self.hash_tab[h] as usize;
                self.hash_tab[h] = in_pos as u32;
                if candidate_pos >= in_pos || candidate_pos + 0xffff < in_pos {
                    let step = search_match >> skip_trigger;
                    literal_len += step;
                    in_pos += step;
                    search_match += 1;
                    continue;
                } else {
                    run_len = run_len_count(in_data, in_len, in_pos, candidate_pos);
                    if run_len < 4 {
                        let step = search_match >> skip_trigger;
                        literal_len += step;
                        in_pos += step;
                        search_match += 1;
                        continue;
                    }
                    let p = in_pos + run_len - 2;
                    let h = hash5(in_data, p);
                    self.hash_tab[h] = p as u32;
                }
            } else {
                // we reached the last few bytes in the block,
                // which are always encoded as literals
                literal_len += in_len - in_pos;
                in_pos = in_len;
                run_len = 4;
                candidate_pos = 0;
            }
            let tag_pos = out_pos;
            out_pos += 1;
            while candidate_pos > 0 && literal_len > 0 && in_pos > 0 && in_data[in_pos - 1] == in_data[candidate_pos - 1] {
                run_len += 1;
                in_pos -= 1;
                literal_len -= 1;
                candidate_pos -= 1;
            }
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
            in_pos += skip_len;
            literal_len = 0;
            search_match = 1 << skip_trigger;
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
        let mut literal_len = 0;
        let mut in_pos = 0;
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
                    if candidate_pos >= in_pos || candidate_pos + 0xffff < in_pos {
                        break;
                    } else {
                        run_len = run_len_backwards(in_data, in_len, in_pos, candidate_pos, best_run_len);
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
            literal_len = 0;
        }
        return Ok(out_pos);
    }

    fn compress_block_very_slow(&mut self, in_data: &Vec<u8>, in_len: usize, out_data: &mut Vec<u8>, o: usize) -> Result<usize, Error> {
        if in_len > in_data.len() {
            return error("Input buffer too small");
        }
        if in_len > self.len {
            return error("Temporary buffer too small");
        }

        let mut match_offsets: Vec<u32> = Vec::new();
        match_offsets.resize(in_len, 0);
        let mut match_lens: Vec<u32> = Vec::new();
        match_lens.resize(in_len, 0);

        let mut indexes: Vec<usize> = Vec::new();
        indexes.resize(in_len, 0);
        let mut block_start = 0;
        while block_start < in_len {
            let block_end = min(block_start + 0x20000, indexes.len());
            for i in block_start..block_end {
                indexes[i] = i;
            }
            indexes[block_start..block_end].sort_by(|a, b| { return compare_at(&in_data, a, b) });
            let update_start = if block_start == 0 { 0 } else { block_start + 0x10000 };
            for x in block_start..block_end {
                let a = indexes[x];
                if a < update_start || a >= block_end {
                    continue;
                }
                for i in 1..1000 {
                    if x >= block_start + i {
                        let b1 = indexes[x - i];
                        if a > b1 && a - b1 < 0xffff {
                            let run_len = run_len_count(in_data, in_len, a, b1);
                            if run_len >= 4 && run_len > match_lens[a] as usize {
                                match_lens[a] = run_len as u32;
                                match_offsets[a] = (a - b1) as u32;
                            }
                            break;
                        }
                    }
                }
                for i in 1..1000 {
                    if x + i < block_end {
                        let b2 = indexes[x + i];
                        if a > b2 && a - b2 < 0xffff {
                            let run_len = run_len_count(in_data, in_len, a, b2);
                            if run_len >= 4 && run_len > match_lens[a] as usize {
                                match_lens[a] = run_len as u32;
                                match_offsets[a] = (a - b2) as u32;
                            }
                            break;
                        }
                    }
                }
            }
            block_start += 0x10000;
        }
        let mut in_pos = in_len - 12;
        // minimum cost (compressed size) from each position (0 if unknown) in bytes
        let mut costs: Vec<usize> = Vec::new();
        costs.resize(in_len, 0);
        let mut literal_count = 0;
        for i in in_len - 12 .. in_len {
            costs[i] = literal_count + 1;
            literal_count += 1;
        }
        let mut best_len: usize;
        while in_pos > 0 {
            // assume literal
            literal_count += 1;
            best_len = 1;
            let mut cost = costs[in_pos + 1] + 1;
            if literal_count >= 15 {
                if literal_count == 15 || ((literal_count - 15) % 255 == 0) {
                    cost += 1;
                }
            }
            let run_len = match_lens[in_pos] as usize;
            let offset = match_offsets[in_pos] as usize;
            if run_len >= 4 && offset != 0 {
                 if offset == 1 {
                    // short offset
                    best_len = run_len;
                    cost = costs[in_pos + run_len] + 3;
                } else {
                     let mut run_len_cost = 3;
                     let mut next_cost_increase = 18;
                     for i in 4..run_len + 1 {
                         let cost2 = costs[in_pos + i] + run_len_cost;
                         if cost2 <= cost {
                             cost = cost2;
                             best_len = i;
                         }
                         if i == next_cost_increase {
                             run_len_cost += 1;
                             next_cost_increase += 255;
                         }
                     }
                }
            }
            costs[in_pos] = cost;
            match_lens[in_pos] = best_len as u32;
            if best_len >= 4 {
                literal_count = 0;
            }
            in_pos -= 1;
        }
        let mut out_pos = o;
        let mut literal_len = min(4, in_len);
        let mut in_pos = literal_len;
        loop {
            let mut run_len: usize;
            if in_pos + 16 < in_len {
                run_len = match_lens[in_pos] as usize;
                if run_len < 4 {
                    literal_len += 1;
                    in_pos += 1;
                    continue;
                }
            } else {
                // we reached the last few bytes in the block,
                // which are always encoded as literals
                literal_len += in_len - in_pos;
                in_pos = in_len;
                run_len = 4;
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
                let offset = match_offsets[in_pos] as usize;
                out_data[out_pos] = offset as u8;
                out_data[out_pos + 1] = (offset >> 8) as u8;
                out_pos += 2;
            } else {
                let tag = literal_len << 4;
                out_data[tag_pos] = tag as u8;
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
            in_pos += skip_len;
            literal_len = 0;
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
        for level in 1..10 {
            let mut comp = Compress::new(1024, level);
            let end = comp.compress_block(&block, block.len(), &mut out_block, 0).unwrap();
            let mut test_block: Vec<u8> = Vec::new();
            test_block.resize(1024, 0);
            let test_end = decompress_block(&out_block, end, &mut test_block, 0).unwrap();
            assert_eq!(test_end, 1024, "level {level}");
            for i in 0..1024 {
                assert_eq!(test_block[i], block[i], "at {i}");
            }
        }
    }

}