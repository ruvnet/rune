fn main() {
    let sum: u64 = (1..=100u64).sum();
    println!(
        "rune-portable arch={} os={} sum={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        sum
    );
}
