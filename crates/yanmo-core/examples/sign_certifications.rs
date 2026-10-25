//! 签发工具：用离线私钥（32 字节 hex）对 entries JSON 签官方认证清单。
//! 用法：cargo run -p yanmo-core --example sign_certifications -- <secret-hex> <entries.json>
use std::env;
use std::fs;

use ed25519_dalek::{Signer, SigningKey};
use yanmo_core::store::{Certification, CertificationSet};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let secret = args.first().expect("secret hex");
    let entries_path = args.get(1).expect("entries json");
    let mut bytes = [0u8; 32];
    for (index, pair) in secret.as_bytes().chunks(2).enumerate() {
        bytes[index] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
    }
    let key = SigningKey::from_bytes(&bytes);
    let entries: Vec<Certification> =
        serde_json::from_str(&fs::read_to_string(entries_path).unwrap()).unwrap();
    let message = CertificationSet::canonical_message(&entries);
    let signature = key.sign(message.as_bytes());
    println!("SIGNATURE={}", signature.to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>());
}
