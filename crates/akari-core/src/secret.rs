use std::fmt;

use zeroize::Zeroizing;

/// A Discord authentication token.
///
/// `Debug` prints `Token(<redacted>)`, and there is no `Display`: the value only leaves
/// through [`Token::expose`]. The memory is zeroed on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(Zeroizing<String>);

impl Token {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// A password or one-time code, handled like a [`Token`].
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_hides_the_token() {
        let token = Token::new("mfa.abc.def".to_owned());

        assert_eq!(format!("{token:?}"), "Token(<redacted>)");
        assert_eq!(token.expose(), "mfa.abc.def");
    }

    #[test]
    fn debug_hides_the_secret() {
        let password = Secret::new("hunter2".to_owned());

        assert!(!format!("{password:?}").contains("hunter2"));
        assert_eq!(password.expose(), "hunter2");
    }

    #[test]
    fn clones_keep_the_value() {
        let token = Token::new("abc".to_owned());

        assert_eq!(token.clone().expose(), "abc");
        assert_eq!(Secret::new("x".to_owned()).clone().expose(), "x");
    }
}
