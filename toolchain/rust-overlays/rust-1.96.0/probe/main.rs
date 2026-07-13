use std::thread;

fn main() {
    let worker = thread::spawn(|| 42usize);
    let value = worker.join().expect("Ristux std thread join failed");
    println!("hello from Ristux std thread {value}");
}
