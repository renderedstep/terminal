//! Ruby's `Random`, reproduced draw for draw.
//!
//! Ruby 3.4 seeds MT19937 in `random.c` `rand_init`: the seed's magnitude is
//! packed into 32-bit words, least significant first. A single word goes to
//! `init_genrand`; several go to `init_by_array`, after a top word that is
//! exactly 1 is dropped. Bounded draws follow `limited_rand`: mask to the next
//! power of two and reject anything over the limit, drawing nothing at all for
//! a limit of 0. The engine seeds every roll this way, so a port that used any
//! other generator would roll different dice.

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// A Mersenne Twister seeded and drawn from exactly as Ruby's `Random` is.
#[derive(Clone)]
pub struct Random {
    state: [u32; N],
    index: usize,
}

impl Random {
    /// `Random.new(seed)`. The sign is discarded, as Ruby discards it.
    pub fn new(seed: i128) -> Self {
        let mut magnitude = seed.unsigned_abs();
        let mut words = Vec::new();
        while magnitude > 0 {
            words.push(magnitude as u32);
            magnitude >>= 32;
        }
        match words.len() {
            0 => Self::from_word(0),
            1 => Self::from_word(words[0]),
            _ => {
                if words.last() == Some(&1) {
                    words.pop();
                }
                Self::from_words(&words)
            }
        }
    }

    /// The reference `init_genrand`.
    fn from_word(seed: u32) -> Self {
        let mut state = [0u32; N];
        state[0] = seed;
        for i in 1..N {
            let previous = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Random { state, index: N }
    }

    /// The reference `init_by_array`.
    fn from_words(key: &[u32]) -> Self {
        let mut random = Self::from_word(19_650_218);
        let state = &mut random.state;
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let previous = state[i - 1];
            state[i] = (state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                state[0] = state[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let previous = state[i - 1];
            state[i] = (state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                state[0] = state[N - 1];
                i = 1;
            }
        }
        state[0] = UPPER_MASK;
        random
    }

    fn twist(&mut self) {
        for i in 0..N {
            let y = (self.state[i] & UPPER_MASK) | (self.state[(i + 1) % N] & LOWER_MASK);
            let mut next = self.state[(i + M) % N] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= MATRIX_A;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    /// The next raw 32-bit output, tempered.
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `limited_rand`: a value in `0..=limit`. A limit of 0 draws nothing.
    pub fn limited(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            return 0;
        }
        let mut mask = limit;
        for shift in [1, 2, 4, 8, 16, 32] {
            mask |= mask >> shift;
        }
        'retry: loop {
            let mut value = 0u64;
            // Words are drawn high first, and only where the mask has bits.
            for word in (0..2).rev() {
                if (mask >> (word * 32)) & 0xffff_ffff == 0 {
                    continue;
                }
                value |= u64::from(self.next_u32()) << (word * 32);
                value &= mask;
                if value > limit {
                    continue 'retry;
                }
            }
            return value;
        }
    }

    /// `rand(min..max)`, both ends included.
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        min + self.limited((max - min) as u64) as i64
    }

    /// `rand(n)` for a positive integer: a value in `0..n`.
    pub fn below(&mut self, n: u64) -> u64 {
        self.limited(n - 1)
    }

    /// `Array#shuffle!(random:)`, in place.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        let mut i = items.len();
        while i > 1 {
            let j = self.limited((i - 1) as u64) as usize;
            i -= 1;
            items.swap(i, j);
        }
    }
}
