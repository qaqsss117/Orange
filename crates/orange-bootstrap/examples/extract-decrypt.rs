use std::{env, fs};

use orange_bootstrap::{BootstrapKey, BootstrapManifest, decrypt, parse_key_hex};

fn main() {
    let args: Vec<String> = env::args().collect();
    // usage: extract-decrypt <exe> <byte-offset-of-ORNGBTP1> <manifest.json> <key-hex>
    let exe = fs::read(&args[1]).expect("read exe");
    let offset: usize = args[2].parse().expect("offset");
    let manifest: BootstrapManifest =
        serde_json::from_slice(&fs::read(&args[3]).expect("read manifest")).expect("parse manifest");
    let key: BootstrapKey = parse_key_hex(&args[4]).expect("parse key");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut found = false;
    for end in (offset + 64..=exe.len().min(offset + 128 * 1024)).step_by(1) {
        let candidate = &exe[offset..end];
        if let Ok(mut secret) = decrypt(candidate, &manifest, &key, now) {
            secret.consume_in_place(|config| {
                println!("envelope length: {}", end - offset);
                println!("{}", serde_json::to_string_pretty(config).expect("serialize"));
            });
            secret.clear();
            found = true;
            break;
        }
    }
    if !found {
        println!("DECRYPT FAILED for all candidate lengths (wrong key or corrupted envelope)");
    }
}
