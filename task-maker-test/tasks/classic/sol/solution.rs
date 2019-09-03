use std::fs::File;
use std::io::{Read, Write};

fn main() {
    let mut input = File::open("input.txt").unwrap();
    let mut output = File::create("output.txt").unwrap();
    let mut buffer = String::new();
    input.read_to_string(&mut buffer).unwrap();
    output.write_all(buffer.as_bytes()).unwrap();
}
