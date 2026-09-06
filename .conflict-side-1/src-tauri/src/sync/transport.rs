//! TLS transport for sync connections.
//!
//! Each device presents a self-signed certificate (its identity). We do not
//! use the WebPKI - there is no CA on a LAN - so the rustls verifiers here
//! accept any certificate at the TLS layer, and authentication happens at the
//! protocol layer: the `Hello` message names a device uuid, and the manager
//! checks the connection's certificate fingerprint against the fingerprint
//! pinned for that uuid when it was paired. An unpaired device can encrypt to
//! us all it likes; the first protocol message it sends is rejected unless the
//! fingerprint matches a paired peer (or the user is actively pairing).

use anyhow::{anyhow, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, ServerConfig, SignatureScheme};
use std::sync::Arc;

#[derive(Debug)]
pub struct AcceptAnyServer {
    provider: CryptoProvider,
}

impl AcceptAnyServer {
    pub fn new(provider: CryptoProvider) -> Arc<Self> {
        Arc::new(Self { provider })
    }
}

impl ServerCertVerifier for AcceptAnyServer {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        // Encryption only; identity is pinned at the protocol layer.
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Debug)]
pub struct AcceptAnyClient {
    provider: CryptoProvider,
}

impl AcceptAnyClient {
    pub fn new(provider: CryptoProvider) -> Arc<Self> {
        Arc::new(Self { provider })
    }
}

impl ClientCertVerifier for AcceptAnyClient {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn crypto_provider() -> CryptoProvider {
    rustls::crypto::ring::default_provider()
}

pub fn server_config(
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
) -> Result<Arc<ServerConfig>> {
    let provider = crypto_provider();
    let config = ServerConfig::builder_with_provider(provider.clone().into())
        .with_safe_default_protocol_versions()
        .map_err(|e| anyhow!("tls protocol setup failed: {e}"))?
        .with_client_cert_verifier(AcceptAnyClient::new(provider))
        .with_single_cert(vec![cert], PrivateKeyDer::Pkcs8(key))
        .map_err(|e| anyhow!("tls server certificate rejected: {e}"))?;
    Ok(Arc::new(config))
}

pub fn client_config(
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
) -> Result<Arc<ClientConfig>> {
    let provider = crypto_provider();
    let config = ClientConfig::builder_with_provider(provider.clone().into())
        .with_safe_default_protocol_versions()
        .map_err(|e| anyhow!("tls protocol setup failed: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(AcceptAnyServer::new(provider))
        .with_client_auth_cert(vec![cert], PrivateKeyDer::Pkcs8(key))
        .map_err(|e| anyhow!("tls client certificate rejected: {e}"))?;
    Ok(Arc::new(config))
}

/// SHA-256 fingerprint of the leaf certificate a peer presented on a live
/// connection. `conn.peer_certificates()` yields the chain; the leaf is first.
pub fn peer_fingerprint(certs: &[CertificateDer<'_>]) -> Result<String> {
    let leaf = certs
        .first()
        .ok_or_else(|| anyhow!("peer presented no certificate"))?;
    Ok(super::identity::fingerprint_of(leaf.as_ref()))
}

pub fn server_name_for(uuid: &str) -> ServerName<'static> {
    // The certificate's SAN is the device uuid; SNI just needs a valid name.
    ServerName::try_from(uuid.to_string())
        .unwrap_or_else(|_| ServerName::try_from("verenu.local".to_string()).expect("static name"))
}

pub fn tls_connector(config: Arc<ClientConfig>) -> tokio_rustls::TlsConnector {
    config.into()
}
