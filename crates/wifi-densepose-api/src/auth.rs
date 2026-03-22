//! JWT auth middleware and token utilities.

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,    // subject (user/device ID)
    pub exp: u64,       // expiry Unix timestamp
    pub iat: u64,       // issued-at
    pub role: String,   // "sensor", "viewer", "admin"
}

pub struct JwtConfig {
    pub secret:      String,
    pub expiry_secs: u64,
}

/// Issue a new JWT token.
pub fn issue_token(
    subject: impl Into<String>,
    role: impl Into<String>,
    cfg: &JwtConfig,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        sub:  subject.into(),
        iat:  now,
        exp:  now + cfg.expiry_secs,
        role: role.into(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.secret.as_bytes()),
    )
}

/// Validate a JWT bearer token. Returns Claims on success.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(data.claims)
}

/// Extract Bearer token from Authorization header value.
pub fn extract_bearer(header: &str) -> Option<&str> {
    header.strip_prefix("Bearer ").map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_and_validate() {
        let cfg = JwtConfig { secret: "test-secret-12345".into(), expiry_secs: 3600 };
        let token = issue_token("device-0", "sensor", &cfg).unwrap();
        let claims = validate_token(&token, &cfg.secret).unwrap();
        assert_eq!(claims.sub, "device-0");
        assert_eq!(claims.role, "sensor");
    }

    #[test]
    fn test_invalid_token_rejected() {
        let result = validate_token("not.a.token", "secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_bearer() {
        assert_eq!(extract_bearer("Bearer abc123"), Some("abc123"));
        assert_eq!(extract_bearer("Token abc123"), None);
    }
}
