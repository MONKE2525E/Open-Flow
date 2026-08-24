//! Device pairing: a SPAKE2 password-authenticated handshake over the (already
//! encrypted but not yet trusted) TLS channel.
//!
//! Flow - feels like pairing headphones:
//! 1. The initiator picks a device from "Nearby devices"; the app shows a
//!    6-digit code and sends `PairRequest` carrying its SPAKE2 message.
//! 2. The responder gets a prompt ("<Name> wants to pair") and types the code
//!    shown on the initiator.
//! 3. Both run SPAKE2 with the code as the password. A man-in-the-middle on
//!    the Wi-Fi doesn't know the code, so it derives a different key and the
//!    encrypted identity exchange below fails.
//! 4. Over the SPAKE2-derived key (ChaCha20-Poly1305), each side sends its
//!    identity: uuid, display name, and certificate. Each stores the other's
//!    certificate fingerprint as the pin for all future connections.
//!
//! After pairing the code is never needed again: connections authenticate by
//! presenting the pinned certificate.

use anyhow::{anyhow, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::protocol::{read_message, send_message, Message};

const PAIRING_INFO: &[u8] = b"verenu-lan-pairing-v1";
const SPAKE_ID_A: &[u8] = b"verenu/initiator";
const SPAKE_ID_B: &[u8] = b"verenu/responder";

/// What each side reveals about itself once the code is proven.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityExchange {
    pub device_uuid: String,
    pub device_name: String,
    /// DER-encoded self-signed certificate; its SHA-256 becomes the pin.
    pub cert_der: Vec<u8>,
}

/// A fresh numeric pairing code, e.g. "483920".
pub fn generate_pairing_code() -> String {
    let value = rand::thread_rng().gen_range(0..1_000_000);
    format!("{value:06}")
}

fn derive_cipher(shared: &[u8; 32]) -> ChaCha20Poly1305 {
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut okm = [0u8; 32];
    hk.expand(PAIRING_INFO, &mut okm)
        .expect("32-byte OKM is valid for HKDF-SHA256");
    ChaCha20Poly1305::new((&okm).into())
}

fn encrypt_identity(
    cipher: &ChaCha20Poly1305,
    identity: &IdentityExchange,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let plaintext = serde_json::to_vec(identity)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload { msg: &plaintext, aad: PAIRING_INFO },
        )
        .map_err(|_| anyhow!("identity encryption failed"))?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decryption failure means the two sides derived different keys - in other
/// words, the codes didn't match (or someone interfered). Both surface as the
/// same friendly error.
fn decrypt_identity(
    cipher: &ChaCha20Poly1305,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<IdentityExchange> {
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| anyhow!("bad nonce length"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload { msg: ciphertext, aad: PAIRING_INFO },
        )
        .map_err(|_| anyhow!("that code didn't match"))?;
    Ok(serde_json::from_slice(&plaintext)?)
}

/// The verified peer at the end of a successful handshake.
pub type PairingOutcome = IdentityExchange;

/// Starts the initiator side of SPAKE2. The returned message travels inside
/// the `PairRequest`; keep the state for [`initiator_cipher`].
pub fn initiator_start(code: &str) -> (Spake2<Ed25519Group>, Vec<u8>) {
    Spake2::<Ed25519Group>::start_a(
        &Password::new(code.as_bytes()),
        &Identity::new(SPAKE_ID_A),
        &Identity::new(SPAKE_ID_B),
    )
}

/// Responder side of SPAKE2. `initiator_msg` comes from the `PairRequest`.
/// Returns the responder's SPAKE2 message plus the derived cipher.
pub fn responder_start(code: &str, initiator_msg: &[u8]) -> Result<(Vec<u8>, ChaCha20Poly1305)> {
    let (state, msg) = Spake2::<Ed25519Group>::start_b(
        &Password::new(code.as_bytes()),
        &Identity::new(SPAKE_ID_A),
        &Identity::new(SPAKE_ID_B),
    );
    let shared = state
        .finish(initiator_msg)
        .map_err(|_| anyhow!("pairing handshake failed"))?;
    let shared: [u8; 32] = shared.try_into().map_err(|_| anyhow!("bad key length"))?;
    Ok((msg, derive_cipher(&shared)))
}

/// Completes the initiator side: derives the shared cipher from the
/// responder's SPAKE2 message.
pub fn initiator_cipher(
    state: Spake2<Ed25519Group>,
    responder_msg: &[u8],
) -> Result<ChaCha20Poly1305> {
    let shared = state
        .finish(responder_msg)
        .map_err(|_| anyhow!("pairing handshake failed"))?;
    let shared: [u8; 32] = shared.try_into().map_err(|_| anyhow!("bad key length"))?;
    Ok(derive_cipher(&shared))
}

/// Initiator side of the wire exchange, after the responder's SPAKE2 message
/// has been read and the shared cipher derived (the manager does both so it
/// can show the approval wait as one timeout). Sends the encrypted identity,
/// verifies the responder's, and waits for the completion marker.
pub async fn initiator_exchange<S>(
    stream: &mut S,
    cipher: &ChaCha20Poly1305,
    self_identity: &IdentityExchange,
    expected_peer_uuid: &str,
) -> Result<PairingOutcome>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (ciphertext, nonce) = encrypt_identity(cipher, self_identity)?;
    send_message(stream, &Message::PairVerify { ciphertext, nonce }).await?;

    let outcome = match read_message(stream).await? {
        Message::PairVerify { ciphertext, nonce } => decrypt_identity(cipher, &ciphertext, &nonce)?,
        Message::PairReject { reason } => return Err(anyhow!("pairing rejected: {reason}")),
        Message::Error { message } => return Err(anyhow!("pairing failed: {message}")),
        other => return Err(anyhow!("unexpected pairing message: {other:?}")),
    };
    if outcome.device_uuid != expected_peer_uuid {
        return Err(anyhow!("pairing peer identity changed mid-handshake"));
    }
    // The responder confirms it stored us.
    match read_message(stream).await? {
        Message::PairComplete => {}
        Message::Error { message } => return Err(anyhow!("pairing failed: {message}")),
        other => return Err(anyhow!("unexpected pairing message: {other:?}")),
    }
    Ok(outcome)
}

/// Responder side of the wire exchange, after the user approved with `code`.
/// Sends acceptance, exchanges encrypted identities, and returns the
/// initiator's verified identity.
pub async fn responder_exchange<S>(
    stream: &mut S,
    cipher: &ChaCha20Poly1305,
    responder_msg: Vec<u8>,
    self_identity: &IdentityExchange,
    expected_peer_uuid: &str,
) -> Result<PairingOutcome>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    send_message(stream, &Message::PairAccept { spake_msg: responder_msg }).await?;

    let (ciphertext, nonce) = match read_message(stream).await? {
        Message::PairVerify { ciphertext, nonce } => (ciphertext, nonce),
        Message::Error { message } => return Err(anyhow!("pairing failed: {message}")),
        other => return Err(anyhow!("unexpected pairing message: {other:?}")),
    };
    let peer = decrypt_identity(cipher, &ciphertext, &nonce)?;
    if peer.device_uuid != expected_peer_uuid {
        return Err(anyhow!("pairing peer identity changed mid-handshake"));
    }

    let (my_ciphertext, my_nonce) = encrypt_identity(cipher, self_identity)?;
    send_message(
        stream,
        &Message::PairVerify { ciphertext: my_ciphertext, nonce: my_nonce },
    )
    .await?;
    send_message(stream, &Message::PairComplete).await?;
    Ok(peer)
}
