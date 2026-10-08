use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::encoding::{AsDer, PublicKeyX509Der};
use aws_lc_rs::error::Unspecified;
use aws_lc_rs::rsa::{
    KeySize, OAEP_SHA256_MGF1SHA256, OaepPrivateDecryptingKey, PrivateDecryptingKey,
};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

// One key per remote auth session: a new connection means a new QR code.
pub(super) struct RemoteAuthKey {
    private: OaepPrivateDecryptingKey,
    spki: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct CryptoError;

impl From<Unspecified> for CryptoError {
    fn from(_: Unspecified) -> Self {
        Self
    }
}

impl RemoteAuthKey {
    pub(super) fn generate() -> Result<Self, CryptoError> {
        let private = PrivateDecryptingKey::generate(KeySize::Rsa2048)?;
        let public: PublicKeyX509Der<'static> = private.public_key().as_der()?;
        Ok(Self {
            spki: public.as_ref().to_vec(),
            private: OaepPrivateDecryptingKey::new(private)?,
        })
    }

    pub(super) fn encoded_public_key(&self) -> String {
        STANDARD.encode(&self.spki)
    }

    pub(super) fn fingerprint(&self) -> String {
        fingerprint_of(&self.spki)
    }

    pub(super) fn decrypt(&self, ciphertext: &str) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let ciphertext = STANDARD.decode(ciphertext).map_err(|_| CryptoError)?;
        let mut plaintext = Zeroizing::new(vec![0; self.private.min_output_size()]);
        let len = self
            .private
            .decrypt(&OAEP_SHA256_MGF1SHA256, &ciphertext, &mut plaintext, None)?
            .len();
        plaintext.truncate(len);
        Ok(plaintext)
    }
}

pub(super) fn fingerprint_of(spki: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, spki))
}

pub(super) fn nonce_proof(nonce: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::rsa::{OAEP_SHA256_MGF1SHA256, OaepPublicEncryptingKey, PublicEncryptingKey};

    // From docs.discord.food's remote auth examples: the init key and the fingerprint the
    // gateway answered with.
    const EXAMPLE_SPKI: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAo2PGAKj4v6r6sPJtgJe2eIDCM8uEHKpYCSDmp+pun9vqiqPt4pDToS1vGtwTwc5hKKqtIo+I/5veBpGWSD/veuB0xVb/JbkPn847Q+mXAb6c9vRMJVkA7l9GaZdN49U5bnGJi009aNBoy9cAcP/19H6TLpHmZ9RojnqGqlCUdyAiqceTDTzPqov4ST3GJSyKPydL3ZVpPf5P/PGyNfISuESKA2CxGCoBvB4H6/FH7cwSFelyqhwwHPZcyxBjF/3iXx+k1PdS01y0NoTRun4p76bE9rWnecIWONPFvCkby8Xs/OqQ8QcAoLkfVj5L29Ut1+Kmwwfg3nzc4glZa6RuTwIDAQAB";
    const EXAMPLE_FINGERPRINT: &str = "UZ0-kOVzXDZTFVV5_QlpURSO2BQHrtkKWHNpIGoDI0k";

    fn encrypt_for(key: &RemoteAuthKey, plaintext: &[u8]) -> String {
        let der = STANDARD.decode(key.encoded_public_key()).unwrap();
        let public =
            OaepPublicEncryptingKey::new(PublicEncryptingKey::from_der(&der).unwrap()).unwrap();
        let mut ciphertext = vec![0; public.ciphertext_size()];
        let ciphertext = public
            .encrypt(&OAEP_SHA256_MGF1SHA256, plaintext, &mut ciphertext, None)
            .unwrap();
        STANDARD.encode(ciphertext)
    }

    #[test]
    fn fingerprint_matches_the_reference_example() {
        let spki = STANDARD.decode(EXAMPLE_SPKI).unwrap();

        assert_eq!(fingerprint_of(&spki), EXAMPLE_FINGERPRINT);
    }

    #[test]
    fn generated_keys_decrypt_what_their_public_key_encrypts() {
        let key = RemoteAuthKey::generate().unwrap();

        let decrypted = key.decrypt(&encrypt_for(&key, b"nonce bytes")).unwrap();

        assert_eq!(decrypted.as_slice(), b"nonce bytes");
        let spki = STANDARD.decode(key.encoded_public_key()).unwrap();
        assert_eq!(key.fingerprint(), fingerprint_of(&spki));
    }

    #[test]
    fn garbage_ciphertext_is_an_error() {
        let key = RemoteAuthKey::generate().unwrap();

        assert!(key.decrypt("not base64!").is_err());
        assert!(key.decrypt(&STANDARD.encode([0_u8; 256])).is_err());
    }

    #[test]
    fn nonce_proof_is_unpadded_base64url_of_the_raw_nonce() {
        assert_eq!(nonce_proof(&[0xfb, 0xff]), "-_8");
    }
}
