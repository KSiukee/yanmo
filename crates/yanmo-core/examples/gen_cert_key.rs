//! 一次性工具：生成官方认证密钥并签一份空清单。运行后人工保存私钥，**不要提交**。
use ed25519_dalek::{Signer, SigningKey};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn main() {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).unwrap();
    let key = SigningKey::from_bytes(&secret);
    let public = key.verifying_key().to_bytes();
    let signature = key.sign(b"");
    println!("SECRET={}", hex(&secret));
    println!("PUBLIC={}", hex(&public));
    println!("SIGNATURE={}", hex(&signature.to_bytes()));
}
