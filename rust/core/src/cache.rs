//! The rendered grid saved to disk, so the first frame after a cold start is a file read and
//! a memory copy, with no PackageManager calls or font work.

const MAGIC: &[u8; 4] = b"MFG1";
const HEADER: usize = 4 + 8 + 4 + 4;

/// FNV-1a: stable across Rust releases, unlike `DefaultHasher`.
#[derive(Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn bytes(mut self, data: &[u8]) -> Self {
        for b in data {
            self.0 ^= *b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
        self
    }

    pub fn u64(self, v: u64) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

pub fn encode(key: u64, width: usize, height: usize, px: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + px.len() * 4);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&key.to_le_bytes());
    out.extend_from_slice(&(width as u32).to_le_bytes());
    out.extend_from_slice(&(height as u32).to_le_bytes());
    for p in px {
        out.extend_from_slice(&p.to_le_bytes());
    }
    out
}

/// Returns the stored key and pixels when the file is intact and has the given size.
pub fn decode(data: &[u8], width: usize, height: usize) -> Option<(u64, Vec<u32>)> {
    if data.len() != HEADER + width * height * 4 || &data[..4] != MAGIC {
        return None;
    }
    let key = u64::from_le_bytes(data[4..12].try_into().ok()?);
    let w = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    let h = u32::from_le_bytes(data[16..20].try_into().ok()?) as usize;
    if (w, h) != (width, height) {
        return None;
    }
    let px = data[HEADER..].as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect();
    Some((key, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_size_check() {
        let px = vec![1, 2, 3, 4, 5, 6];
        let data = encode(42, 3, 2, &px);
        assert_eq!(decode(&data, 3, 2), Some((42, px)));
        assert_eq!(decode(&data, 2, 3), None);
        assert_eq!(decode(&data[..data.len() - 1], 3, 2), None);
    }
}
