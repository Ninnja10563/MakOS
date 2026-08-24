fn main() {
    let worker = std::thread::spawn(|| 40 + 2);
    assert_eq!(worker.join().unwrap(), 42);
    println!("MAKOS_RUST_STD_OK target=aarch64-unknown-makos std=1 threads=1");
}
