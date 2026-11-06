use std::alloc::{self, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};

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
    ptr: NonNull<u64>,
    len: usize,
    idx: usize,
}

impl Tape {
    pub fn new() -> Self {
        let len = 128;
        let layout = Layout::array::<u64>(len).expect("Allocation too large");
        let new_ptr = unsafe { alloc::alloc_zeroed(layout) };

        let ptr = match NonNull::new(new_ptr as *mut u64) {
            Some(p) => p,
            None => alloc::handle_alloc_error(layout),
        };

        Self { ptr, len, idx: 64 }
    }

    #[inline(always)]
    pub fn cross<const RIGHT: bool>(&mut self, h: &mut Head) {
        let idx = self.idx;

        unsafe { *self.get_unchecked_mut(idx) = h.slot_value };

        if RIGHT {
            let next = idx + 1;

            if next >= self.len {
                self.grow_forward();
            }

            self.idx = next;
        } else {
            if idx == 0 {
                self.grow_backward();
            }
            self.idx -= 1;
        }

        h.slot_value = unsafe { *self.get_unchecked(self.idx) };
        h.mask = if RIGHT { 1 } else { 1 << 63 };
    }

    #[inline(always)]
    pub fn flush(&mut self, h: &Head) {
        let idx = self.idx;
        unsafe { *self.get_unchecked_mut(idx) = h.slot_value };
    }

    #[cold]
    #[inline(never)]
    fn grow_forward(&mut self) {
        let new_len = self.len * 2;

        let new_layout = Layout::array::<u64>(new_len).expect("Allocation too large");
        let old_layout = Layout::array::<u64>(self.len).unwrap();

        let old_ptr = self.ptr.as_ptr() as *mut u8;
        let new_ptr = unsafe { alloc::realloc(old_ptr, old_layout, new_layout.size()) };

        self.ptr = match NonNull::new(new_ptr as *mut u64) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };

        unsafe {
            ptr::write_bytes(self.ptr.as_ptr().add(self.len), 0, self.len);
        }

        self.len = new_len;
    }

    #[cold]
    #[inline(never)]
    fn grow_backward(&mut self) {
        debug_assert!(self.idx == 0, "self.idx must be 0 before growing");

        let old_len = self.len;
        let new_len = old_len * 2;

        let new_layout = Layout::array::<u64>(new_len).unwrap();
        let old_layout = Layout::array::<u64>(old_len).unwrap();

        let old_ptr = self.ptr.as_ptr() as *mut u8;
        let new_ptr = unsafe { alloc::realloc(old_ptr, old_layout, new_layout.size()) };

        let new_ptr = match NonNull::new(new_ptr as *mut u64) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };

        unsafe {
            ptr::copy_nonoverlapping(new_ptr.as_ptr(), new_ptr.as_ptr().add(old_len), old_len);
            ptr::write_bytes(new_ptr.as_ptr(), 0, old_len);
        }

        self.ptr = new_ptr;
        self.len = new_len;
        self.idx = old_len;
    }
}

impl Deref for Tape {
    type Target = [u64];
    fn deref(&self) -> &[u64] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl DerefMut for Tape {
    fn deref_mut(&mut self) -> &mut [u64] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for Tape {
    fn drop(&mut self) {
        let layout = Layout::array::<u64>(self.len).unwrap();
        unsafe {
            alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}
