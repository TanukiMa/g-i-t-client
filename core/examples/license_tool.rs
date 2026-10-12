//! The operator's license tool (run after a Stripe payment). The private key never goes into the repository.
//!
//!   cargo run -p git_core --example license_tool -- keygen            prints  private=<hex>  public=<rust array>
//!   cargo run -p git_core --example license_tool -- sign <private-key-file> <subscriber> [days]
//!                                                                      prints the license key (days omitted = no expiry)

use git_core::entitlement::sign_license;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> [u8; 32] {
    let s = s.trim();
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("the key file holds 64 hex digits");
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("keygen") => {
            let mut secret = [0u8; 32];
            getrandom::getrandom(&mut secret).expect("random");
            let sk = ed25519_dalek::SigningKey::from_bytes(&secret);
            println!("private={}", hex(&secret));
            let p = sk.verifying_key().to_bytes();
            println!("public={}", p.iter().map(|x| format!("0x{x:02x}")).collect::<Vec<_>>().join(", "));
        }
        Some("sign") if a.len() >= 3 => {
            let secret = unhex(&std::fs::read_to_string(&a[1]).expect("key file"));
            let exp = a.get(3).and_then(|d| d.parse::<i64>().ok()).map(|d| chrono::Utc::now().timestamp_millis() + d * 86_400_000).unwrap_or(0);
            println!("{}", sign_license(&secret, &a[2], exp));
        }
        _ => eprintln!("usage: license_tool keygen | sign <private-key-file> <subscriber> [days]"),
    }
}
