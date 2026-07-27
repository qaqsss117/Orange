#![forbid(unsafe_code)]

use std::{
    env, fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use orange_bootstrap::{BootstrapConfig, BuildMetadata, parse_key_hex, seal};
use zeroize::Zeroizing;

const KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX";
const MAX_PLAINTEXT_BYTES: u64 = 64 * 1024;

fn main() {
    if let Err(error) = run(env::args_os().skip(1)) {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run(arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), CliError> {
    let arguments = Arguments::parse(arguments)?;
    let mut plaintext = Zeroizing::new(String::new());
    io::stdin()
        .take(MAX_PLAINTEXT_BYTES + 1)
        .read_to_string(&mut plaintext)
        .map_err(|_| CliError::Input)?;
    if plaintext.len() as u64 > MAX_PLAINTEXT_BYTES {
        return Err(CliError::InputTooLarge);
    }

    let config: BootstrapConfig =
        serde_json::from_str(&plaintext).map_err(|_| CliError::InvalidPlaintext)?;
    let key_hex = Zeroizing::new(env::var(KEY_ENV).map_err(|_| CliError::MissingKey)?);
    let key = parse_key_hex(&key_hex).map_err(|_| CliError::InvalidKey)?;
    let metadata = BuildMetadata {
        channel: arguments.channel,
        product_version: arguments.product_version,
        key_id: arguments.key_id,
        generated_at_unix: current_unix_time()?,
    };
    let artifact = seal(&config, &metadata, &key).map_err(|_| CliError::Encryption)?;
    let mut manifest =
        serde_json::to_vec_pretty(&artifact.manifest).map_err(|_| CliError::Manifest)?;
    manifest.push(b'\n');

    write_output(&arguments.output, &artifact.envelope)?;
    write_output(&arguments.manifest, &manifest)?;
    println!("Bootstrap encryption completed.");
    Ok(())
}

fn current_unix_time() -> Result<u64, CliError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| CliError::Clock)
}

fn write_output(path: &Path, contents: &[u8]) -> Result<(), CliError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent).map_err(|_| CliError::Output)?;
    }
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(CliError::Output);
    }

    let temporary = temporary_path(path);
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|_| CliError::Output)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| CliError::Output)?;
    file.write_all(contents).map_err(|_| CliError::Output)?;
    file.sync_all().map_err(|_| CliError::Output)?;
    drop(file);

    if path.exists() {
        fs::remove_file(path).map_err(|_| CliError::Output)?;
    }
    fs::rename(&temporary, path).map_err(|_| CliError::Output)
}

fn temporary_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("output");
    path.with_extension(format!("{extension}.tmp-{}", process::id()))
}

#[derive(Debug)]
struct Arguments {
    output: PathBuf,
    manifest: PathBuf,
    channel: String,
    product_version: String,
    key_id: String,
}

impl Arguments {
    fn parse(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, CliError> {
        if arguments.next().as_deref() != Some(std::ffi::OsStr::new("encrypt")) {
            return Err(CliError::Usage);
        }

        let mut output = None;
        let mut manifest = None;
        let mut channel = None;
        let mut product_version = None;
        let mut key_id = None;

        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or(CliError::Usage)?;
            match flag.to_str() {
                Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
                Some("--manifest") if manifest.is_none() => manifest = Some(PathBuf::from(value)),
                Some("--channel") if channel.is_none() => {
                    channel = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                Some("--product-version") if product_version.is_none() => {
                    product_version = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                Some("--key-id") if key_id.is_none() => {
                    key_id = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                _ => return Err(CliError::Usage),
            }
        }

        let parsed = Self {
            output: output.ok_or(CliError::Usage)?,
            manifest: manifest.ok_or(CliError::Usage)?,
            channel: channel.ok_or(CliError::Usage)?,
            product_version: product_version.ok_or(CliError::Usage)?,
            key_id: key_id.ok_or(CliError::Usage)?,
        };
        if parsed.output == parsed.manifest {
            return Err(CliError::Usage);
        }

        Ok(parsed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliError {
    Usage,
    Input,
    InputTooLarge,
    InvalidPlaintext,
    MissingKey,
    InvalidKey,
    Clock,
    Encryption,
    Manifest,
    Output,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Usage => {
                "usage: orange-bootstrap-crypto encrypt --output FILE --manifest FILE --channel NAME --product-version VERSION --key-id ID"
            }
            Self::Input => "cannot read bootstrap plaintext from stdin",
            Self::InputTooLarge => "bootstrap plaintext exceeds the size limit",
            Self::InvalidPlaintext => "bootstrap plaintext is invalid",
            Self::MissingKey => "bootstrap build key environment variable is missing",
            Self::InvalidKey => "bootstrap build key must be 32 bytes encoded as hexadecimal",
            Self::Clock => "system clock is unavailable",
            Self::Encryption => "bootstrap encryption failed",
            Self::Manifest => "bootstrap manifest generation failed",
            Self::Output => "bootstrap output could not be written",
        })
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_require_every_named_value_and_distinct_outputs() {
        let valid = [
            "encrypt",
            "--output",
            "bootstrap.enc",
            "--manifest",
            "bootstrap.manifest.json",
            "--channel",
            "development",
            "--product-version",
            "0.1.0",
            "--key-id",
            "dev-2026-01",
        ];
        let parsed = Arguments::parse(valid.into_iter().map(std::ffi::OsString::from)).unwrap();
        assert_eq!(parsed.channel, "development");

        let same_outputs = [
            "encrypt",
            "--output",
            "bootstrap.enc",
            "--manifest",
            "bootstrap.enc",
            "--channel",
            "development",
            "--product-version",
            "0.1.0",
            "--key-id",
            "dev-2026-01",
        ];
        assert_eq!(
            Arguments::parse(same_outputs.into_iter().map(std::ffi::OsString::from)).unwrap_err(),
            CliError::Usage
        );
    }
}
