#[derive(Clone, Copy)]
pub struct Head {
    mask: u64,
    slot_value: u64,
}

impl Head {
    pub fn new() -> Self {
        Self {
            mask: 1,
            slot_value: 0,
        }
    }

    #[inline(always)]
    pub fn read(&self) -> bool {
        self.slot_value & self.mask != 0
    }

    #[inline(always)]
    pub fn write<const WRITE: bool>(&mut self) {
        if WRITE {
            self.slot_value |= self.mask
        } else {
            self.slot_value &= !self.mask
        }
    }

    #[inline(always)]
    pub fn flip(&mut self) {
        self.slot_value ^= self.mask;
    }

    #[inline(always)]
    pub fn shift<const RIGHT: bool>(&mut self, tape: &mut Tape) {
        if RIGHT {
            self.mask <<= 1;
        } else {
            self.mask >>= 1;
        }

        if self.mask == 0 {
            tape.cross::<RIGHT>(self)
        }
    }

    // #[inline(always)]
    // pub fn shift_rule<const RIGHT: bool, const SYMBOL: bool, const WRITE: bool>(
    //     &mut self,
    //     tape: &mut Tape,
    //     mut budget: u64,
    // ) -> u64 {
    //     let start = budget;
    //     let full: u64 = if SYMBOL { u64::MAX } else { 0 };
    //     let filled: u64 = if WRITE { u64::MAX } else { 0 };

    //     loop {
    //         let p = self.mask.trailing_zeros();
    //         let diff = if SYMBOL {
    //             !self.slot_value
    //         } else {
    //             self.slot_value
    //         };

    //         let (dist, to_edge) = if RIGHT {
    //             ((diff >> p).trailing_zeros(), 64 - p)
    //         } else {
    //             ((diff << (63 - p)).leading_zeros(), p + 1)
    //         };

    //         let n = (dist.min(to_edge)).min(budget as u32);
    //         if n == 0 {
    //             return start - budget;
    //         }
    //         budget -= n as u64;

    //         if SYMBOL != WRITE {
    //             let ones = u64::MAX >> (64 - n);
    //             let range = if RIGHT {
    //                 ones << p
    //             } else {
    //                 ones << (p + 1 - n)
    //             };

    //             if WRITE {
    //                 self.slot_value |= range
    //             } else {
    //                 self.slot_value &= !range
    //             }
    //         }

    //         if n == to_edge {
    //             tape.cross::<RIGHT>(self);

    //             // Tight loop: skip whole uniform words while the budget covers all 64 cells.
    //             while self.slot_value == full && budget >= 64 {
    //                 if SYMBOL != WRITE {
    //                     self.slot_value = filled;
    //                 }
    //                 tape.cross::<RIGHT>(self); // stores `filled`, loads the next word
    //                 budget -= 64;
    //             }
    //             // Fall through to the general scan: handles a mixed word, a word that
    //             // starts with a different symbol (n == 0 -> return), or budget < 64.
    //         } else {
    //             if RIGHT {
    //                 self.mask <<= n
    //             } else {
    //                 self.mask >>= n
    //             }
    //             return start - budget;
    //         }
    //     }
    // }

    #[inline(always)]
    pub fn shift_rule<const RIGHT: bool, const SYMBOL: bool, const WRITE: bool>(
        &mut self,
        tape: &mut Tape,
        mut budget: u64,
    ) -> u64 {
        let filled: u64 = if WRITE { u64::MAX } else { 0 };

        // Phase 1: finish the word we're in (arbitrary bit position).
        {
            let p = self.mask.trailing_zeros();
            let diff = if SYMBOL {
                !self.slot_value
            } else {
                self.slot_value
            };
            let (dist, to_edge) = if RIGHT {
                ((diff >> p).trailing_zeros(), 64 - p)
            } else {
                ((diff << (63 - p)).leading_zeros(), p + 1)
            };

            // Clamp in u64 first: `budget as u32` would truncate.
            let n = dist.min(to_edge).min(budget.min(64) as u32);
            if n == 0 {
                return budget;
            }
            budget -= n as u64;

            if SYMBOL != WRITE {
                let ones = u64::MAX >> (64 - n); // n in 1..=64
                let range = if RIGHT {
                    ones << p
                } else {
                    ones << (p + 1 - n)
                };
                if WRITE {
                    self.slot_value |= range
                } else {
                    self.slot_value &= !range
                }
            }

            if n < to_edge {
                if RIGHT {
                    self.mask <<= n
                } else {
                    self.mask >>= n
                }
                return budget;
            }
            tape.cross::<RIGHT>(self); // mask is now on the entering edge
        }

        // Phase 2: the head is on the entering edge of a fresh word, so all positions are constants.
        loop {
            let diff = if SYMBOL {
                !self.slot_value
            } else {
                self.slot_value
            };

            if diff == 0 && budget >= 64 {
                if SYMBOL != WRITE {
                    self.slot_value = filled;
                }
                tape.cross::<RIGHT>(self);
                budget -= 64;
                continue;
            }

            // The run ends inside this word. Either diff != 0 (dist < 64) or budget < 64,
            // so n < 64 and every shift below is in range.
            let dist = if RIGHT {
                diff.trailing_zeros()
            } else {
                diff.leading_zeros()
            };
            let n = dist.min(budget.min(64) as u32);
            budget -= n as u64;

            if SYMBOL != WRITE {
                let range = if RIGHT {
                    (1u64 << n) - 1
                } else {
                    !(u64::MAX >> n)
                };
                if WRITE {
                    self.slot_value |= range
                } else {
                    self.slot_value &= !range
                }
            }
            self.mask = if RIGHT { 1u64 << n } else { (1u64 << 63) >> n };
            return budget;
        }
    }
}

pub struct Tape {
    left_tape: Vec<u64>,
    right_tape: Vec<u64>,
}

impl Tape {
    pub fn new() -> Self {
        Self {
            left_tape: vec![0],
            right_tape: vec![0],
        }
    }

    #[inline(always)]
    pub fn cross<const RIGHT: bool>(&mut self, h: &mut Head) {
        if RIGHT {
            self.left_tape.push(h.slot_value);
            h.slot_value = self.right_tape.pop().unwrap_or(0);
            h.mask = 1;
        } else {
            self.right_tape.push(h.slot_value);
            h.slot_value = self.left_tape.pop().unwrap_or(0);
            h.mask = 1 << 63;
        }
    }
}
