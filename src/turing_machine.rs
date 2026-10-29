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
    pub fn shift<const RIGHT: bool>(&mut self) -> bool {
        if RIGHT {
            self.mask <<= 1;
        } else {
            self.mask >>= 1;
        }

        self.mask == 0
    }
}

pub struct Tape {
    slot_idx: usize,
    tape_side: bool,
    left_tape: Vec<u64>,
    right_tape: Vec<u64>,
}

impl Tape {
    pub fn new() -> Self {
        Self {
            slot_idx: 0,
            tape_side: true,
            left_tape: vec![0],
            right_tape: vec![0],
        }
    }

    #[inline(always)]
    pub fn cross<const RIGHT: bool>(&mut self, h: &mut Head) {
        let outward = self.tape_side == RIGHT;

        let tape = if self.tape_side {
            &mut self.right_tape
        } else {
            &mut self.left_tape
        };

        // inward from index 0 means switching sides
        if !outward && self.slot_idx == 0 {
            *h = self.side_cross::<RIGHT>(*h);
            return;
        }

        unsafe { *tape.get_unchecked_mut(self.slot_idx) = h.slot_value };

        // For some reason replacing next with self.slot_idx tanks performance
        let next = if outward {
            let next = self.slot_idx + 1;

            // Can only grow when moving outwards
            if next == tape.len() {
                tape.push(0);
            }

            next
        } else {
            self.slot_idx - 1
        };

        if outward {
            self.slot_idx += 1;
        } else {
            self.slot_idx -= 1;
        }

        h.slot_value = unsafe { *tape.get_unchecked(next) };
        h.mask = if RIGHT { 1 } else { 1 << 63 };
    }

    #[inline(always)]
    fn side_cross<const RIGHT: bool>(&mut self, mut h: Head) -> Head {
        self.tape_side = !self.tape_side;

        if RIGHT {
            unsafe { *self.left_tape.get_unchecked_mut(0) = h.slot_value };
            h.slot_value = unsafe { *self.right_tape.get_unchecked(0) };
            h.mask = 1;
        } else {
            unsafe { *self.right_tape.get_unchecked_mut(0) = h.slot_value };
            h.slot_value = unsafe { *self.left_tape.get_unchecked(0) };
            h.mask = 1 << 63;
        }
        h
    }
}
