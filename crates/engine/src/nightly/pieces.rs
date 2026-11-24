//! Cutting a file into pieces where its content says, so a change in one place changes one or two
//! pieces and the rest are found again the next night. A gear hash over the bytes cuts where its
//! low bits are zero; the gear table comes from the backup's key, so where a large known file is
//! cut says nothing of it. A file smaller than [`MIN_PIECE`] is one piece, and the last piece of
//! a file ends with it, so the account sees their sizes.

use std::io::Read;

use sha2::{Digest, Sha256};

/// No piece is smaller than this, except a file's last.
pub(crate) const MIN_PIECE: usize = 256 * 1024;
/// No piece is larger than this.
pub(crate) const MAX_PIECE: usize = 4 * 1024 * 1024;
/// About one cut in this many bytes past the smallest size.
const MASK: u64 = (1 << 20) - 1;

pub(crate) struct Cutter {
    gear: [u64; 256],
}

impl Cutter {
    pub(crate) fn new(seed: &[u8; 32]) -> Self {
        let mut gear = [0u64; 256];
        for (i, slot) in gear.iter_mut().enumerate() {
            let d = Sha256::new()
                .chain_update(seed)
                .chain_update((i as u32).to_le_bytes())
                .finalize();
            let mut b = [0u8; 8];
            b.copy_from_slice(&d[..8]);
            *slot = u64::from_le_bytes(b);
        }
        Cutter { gear }
    }

    /// Where the first piece of `buf` ends. `at_end` says no more bytes follow `buf`.
    fn cut(&self, buf: &[u8], at_end: bool) -> Option<usize> {
        if buf.len() <= MIN_PIECE {
            return at_end.then_some(buf.len());
        }
        let mut h: u64 = 0;
        let end = buf.len().min(MAX_PIECE);
        for (i, &b) in buf.iter().enumerate().take(end).skip(MIN_PIECE) {
            h = (h << 1).wrapping_add(self.gear[b as usize]);
            if h & MASK == 0 {
                return Some(i + 1);
            }
        }
        if end == MAX_PIECE || at_end {
            Some(end)
        } else {
            None
        }
    }

    /// Read `src` to its end, handing each piece to `each` in order.
    pub(crate) fn pieces<E>(
        &self,
        src: &mut impl Read,
        mut each: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E>
    where
        E: From<std::io::Error>,
    {
        let mut buf: Vec<u8> = Vec::with_capacity(MAX_PIECE);
        let mut at_end = false;
        loop {
            while !at_end && buf.len() < MAX_PIECE {
                let start = buf.len();
                buf.resize(MAX_PIECE, 0);
                let n = src.read(&mut buf[start..])?;
                buf.truncate(start + n);
                if n == 0 {
                    at_end = true;
                }
            }
            if buf.is_empty() {
                return Ok(());
            }
            let Some(n) = self.cut(&buf, at_end) else {
                continue;
            };
            each(&buf[..n])?;
            buf.drain(..n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut(seed: u8, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        Cutter::new(&[seed; 32])
            .pieces::<std::io::Error>(&mut &bytes[..], |p| {
                out.push(p.to_vec());
                Ok(())
            })
            .unwrap();
        out
    }

    fn noise(len: usize, mut x: u64) -> Vec<u8> {
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    #[test]
    fn the_pieces_put_back_together_are_the_file() {
        for len in [0, 1, MIN_PIECE, MIN_PIECE + 1, 9 * 1024 * 1024 + 7] {
            let bytes = noise(len, 7);
            let pieces = cut(1, &bytes);
            assert_eq!(pieces.concat(), bytes);
            assert!(pieces.iter().all(|p| p.len() <= MAX_PIECE));
            let n = pieces.len();
            assert!(
                pieces
                    .iter()
                    .take(n.saturating_sub(1))
                    .all(|p| p.len() >= MIN_PIECE)
            );
        }
    }

    #[test]
    fn a_change_in_one_place_leaves_most_pieces_as_they_were() {
        let bytes = noise(16 * 1024 * 1024, 11);
        let before = cut(2, &bytes);
        let mut changed = bytes.clone();
        changed.splice(8_000_000..8_000_000, b"a few new bytes".iter().copied());
        let after = cut(2, &changed);
        let same = after.iter().filter(|p| before.contains(p)).count();
        assert!(
            same + 3 >= after.len(),
            "{same} of {} pieces found again",
            after.len()
        );
    }

    #[test]
    fn another_key_cuts_in_other_places() {
        let bytes = noise(8 * 1024 * 1024, 13);
        assert_ne!(
            cut(3, &bytes).iter().map(Vec::len).collect::<Vec<_>>(),
            cut(4, &bytes).iter().map(Vec::len).collect::<Vec<_>>()
        );
    }
}
