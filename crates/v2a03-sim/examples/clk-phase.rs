use v2a03_sim::Cpu;
fn main() {
    let mut cpu = Cpu::power_on();
    let mut prev = cpu.engine.is_high(cpu.sig.clk0);
    let mut edges = Vec::new();
    for m in 1..=120u32 {
        cpu.half_step();
        let c = cpu.engine.is_high(cpu.sig.clk0);
        if c != prev {
            edges.push((m, c));
            prev = c;
        }
    }
    println!("2A03 clk0 edges (master half-step, level) after power_on: {edges:?}");
    println!("clk_in level after power_on: {}", cpu.engine.is_high(cpu.sig.clk_in));
}
