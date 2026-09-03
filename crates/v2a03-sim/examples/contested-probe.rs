fn main() {
    let cpu = v2a03_sim::Cpu::power_on();
    println!("contested_groups after power_on: {}", cpu.engine.stats().contested_groups);
    let mut cpu = cpu;
    for _ in 0..600 { cpu.half_step(); }
    println!("contested_groups after 600 half-steps: {}", cpu.engine.stats().contested_groups);
}
