//! The 2A03's world: memory on the CPU bus, serviced exactly the way
//! the reference simulator's macros.js does it (halfStep: spin the
//! master clock until clk0 flips, then handleBusRead on the falling
//! half, handleBusWrite on the rising; reads drive the data bus by
//! flipping its pulls and settling once with all eight as seeds;
//! unwritten memory reads zero). The A3 golden generator scripts the
//! same protocol in JS, so the node-level comparison covers the
//! harness as well as the chip.

use halfphi::NodeId;

use crate::Cpu;

pub struct Harness {
    pub cpu: Cpu,
    ab: [NodeId; 16],
    db: [NodeId; 8],
    rw: NodeId,
    /// 64 KiB, zero where nothing was loaded or written, like mRead.
    pub memory: Vec<u8>,
    pub half_steps: u64,
    /// Test-only (the A3 mutation): XOR this mask into the byte served
    /// for this address, so the chip plays a note the authored program
    /// did not write. The golden replay and the period measurement must
    /// both go red. Setting it anywhere but a mutation proof is a bug
    /// by name.
    pub mutate_read: Option<(u16, u8)>,
}

impl Harness {
    pub fn new(cpu: Cpu) -> Harness {
        let nl = cpu.engine.netlist().clone();
        let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
        let arr = |prefix: &str, i: usize| n(&format!("{prefix}{i}"));
        Harness {
            ab: std::array::from_fn(|i| arr("ab", i)),
            db: std::array::from_fn(|i| arr("db", i)),
            rw: n("rw"),
            memory: vec![0u8; 65536],
            half_steps: 0,
            mutate_read: None,
            cpu,
        }
    }

    /// Load bytes and point the reset vector at them. Call before the
    /// first half step so the post-reset fetch finds the program.
    pub fn load(&mut self, addr: u16, bytes: &[u8], reset_vector: u16) {
        self.memory[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
        self.memory[0xfffc] = (reset_vector & 0xff) as u8;
        self.memory[0xfffd] = (reset_vector >> 8) as u8;
    }

    /// writeBits: set every pull, then one settle with all the pins as
    /// seeds, exactly writeDataBus's recalcNodeList of all eight.
    fn write_bits(&mut self, nodes: &[NodeId], mut val: u32) {
        for &n in nodes {
            self.cpu.engine.set_pull(n, val & 1 != 0);
            val >>= 1;
        }
        self.cpu.engine.settle(nodes);
    }

    fn read_bits(&self, nodes: &[NodeId]) -> u32 {
        nodes
            .iter()
            .enumerate()
            .map(|(i, &n)| (self.cpu.engine.is_high(n) as u32) << i)
            .sum()
    }

    /// One CPU half step: master toggles until clk0 flips, then the bus
    /// reacts, the reference's halfStep order.
    pub fn half_step(&mut self) {
        assert!(self.half_step_bounded(1 << 16), "clk0 stopped: the divider did not flip in 65,536 master pulses");
    }

    /// `half_step` with a ceiling on the master pulses spent waiting for
    /// clk0 to flip; false if it never did (a divider held in reset would
    /// otherwise spin this loop forever, and a caller driving the reset
    /// pin mid-run wants the fact, not a hang).
    pub fn half_step_bounded(&mut self, max_master_pulses: u32) -> bool {
        let clk = self.cpu.engine.is_high(self.cpu.sig.clk0);
        let mut spent = 0u32;
        loop {
            self.cpu.engine.drive_high(self.cpu.sig.clk_in);
            self.cpu.engine.drive_low(self.cpu.sig.clk_in);
            if self.cpu.engine.is_high(self.cpu.sig.clk0) != clk {
                break;
            }
            spent += 1;
            if spent >= max_master_pulses {
                return false;
            }
        }
        if clk {
            self.bus_read();
        } else {
            self.bus_write();
        }
        self.half_steps += 1;
        true
    }

    fn bus_read(&mut self) {
        if self.cpu.engine.is_high(self.rw) {
            let a = self.read_bits(&self.ab) as u16;
            let mut d = self.memory[a as usize];
            if let Some((ma, mask)) = self.mutate_read {
                if a == ma {
                    d ^= mask;
                }
            }
            let db = self.db;
            self.write_bits(&db, d as u32);
        }
    }

    fn bus_write(&mut self) {
        if !self.cpu.engine.is_high(self.rw) {
            let a = self.read_bits(&self.ab) as u16;
            let d = self.read_bits(&self.db) as u8;
            self.memory[a as usize] = d;
        }
    }
}
