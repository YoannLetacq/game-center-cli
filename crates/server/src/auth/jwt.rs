use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Username
    pub username: String,
    /// Expiry (unix timestamp)
    pub exp: i64,
    /// Issued at (unix timestamp)
    pub iat: i64,
}

pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    expiry_secs: i64,
}

impl JwtManager {
    pub fn new(secret: &[u8], expiry_secs: i64) -> Self {
        // With the aws_lc_rs feature, jsonwebtoken auto-detects the crypto provider.

        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            expiry_secs,
        }
    }

    pub fn create_token(&self, user_id: &str, username: &str) -> Result<(String, i64), String> {
        let now = chrono_now();
        let exp = now + self.expiry_secs;

        let claims = Claims {
            sub: user_id.to_string(),
            username: username.to_string(),
            exp,
            iat: now,
        };

        let token = encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| format!("failed to create token: {e}"))?;

        Ok((token, exp))
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, String> {
        let mut validation = Validation::default();
        validation.set_required_spec_claims(&["sub", "exp", "iat"]);

        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|e| format!("invalid token: {e}"))
    }
}

/// Current unix timestamp in seconds.
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    use jsonwebtoken::{EncodingKey, Header, encode};

    fn test_manager() -> JwtManager {
        JwtManager::new(b"test-secret-key-for-unit-tests", 3600)
    }

    #[test]
    fn create_and_validate_token() {
        let mgr = test_manager();
        let (token, exp) = mgr.create_token("user-123", "alice").unwrap();

        assert!(!token.is_empty());
        assert!(exp > chrono_now());

        let claims = mgr.validate_token(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.username, "alice");
    }

    #[test]
    fn reject_invalid_token() {
        let mgr = test_manager();
        let result = mgr.validate_token("garbage.token.here");
        assert!(result.is_err());
    }

    #[test]
    fn reject_token_signed_with_different_secret() {
        let mgr1 = JwtManager::new(b"secret-one", 3600);
        let mgr2 = JwtManager::new(b"secret-two", 3600);

        let (token, _) = mgr1.create_token("user-1", "bob").unwrap();
        let result = mgr2.validate_token(&token);
        assert!(result.is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        let mgr = JwtManager::new(b"test-secret", 3600);
        // Manually create an already-expired token
        let claims = Claims {
            sub: "user-1".to_string(),
            username: "charlie".to_string(),
            exp: 1000, // far in the past
            iat: 999,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();
        let result = mgr.validate_token(&token);
        assert!(result.is_err());
    }
}
