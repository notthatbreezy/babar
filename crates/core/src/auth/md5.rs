//! MD5 password authentication.
//!
//! The wire format is `"md5" || hex(md5(hex(md5(password || username)) || salt))`.

use md5::{Digest, Md5};

/// Compute the `PasswordMessage` payload (without trailing NUL) for an
/// `AuthenticationMD5Password` request. The returned `String` includes the
/// `"md5"` prefix the server expects.
pub fn md5_password(user: &str, password: &str, salt: [u8; 4]) -> String {
    let inner = hex_md5_concat(password.as_bytes(), user.as_bytes());
    let outer = hex_md5_concat(inner.as_bytes(), &salt);
    format!("md5{outer}")
}

fn hex_md5_concat(a: &[u8], b: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(a);
    hasher.update(b);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(32);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified independently with Python:
    ///   inner = md5("secret" || "alice") = md5("secretalice")
    ///         = 4a0a68b43b6cd5cf266fa02f196e2371
    ///   outer = md5(hex(inner) || [01 02 03 04])
    ///         = 98a0412b9c31436fc53776e863350083
    #[test]
    fn md5_password_known_vector() {
        let got = md5_password("alice", "secret", [0x01, 0x02, 0x03, 0x04]);
        assert_eq!(got, "md598a0412b9c31436fc53776e863350083");
    }

    #[test]
    fn md5_password_includes_md5_prefix() {
        let got = md5_password("u", "p", [0, 0, 0, 0]);
        assert!(got.starts_with("md5"), "got: {got}");
        assert_eq!(got.len(), 3 + 32);
    }
}
