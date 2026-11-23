//! Managed guest heap used by `GALLOC`.
//!
//! This is a non-moving, conservative mark-and-sweep allocator. All of its
//! bookkeeping lives in guest RAM, so it survives save states exactly like the
//! original SiliconCraft implementation.
//!
//! Layout at `base`: `[top, free_list_head, allocation_budget]`, followed by
//! blocks. Every block starts with `[size, next_free]`. A positive size marks an
//! allocated block, a negative size marks a free block.

const CONTROL_WORDS: i64 = 3;
const HEADER_WORDS: i64 = 2;
const MIN_BUDGET: i64 = 2048;

fn oob(message: &str) -> String {
    message.to_string()
}

/// Reset the heap at `base` so that it is empty.
pub fn initialize(ram: &mut [i32], base: i32) -> Result<(), String> {
    let base = i64::from(base);
    if base < 0 || base > ram.len() as i64 - CONTROL_WORDS {
        return Err(oob("N2 heap does not fit in RAM"));
    }
    let b = base as usize;
    ram[b] = (base + CONTROL_WORDS) as i32;
    ram[b + 1] = 0;
    ram[b + 2] = MIN_BUDGET as i32;
    Ok(())
}

/// Allocate `words` zeroed cells. `roots` lists every register, data stack
/// value and saved interrupt register that might reference heap memory.
pub fn allocate(ram: &mut [i32], base: i32, words: i32, roots: &[i32]) -> Result<i32, String> {
    let len = ram.len() as i64;
    let (base, words) = (i64::from(base), i64::from(words));
    if words < 1
        || base < 0
        || base > len - CONTROL_WORDS
        || words > len - base - CONTROL_WORDS - HEADER_WORDS
    {
        return Err(oob("N2 heap allocation exceeds installed RAM"));
    }
    let start = base + CONTROL_WORDS;
    let top = i64::from(ram[base as usize]);
    if top < start || top > len {
        return Err(oob("N2 heap bounds are invalid"));
    }
    let size = words + HEADER_WORDS;
    let collected = i64::from(ram[base as usize + 2]) < size;
    if collected {
        collect(ram, base, roots)?;
    }
    let mut block = reserve(ram, base, size)?;
    if block == 0 && !collected {
        collect(ram, base, roots)?;
        block = reserve(ram, base, size)?;
    }
    if block == 0 {
        return Err(oob("N2 heap is out of memory"));
    }
    let block_size = ram[block as usize];
    ram[base as usize + 2] -= block_size;
    let first = (block + HEADER_WORDS) as usize;
    ram[first..block as usize + block_size as usize].fill(0);
    Ok(first as i32)
}

fn block_size(ram: &[i32], block: i64, top: i64) -> Result<i64, String> {
    let size = i64::from(ram[block as usize]);
    let length = size.abs();
    if length <= HEADER_WORDS || length > top - block {
        return Err(oob("Invalid N2 heap block size"));
    }
    Ok(size)
}

fn reserve(ram: &mut [i32], base: i64, size: i64) -> Result<i64, String> {
    let top = i64::from(ram[base as usize]);
    let mut link = base + 1;
    loop {
        let block = i64::from(ram[link as usize]);
        if block == 0 {
            break;
        }
        if block < base + CONTROL_WORDS || block >= top || ram[block as usize] >= 0 {
            return Err(oob("Invalid N2 free block"));
        }
        let available = -block_size(ram, block, top)?;
        let next = i64::from(ram[block as usize + 1]);
        if next != 0 && next < block + available {
            return Err(oob("Invalid N2 free list"));
        }
        if available >= size {
            if available - size > HEADER_WORDS {
                let remainder = block + size;
                ram[remainder as usize] = (size - available) as i32;
                ram[remainder as usize + 1] = next as i32;
                ram[link as usize] = remainder as i32;
                ram[block as usize] = size as i32;
            } else {
                ram[link as usize] = next as i32;
                ram[block as usize] = available as i32;
            }
            return Ok(block);
        }
        link = block + 1;
    }
    if size > ram.len() as i64 - top {
        return Ok(0);
    }
    ram[top as usize] = size as i32;
    ram[base as usize] = (top + size) as i32;
    Ok(top)
}

struct Marker<'a> {
    ram: &'a [i32],
    start: i64,
    top: i64,
    blocks: Vec<i64>,
    live: Vec<bool>,
    pending: Vec<i64>,
}

impl Marker<'_> {
    fn reference(&mut self, value: i32) {
        let value = i64::from(value);
        if value < self.start + HEADER_WORDS || value >= self.top {
            return;
        }
        let index = self.blocks.partition_point(|&block| block <= value - HEADER_WORDS);
        if index == 0 || self.live[index - 1] {
            return;
        }
        let block = self.blocks[index - 1];
        if value >= block + i64::from(self.ram[block as usize]) {
            return;
        }
        self.live[index - 1] = true;
        self.pending.push(block);
    }

    fn scan(&mut self, from: i64, to: i64) {
        for index in from..to {
            self.reference(self.ram[index as usize]);
        }
    }
}

fn collect(ram: &mut [i32], base: i64, roots: &[i32]) -> Result<(), String> {
    let start = base + CONTROL_WORDS;
    let top = i64::from(ram[base as usize]);
    let mut blocks = Vec::new();
    let mut block = start;
    while block < top {
        let size = block_size(ram, block, top)?;
        if size > 0 {
            blocks.push(block);
        }
        block += size.abs();
    }
    let live = vec![false; blocks.len()];
    let mut mark = Marker { ram: &*ram, start, top, blocks, live, pending: Vec::new() };
    mark.scan(0, base);
    for &root in roots {
        mark.reference(root);
    }
    while let Some(block) = mark.pending.pop() {
        let end = block + i64::from(mark.ram[block as usize]);
        mark.scan(block + HEADER_WORDS, end);
    }
    let live_words: i64 = mark
        .blocks
        .iter()
        .zip(&mark.live)
        .filter(|(_, &live)| live)
        .map(|(&block, _)| i64::from(mark.ram[block as usize]))
        .sum();
    let Marker { blocks, live, .. } = mark;
    sweep(ram, base, start, top, &blocks, &live, live_words);
    Ok(())
}

fn sweep(ram: &mut [i32], base: i64, start: i64, top: i64, blocks: &[i64], live: &[bool], live_words: i64) {
    let is_live = |block: i64| blocks.binary_search(&block).map_or(false, |index| live[index]);
    let mut free_link = base + 1;
    ram[free_link as usize] = 0;
    ram[base as usize + 2] = 0;
    let mut free_start = start;
    let mut block = start;
    while block < top {
        let size = i64::from(ram[block as usize]).abs();
        if is_live(block) {
            if free_start < block {
                ram[free_start as usize] = (free_start - block) as i32;
                ram[free_start as usize + 1] = 0;
                ram[free_link as usize] = free_start as i32;
                free_link = free_start + 1;
            }
            free_start = block + size;
        }
        block += size;
    }
    ram[base as usize] = free_start as i32;
    ram[base as usize + 2] = live_words.max(MIN_BUDGET) as i32;
}
