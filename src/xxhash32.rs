use std::io::prelude::*;
use std::io::BufReader;
use std::fs::File;
use std::io::Error;
use std::io::ErrorKind;

pub fn error(message: &str) -> Result<usize, Error> {
    return Err(Error::new(ErrorKind::Other, message));
}

pub fn read_fully<R: Read>(mut read: R, buffer: &mut [u8]) -> Result<usize, Error> {
    let mut count: usize = 0;
    loop {
        let r = read.read(&mut buffer[count..]);
        match r {
            Ok(0) => {
                return Ok(count)
            },
            Ok(x) => {
                count += x
            },
            Err(e) => {
                if e.kind() == ErrorKind::Interrupted {
                    // retry
                } else {
                    return Err(e);
                }
            }
        };
    }
}

pub fn read_vec_u32_le(data: &Vec<u8>, pos: usize) -> u32 {
    return (data[pos] as u32) |
        ((data[pos + 1] as u32) << 8) |
        ((data[pos + 2] as u32) << 16) |
        ((data[pos + 3] as u32) << 24);
}

pub fn write_vec_u32_le(data: &mut Vec<u8>, pos: usize, x: u32) {
    data[pos] = x as u8;
    data[pos + 1] = (x >> 8) as u8;
    data[pos + 2] = (x >> 16) as u8;
    data[pos + 3] = (x >> 24) as u8;
}

pub fn read_u32_le(data: &[u8], pos: usize) -> u32 {
    return (data[pos] as u32) |
        ((data[pos + 1] as u32) << 8) |
        ((data[pos + 2] as u32) << 16) |
        ((data[pos + 3] as u32) << 24);
}

pub fn xxhash32_stream() -> Result<u32, Error> {
    return xxhash32(std::io::stdin());
}

pub fn xxhash32_file(input_file_name: &str) -> Result<u32, Error> {
    let in_file = File::open(input_file_name)?;
    return xxhash32(in_file);
}

fn xxhash32<R: Read>(read: R) -> Result<u32, Error> {
    let mut reader = BufReader::new(read);
    let mut block: Vec<u8> = Vec::new();
    let block_size = 4 * 1024 * 1024;
    block.resize(block_size, 0);
    let mut hash = 0;
    let mut state = XXHash32::new(0);
    loop {
        let read = read_fully(&mut reader, &mut block)?;
        if read == 0 {
            break;
        }
        hash = state.update(&block, 0, read)?;
    }
    return Ok(hash);
}

const PRIME1: u32 = 2654435761;
const PRIME2: u32 = 2246822519;
const PRIME3: u32 = 3266489917;
const PRIME4: u32 = 668265263;
const PRIME5: u32 = 374761393;

pub struct XXHash32 {
    v1: u32,
    v2: u32,
    v3: u32,
    v4: u32,
    total: usize
}

impl XXHash32 {
    pub fn new(seed: u32) -> XXHash32 {
        XXHash32 {
            v1: seed.wrapping_add(PRIME1).wrapping_add(PRIME2),
            v2: seed.wrapping_add(PRIME2),
            v3: seed,
            v4: seed.wrapping_sub(PRIME1),
            total: 0
        }
    }

    pub fn update(&mut self, buf: &Vec<u8>, start: usize, len: usize) -> Result<u32, Error> {
        let end = start + len;
        let mut pos = start;
        if len >= 16 {
            let limit = end - 16;
            let mut v1 = self.v1;
            let mut v2 = self.v2;
            let mut v3 = self.v3;
            let mut v4 = self.v4;
            loop {
                let sb: [u8; 16] = buf[pos..pos + 16].try_into().unwrap();
                v1 = read_u32_le(&sb, 0).
                    wrapping_mul(PRIME2).wrapping_add(v1).
                    rotate_left(13).wrapping_mul(PRIME1);
                v2 = read_u32_le(&sb, 4).
                    wrapping_mul(PRIME2).wrapping_add(v2).
                    rotate_left(13).wrapping_mul(PRIME1);
                v3 = read_u32_le(&sb, 8).
                    wrapping_mul(PRIME2).wrapping_add(v3).
                    rotate_left(13).wrapping_mul(PRIME1);
                v4 = read_u32_le(&sb, 12).
                    wrapping_mul(PRIME2).wrapping_add(v4).
                    rotate_left(13).wrapping_mul(PRIME1);
                pos += 16;
                if pos > limit {
                    break;
                }
            }
            self.v1 = v1;
            self.v2 = v2;
            self.v3 = v3;
            self.v4 = v4;
        }
        let mut h32: u32;
        if self.total & 0xf != 0 {
            return Err(Error::new(ErrorKind::Other, "Wrong call sequence"));
        }
        self.total += len;
        if self.total >= 16 {
            h32 = self.v1.rotate_left(1).
                wrapping_add(self.v2.rotate_left(7)).
                wrapping_add(self.v3.rotate_left(12)).
                wrapping_add(self.v4.rotate_left(18));
        } else {
            h32 = self.v3.wrapping_add(PRIME5);
        }
        h32 = h32.wrapping_add(self.total as u32);
        while pos + 4 <= end {
            h32 = read_vec_u32_le(buf, pos).
                wrapping_mul(PRIME3).wrapping_add(h32).
                rotate_left(17).wrapping_mul(PRIME4);
            pos += 4;
        }
        while pos < end {
            h32 = (buf[pos] as u32).
                wrapping_mul(PRIME5).wrapping_add(h32).
                rotate_left(11).wrapping_mul(PRIME1);
            pos += 1;
        }
        h32 = (h32 ^ (h32 >> 15)).wrapping_mul(PRIME2);
        h32 = (h32 ^ (h32 >> 13)).wrapping_mul(PRIME3);
        return Ok(h32 ^ (h32 >> 16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_world() {
        let mut hash = XXHash32::new(0);
        let result = hash.update(&"Hello world".as_bytes().to_vec(), 0, 11);
        assert_eq!(0x9705d437, result.unwrap());
    }

    #[test]
    fn read_vec_u32_le_test() {
        let mut x: Vec<u8> = Vec::new();
        x.resize(4, 0);
        x[0] = 0x12;
        x[1] = 0x34;
        x[2] = 0x56;
        x[3] = 0x78;
        assert_eq!(0x78563412, read_vec_u32_le(&x, 0));
    }

    #[test]
    fn read_u32_le_test() {
        let mut x: [u8; 4] = [0; 4];
        x[0] = 0x12;
        x[1] = 0x34;
        x[2] = 0x56;
        x[3] = 0x78;
        assert_eq!(0x78563412, read_u32_le(&x, 0));
    }
}
