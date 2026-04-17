/// Current protocol version. Increment on breaking changes.
pub const PROTOCOL_VERSION: u8 = 1;

/// Minimum client protocol version the server will accept.
pub const MIN_CLIENT_VERSION: u8 = 1;

/// Validate an envelope's protocol version against the server minimum.
pub fn check_version(envelope_version: u8) -> Result<(), String> {
    if envelope_version < MIN_CLIENT_VERSION {
        return Err(format!(
            "protocol version {envelope_version} too old, minimum is {MIN_CLIENT_VERSION}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_current_version() {
        assert!(check_version(PROTOCOL_VERSION).is_ok());
    }

    #[test]
    fn rejects_old_version() {
        if MIN_CLIENT_VERSION > 0 {
            assert!(check_version(MIN_CLIENT_VERSION - 1).is_err());
        }
    }
}
