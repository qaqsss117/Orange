#![forbid(unsafe_code)]

use std::{
    env, fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use orange_bootstrap::{
    AndroidUpdateManifest, BootstrapConfig, BuildMetadata, RemoteBootstrapManifest, SigningKey,
    TxtLocatorDocument, VerifyingKey, parse_key_hex, seal, sign_android_update_manifest,
    sign_remote_manifest, sign_txt_locator, validate_verifying_key_set,
};
use zeroize::Zeroizing;

const KEY_ENV: &str = "ORANGE_BOOTSTRAP_BUILD_KEY_HEX";
const SIGNING_KEY_ENV: &str = "ORANGE_BOOTSTRAP_SIGNING_KEY_HEX";
const VERIFYING_KEYS_ENV: &str = "ORANGE_BOOTSTRAP_SIGNING_PUBLIC_KEYS";
const MAX_PLAINTEXT_BYTES: u64 = 64 * 1024;

fn main() {
    if let Err(error) = run(env::args_os().skip(1)) {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run(arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<(), CliError> {
    let mut arguments = arguments;
    match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .as_deref()
    {
        Some("encrypt") => encrypt(EncryptArguments::parse(arguments)?),
        Some("sign-locator") => sign_locator(LocatorArguments::parse(arguments)?),
        Some("sign-android-update") => sign_android_update(FileArguments::parse(arguments)?),
        _ => Err(CliError::Usage),
    }
}

fn sign_android_update(arguments: FileArguments) -> Result<(), CliError> {
    let input = fs::read(&arguments.input).map_err(|_| CliError::Input)?;
    if input.is_empty() || input.len() as u64 > MAX_PLAINTEXT_BYTES {
        return Err(CliError::InputTooLarge);
    }
    let mut manifest: AndroidUpdateManifest =
        serde_json::from_slice(&input).map_err(|_| CliError::InvalidPlaintext)?;
    let signing_key = release_signing_key(&manifest.signature_key_id)?;
    sign_android_update_manifest(&mut manifest, &signing_key).map_err(|_| CliError::Signature)?;
    let mut output = serde_json::to_vec_pretty(&manifest).map_err(|_| CliError::Manifest)?;
    output.push(b'\n');
    write_output(&arguments.output, &output)
}

fn encrypt(arguments: EncryptArguments) -> Result<(), CliError> {
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
    let ciphertext_bytes =
        u32::try_from(artifact.envelope.len()).map_err(|_| CliError::Manifest)?;
    let signing_key = release_signing_key(&arguments.signing_key_id)?;
    let mut remote_manifest = RemoteBootstrapManifest::unsigned(
        artifact.manifest.clone(),
        arguments.envelope_url,
        ciphertext_bytes,
        arguments.minimum_client_version,
        metadata.generated_at_unix,
        arguments.signing_key_id,
    );
    sign_remote_manifest(&mut remote_manifest, &signing_key).map_err(|_| CliError::Signature)?;
    let mut manifest =
        serde_json::to_vec_pretty(&artifact.manifest).map_err(|_| CliError::Manifest)?;
    manifest.push(b'\n');
    let mut remote_manifest =
        serde_json::to_vec_pretty(&remote_manifest).map_err(|_| CliError::Manifest)?;
    remote_manifest.push(b'\n');

    write_output(&arguments.output, &artifact.envelope)?;
    write_output(&arguments.manifest, &manifest)?;
    write_output(&arguments.remote_manifest, &remote_manifest)?;
    println!("Bootstrap encryption completed.");
    Ok(())
}

fn sign_locator(arguments: LocatorArguments) -> Result<(), CliError> {
    let signing_key = release_signing_key(&arguments.signing_key_id)?;
    let generated_at_unix = current_unix_time()?;
    let mut locator = TxtLocatorDocument::unsigned(
        arguments.sequence,
        generated_at_unix,
        arguments.expires_at_unix,
        arguments.manifest_urls,
        arguments.signing_key_id,
    );
    sign_txt_locator(&mut locator, &signing_key).map_err(|_| CliError::Signature)?;
    let json = serde_json::to_vec(&locator).map_err(|_| CliError::Manifest)?;
    let record = format!("orange-bootstrap-v1:{}\n", URL_SAFE_NO_PAD.encode(json));
    write_output(&arguments.output, record.as_bytes())?;
    println!("Bootstrap TXT locator signed.");
    Ok(())
}

fn current_unix_time() -> Result<u64, CliError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| CliError::Clock)
}

fn release_signing_key(key_id: &str) -> Result<SigningKey, CliError> {
    let signing_key_hex =
        Zeroizing::new(env::var(SIGNING_KEY_ENV).map_err(|_| CliError::MissingSigningKey)?);
    let signing_key =
        SigningKey::from_seed_hex(&signing_key_hex).map_err(|_| CliError::InvalidSigningKey)?;
    let configured = env::var(VERIFYING_KEYS_ENV).map_err(|_| CliError::MissingVerifyingKeys)?;
    let keys = configured
        .split(';')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (id, value) = entry
                .split_once('=')
                .ok_or(CliError::InvalidVerifyingKeys)?;
            VerifyingKey::from_base64(id.to_owned(), value)
                .map_err(|_| CliError::InvalidVerifyingKeys)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_verifying_key_set(&keys).map_err(|_| CliError::InvalidVerifyingKeys)?;
    if !keys.iter().any(|key| {
        key.key_id() == key_id && key.public_key_base64() == signing_key.public_key_base64()
    }) {
        return Err(CliError::SigningKeyNotTrusted);
    }
    Ok(signing_key)
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
struct EncryptArguments {
    output: PathBuf,
    manifest: PathBuf,
    remote_manifest: PathBuf,
    envelope_url: String,
    minimum_client_version: String,
    signing_key_id: String,
    channel: String,
    product_version: String,
    key_id: String,
}

impl EncryptArguments {
    fn parse(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, CliError> {
        let mut output = None;
        let mut manifest = None;
        let mut remote_manifest = None;
        let mut envelope_url = None;
        let mut minimum_client_version = None;
        let mut signing_key_id = None;
        let mut channel = None;
        let mut product_version = None;
        let mut key_id = None;

        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or(CliError::Usage)?;
            match flag.to_str() {
                Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
                Some("--manifest") if manifest.is_none() => manifest = Some(PathBuf::from(value)),
                Some("--remote-manifest") if remote_manifest.is_none() => {
                    remote_manifest = Some(PathBuf::from(value))
                }
                Some("--envelope-url") if envelope_url.is_none() => {
                    envelope_url = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                Some("--minimum-client-version") if minimum_client_version.is_none() => {
                    minimum_client_version = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                Some("--signing-key-id") if signing_key_id.is_none() => {
                    signing_key_id = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
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
            remote_manifest: remote_manifest.ok_or(CliError::Usage)?,
            envelope_url: envelope_url.ok_or(CliError::Usage)?,
            minimum_client_version: minimum_client_version.ok_or(CliError::Usage)?,
            signing_key_id: signing_key_id.ok_or(CliError::Usage)?,
            channel: channel.ok_or(CliError::Usage)?,
            product_version: product_version.ok_or(CliError::Usage)?,
            key_id: key_id.ok_or(CliError::Usage)?,
        };
        if parsed.output == parsed.manifest
            || parsed.output == parsed.remote_manifest
            || parsed.manifest == parsed.remote_manifest
        {
            return Err(CliError::Usage);
        }

        Ok(parsed)
    }
}

#[derive(Debug)]
struct LocatorArguments {
    output: PathBuf,
    sequence: u64,
    expires_at_unix: u64,
    signing_key_id: String,
    manifest_urls: Vec<String>,
}

#[derive(Debug)]
struct FileArguments {
    input: PathBuf,
    output: PathBuf,
}

impl FileArguments {
    fn parse(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, CliError> {
        let mut input = None;
        let mut output = None;
        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or(CliError::Usage)?;
            match flag.to_str() {
                Some("--input") if input.is_none() => input = Some(PathBuf::from(value)),
                Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
                _ => return Err(CliError::Usage),
            }
        }
        let parsed = Self {
            input: input.ok_or(CliError::Usage)?,
            output: output.ok_or(CliError::Usage)?,
        };
        if parsed.input == parsed.output {
            return Err(CliError::Usage);
        }
        Ok(parsed)
    }
}

impl LocatorArguments {
    fn parse(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<Self, CliError> {
        let mut output = None;
        let mut sequence = None;
        let mut expires_at_unix = None;
        let mut signing_key_id = None;
        let mut manifest_urls = Vec::new();
        while let Some(flag) = arguments.next() {
            let value = arguments.next().ok_or(CliError::Usage)?;
            match flag.to_str() {
                Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
                Some("--sequence") if sequence.is_none() => sequence = Some(parse_u64(value)?),
                Some("--expires-at-unix") if expires_at_unix.is_none() => {
                    expires_at_unix = Some(parse_u64(value)?)
                }
                Some("--signing-key-id") if signing_key_id.is_none() => {
                    signing_key_id = Some(value.into_string().map_err(|_| CliError::Usage)?)
                }
                Some("--manifest-url") => {
                    manifest_urls.push(value.into_string().map_err(|_| CliError::Usage)?)
                }
                _ => return Err(CliError::Usage),
            }
        }
        Ok(Self {
            output: output.ok_or(CliError::Usage)?,
            sequence: sequence.ok_or(CliError::Usage)?,
            expires_at_unix: expires_at_unix.ok_or(CliError::Usage)?,
            signing_key_id: signing_key_id.ok_or(CliError::Usage)?,
            manifest_urls,
        })
    }
}

fn parse_u64(value: std::ffi::OsString) -> Result<u64, CliError> {
    value
        .into_string()
        .map_err(|_| CliError::Usage)?
        .parse()
        .map_err(|_| CliError::Usage)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliError {
    Usage,
    Input,
    InputTooLarge,
    InvalidPlaintext,
    MissingKey,
    InvalidKey,
    MissingSigningKey,
    InvalidSigningKey,
    MissingVerifyingKeys,
    InvalidVerifyingKeys,
    SigningKeyNotTrusted,
    Clock,
    Encryption,
    Manifest,
    Signature,
    Output,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Usage => {
                "usage: orange-bootstrap-crypto encrypt --output FILE --manifest FILE --remote-manifest FILE --envelope-url HTTPS_URL --minimum-client-version VERSION --signing-key-id ID --channel NAME --product-version VERSION --key-id ID | sign-locator --output FILE --sequence N --expires-at-unix UNIX --signing-key-id ID --manifest-url HTTPS_URL [...] | sign-android-update --input FILE --output FILE"
            }
            Self::Input => "cannot read bootstrap plaintext from stdin",
            Self::InputTooLarge => "bootstrap plaintext exceeds the size limit",
            Self::InvalidPlaintext => "bootstrap plaintext is invalid",
            Self::MissingKey => "bootstrap build key environment variable is missing",
            Self::InvalidKey => "bootstrap build key must be 32 bytes encoded as hexadecimal",
            Self::MissingSigningKey => "bootstrap signing key environment variable is missing",
            Self::InvalidSigningKey => "bootstrap signing key must be a 32-byte hexadecimal seed",
            Self::MissingVerifyingKeys => {
                "bootstrap signing public key set environment variable is missing"
            }
            Self::InvalidVerifyingKeys => {
                "bootstrap signing public key set requires distinct current and next keys"
            }
            Self::SigningKeyNotTrusted => {
                "bootstrap signing key does not match its configured trusted public key"
            }
            Self::Clock => "system clock is unavailable",
            Self::Encryption => "bootstrap encryption failed",
            Self::Manifest => "bootstrap manifest generation failed",
            Self::Signature => "bootstrap signature generation failed",
            Self::Output => "bootstrap output could not be written",
        })
    }
}

impl std::error::Error for CliError {}
