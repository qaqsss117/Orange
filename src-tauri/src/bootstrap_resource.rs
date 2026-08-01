use std::{fmt, sync::Arc};

#[cfg(orange_embedded_bootstrap)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(orange_embedded_bootstrap)]
use orange_bootstrap::{BootstrapKey, BootstrapManifest, decrypt};
#[cfg(orange_embedded_bootstrap)]
use orange_control_plane_host::HostOptions;

use crate::control_plane::ManagedControlPlane;

#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_ENVELOPE: &[u8] = include_bytes!(env!("ORANGE_BOOTSTRAP_ENVELOPE_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_MANIFEST: &str = include_str!(env!("ORANGE_BOOTSTRAP_MANIFEST_PATH"));
#[cfg(orange_embedded_bootstrap)]
const EMBEDDED_KEY: &[u8; 32] = include_bytes!(env!("ORANGE_BOOTSTRAP_KEY_PATH"));

pub(crate) fn start_embedded(
    control_plane: &Arc<ManagedControlPlane>,
) -> Result<bool, EmbeddedBootstrapError> {
    #[cfg(not(orange_embedded_bootstrap))]
    {
        let _ = control_plane;
        Ok(false)
    }

    #[cfg(orange_embedded_bootstrap)]
    {
        let manifest: BootstrapManifest = serde_json::from_str(EMBEDDED_MANIFEST)
            .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
        let key = BootstrapKey::from_bytes(*EMBEDDED_KEY);
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| EmbeddedBootstrapError::Clock)?
            .as_secs();
        let mut secret = decrypt(EMBEDDED_ENVELOPE, &manifest, &key, now_unix)
            .map_err(|_| EmbeddedBootstrapError::InvalidResource)?;
        control_plane
            .start(&mut secret, 0, HostOptions::default())
            .map_err(|_| EmbeddedBootstrapError::Unavailable)?;
        Ok(true)
    }
}

#[cfg_attr(not(orange_embedded_bootstrap), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedBootstrapError {
    InvalidResource,
    Clock,
    Unavailable,
}

impl fmt::Display for EmbeddedBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidResource => "embedded-bootstrap-invalid",
            Self::Clock => "embedded-bootstrap-clock-unavailable",
            Self::Unavailable => "embedded-bootstrap-unavailable",
        })
    }
}

impl std::error::Error for EmbeddedBootstrapError {}
