//! Operator-supplied artifacts, loaded from files with permission hardening.
//!
//! # What this module deliberately cannot do
//!
//! It cannot mint a Fleet root key, and it cannot mint an [`EnrollmentAuthorizationV1`]. Both are
//! signed by the Fleet **root**, which a client must never hold: a client that could self-sign an
//! authorization would defeat the admission control the envelope exists to provide, and Core would
//! reject it anyway because it verifies against its own root. They are therefore *inputs*, loaded
//! from operator-provided files.
//!
//! # Why files rather than argv or inline env
//!
//! Process arguments are world-readable on a typical host (`ps`), and inline environment values
//! leak into child processes and crash dumps. Reading from a file lets the operating system's
//! permission bits carry the secrecy guarantee, and lets this module *verify* those bits before
//! trusting the contents. This mirrors Fleet Core's own `private_seed` handling.

use crate::error::FleetClientError;
use fleet_enrollment::{EnrollmentAuthorizationV1, FleetSignedEnvelopeV1};
use std::path::Path;

fn credential_error(path: &Path, message: impl Into<String>) -> FleetClientError {
    FleetClientError::Credential {
        path: path.display().to_string(),
        message: message.into(),
    }
}

/// Reads a file that is expected to hold secret material, refusing unsafe permissions.
///
/// A group- or world-accessible secret is treated as a hard error rather than a warning: the
/// alternative is a client that keeps working while silently offering its credentials to every
/// local account, which is precisely the failure nobody notices until it matters.
#[cfg(unix)]
pub fn read_private_file(path: &Path) -> Result<Vec<u8>, FleetClientError> {
    use std::os::unix::fs::PermissionsExt as _;

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| credential_error(path, error.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(credential_error(
            path,
            "refusing to read a symlinked credential file; the link target's permissions are not \
             the ones checked here",
        ));
    }
    if !metadata.is_file() {
        return Err(credential_error(path, "not a regular file"));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(credential_error(
            path,
            format!(
                "is accessible to group or others (mode {:o}); tighten it to 0600",
                metadata.permissions().mode() & 0o777
            ),
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| credential_error(path, error.to_string()))?;
    if bytes.is_empty() {
        return Err(credential_error(path, "is empty"));
    }
    Ok(bytes)
}

/// Non-unix fallback: permission semantics differ, so only existence and non-emptiness are checked.
///
/// The weaker guarantee is stated rather than hidden, so a caller on such a platform knows the
/// filesystem is not enforcing secrecy for them.
#[cfg(not(unix))]
pub fn read_private_file(path: &Path) -> Result<Vec<u8>, FleetClientError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| credential_error(path, error.to_string()))?;
    if !metadata.is_file() {
        return Err(credential_error(path, "not a regular file"));
    }
    let bytes = std::fs::read(path).map_err(|error| credential_error(path, error.to_string()))?;
    if bytes.is_empty() {
        return Err(credential_error(path, "is empty"));
    }
    Ok(bytes)
}

/// Reads a 32-byte Ed25519 **public** key, accepting raw bytes or hex text.
///
/// Public keys are not secret, so permission hardening is intentionally not applied here; treating
/// them as secret would train operators to give secrets the wrong handling.
pub fn read_public_key(path: &Path) -> Result<Vec<u8>, FleetClientError> {
    let bytes = std::fs::read(path).map_err(|error| credential_error(path, error.to_string()))?;
    if bytes.len() == 32 {
        return Ok(bytes);
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| credential_error(path, "is neither 32 raw bytes nor hex text"))?;
    let trimmed = text.trim();
    if trimmed.len() != 64 {
        return Err(credential_error(
            path,
            format!(
                "must be 32 raw bytes or 64 hex characters, found {} characters",
                trimmed.len()
            ),
        ));
    }
    (0..32)
        .map(|index| {
            u8::from_str_radix(&trimmed[index * 2..index * 2 + 2], 16)
                .map_err(|_| credential_error(path, "contains malformed hex"))
        })
        .collect()
}

/// Loads the operator-supplied, Fleet-signed enrollment authorization.
///
/// The payload is validated on load so a malformed artifact fails locally with a precise message
/// instead of becoming an opaque `invalid_request` from Core.
pub fn read_authorization(
    path: &Path,
) -> Result<FleetSignedEnvelopeV1<EnrollmentAuthorizationV1>, FleetClientError> {
    let bytes = std::fs::read(path).map_err(|error| credential_error(path, error.to_string()))?;
    let envelope: FleetSignedEnvelopeV1<EnrollmentAuthorizationV1> = serde_json::from_slice(&bytes)
        .map_err(|error| {
            credential_error(path, format!("is not a Fleet-signed envelope: {error}"))
        })?;
    envelope
        .payload
        .validate()
        .map_err(|error| credential_error(path, format!("payload is malformed: {error:?}")))?;
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write(dir: &std::path::Path, name: &str, contents: &[u8], mode: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(contents).expect("write");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("chmod");
        }
        let _ = mode;
        path
    }

    #[test]
    fn public_key_accepts_hex_and_raw() {
        let dir = std::env::temp_dir().join(format!("fleet-client-pk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");

        let hex = write(&dir, "hex", b"ab".repeat(32).as_slice(), 0o600);
        assert_eq!(read_public_key(&hex).expect("hex key").len(), 32);

        let raw = write(&dir, "raw", &[7u8; 32], 0o600);
        assert_eq!(read_public_key(&raw).expect("raw key"), vec![7u8; 32]);

        let bad = write(&dir, "bad", b"short", 0o600);
        assert!(read_public_key(&bad).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn private_file_refuses_group_or_world_readable_modes() {
        let dir = std::env::temp_dir().join(format!("fleet-client-priv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");

        let ok = write(&dir, "ok", b"secret", 0o600);
        assert_eq!(read_private_file(&ok).expect("0600 is accepted"), b"secret");

        let loose = write(&dir, "loose", b"secret", 0o644);
        let error = read_private_file(&loose).expect_err("0644 must be refused");
        assert!(matches!(error, FleetClientError::Credential { .. }));

        let empty = write(&dir, "empty", b"", 0o600);
        assert!(read_private_file(&empty).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
