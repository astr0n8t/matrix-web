use sha2::{Sha256, Digest};
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() != 2 {
        eprintln!("Usage: {} <value-to-hash>", args[0]);
        eprintln!("\nGenerates a SHA-256 hash of the provided value for use in config.yaml");
        eprintln!("\nExample:");
        eprintln!("  {} my-secret-token", args[0]);
        std::process::exit(1);
    }
    
    let value = &args[1];
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    let hash = hex::encode(result);
    
    println!("Input value: {}", value);
    println!("SHA-256 hash: {}", hash);
    println!("\nUse this hash in your config.yaml:");
    println!("web:");
    println!("  auth:");
    println!("    header_name: \"X-Auth-Token\"");
    println!("    header_value_hash: \"{}\"", hash);
}
