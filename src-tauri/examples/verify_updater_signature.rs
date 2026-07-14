use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let config_path = PathBuf::from(args.next().ok_or("missing tauri config path")?);
    let signature_path = PathBuf::from(args.next().ok_or("missing signature path")?);
    let artifact_path = PathBuf::from(args.next().ok_or("missing artifact path")?);
    if args.next().is_some() {
        return Err(
            "usage: verify_updater_signature <tauri.conf.json> <file.sig> <artifact>".into(),
        );
    }

    let config: serde_json::Value = serde_json::from_reader(File::open(config_path)?)?;
    let encoded_key = config
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .ok_or("tauri updater pubkey missing")?;
    let decoded_key = base64::engine::general_purpose::STANDARD.decode(encoded_key)?;
    let decoded_key = String::from_utf8(decoded_key)?;
    let public_key = PublicKey::decode(&decoded_key)?;
    let signature_text = std::fs::read_to_string(signature_path)?;
    let signature = match Signature::decode(signature_text.trim()) {
        Ok(signature) => signature,
        Err(_) => {
            let decoded =
                base64::engine::general_purpose::STANDARD.decode(signature_text.trim())?;
            Signature::decode(&String::from_utf8(decoded)?)?
        }
    };

    let mut verifier = public_key.verify_stream(&signature)?;
    let mut artifact = File::open(&artifact_path)?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = artifact.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier.finalize()?;
    println!("updater minisign OK: {}", artifact_path.display());
    Ok(())
}
