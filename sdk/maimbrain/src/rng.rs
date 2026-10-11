/// Small deterministic PRNG (PCG32). Seed it from [`crate::sys::rand_seed`]
/// so replays reproduce the same sequence.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        let mut r = Rng { state: 0, inc: (seed << 1) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    /// Seeded from the session seed.
    pub fn from_host() -> Rng {
        Rng::new(crate::sys::rand_seed())
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform in `[0, 1)`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform in `0..n`, unbiased (an index, a lane, a die roll: `below(6) + 1`).
    /// Panics if `n` is 0.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "Rng::below(0): no value is below 0");
        // Lemire's multiply-shift, rejecting the few draws that would bias it.
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = self.next_u32() as u64 * n as u64;
            if m as u32 >= threshold {
                return (m >> 32) as u32;
            }
        }
    }

    /// One of `items`, uniformly; `None` if it's empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.below(items.len().min(u32::MAX as usize) as u32) as usize)
    }

    /// Shuffles `items` in place (Fisher–Yates): a deck, a bag of pieces.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len().min(u32::MAX as usize)).rev() {
            items.swap(i, self.below(i as u32 + 1) as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn below_covers_its_range_evenly_and_replays() {
        let mut r = Rng::new(7);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[r.below(6) as usize] += 1;
        }
        assert!(counts.iter().all(|&c| (9_400..10_600).contains(&c)), "{counts:?}");
        let (mut a, mut b) = (Rng::new(3), Rng::new(3));
        assert!((0..100).all(|_| a.below(1000) == b.below(1000)));
        assert_eq!(Rng::new(1).below(1), 0);
    }

    #[test]
    fn pick_and_shuffle() {
        let mut r = Rng::new(5);
        assert_eq!(r.pick::<u8>(&[]), None);
        assert!([1, 2, 3].contains(r.pick(&[1, 2, 3]).unwrap()));
        let mut deck: Vec<u32> = (0..52).collect();
        r.shuffle(&mut deck);
        assert_ne!(deck, (0..52).collect::<Vec<_>>());
        deck.sort();
        assert_eq!(deck, (0..52).collect::<Vec<_>>());
    }
}
