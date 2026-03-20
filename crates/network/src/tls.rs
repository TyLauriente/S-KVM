//! TLS certificate generation and TOFU (Trust On First Use) authentication.

use rcgen::{CertificateParams, DnType, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::path::Path;
use std::sync::Arc;

/// Generated TLS identity (certificate + private key).
pub struct TlsIdentity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint: String,
}

/// Generate a self-signed Ed25519 certificate for this peer.
pub fn generate_self_signed_cert(hostname: &str) -> Result<TlsIdentity, TlsError> {
    let key_pair = KeyPair::generate_for(&rcgen::PKCS_ED25519)
        .map_err(|e| TlsError::CertGeneration(e.to_string()))?;

    let mut params = CertificateParams::new(vec![hostname.to_string()])
        .map_err(|e| TlsError::CertGeneration(e.to_string()))?;
    params
        .distinguished_name
        .push(DnType::CommonName, hostname);
    params
        .distinguished_name
        .push(DnType::OrganizationName, "S-KVM");

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| TlsError::CertGeneration(e.to_string()))?;

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    // Compute SHA-256 fingerprint
    let fingerprint = compute_fingerprint(&cert_der);

    Ok(TlsIdentity {
        cert_der,
        key_der,
        fingerprint,
    })
}

/// Compute SHA-256 fingerprint of a DER-encoded certificate.
pub fn compute_fingerprint(cert_der: &[u8]) -> String {
    use std::fmt::Write;
    let digest = ring::digest::digest(&ring::digest::SHA256, cert_der);
    let mut hex = String::with_capacity(digest.as_ref().len() * 3);
    for (i, byte) in digest.as_ref().iter().enumerate() {
        if i > 0 {
            hex.push(':');
        }
        write!(hex, "{:02X}", byte).unwrap();
    }
    hex
}

/// Save certificate and key to files.
pub fn save_identity(identity: &TlsIdentity, cert_path: &Path, key_path: &Path) -> Result<(), TlsError> {
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TlsError::Io(e.to_string()))?;
    }
    std::fs::write(cert_path, &identity.cert_der).map_err(|e| TlsError::Io(e.to_string()))?;
    std::fs::write(key_path, &identity.key_der).map_err(|e| TlsError::Io(e.to_string()))?;
    Ok(())
}

/// Load certificate and key from files.
pub fn load_identity(cert_path: &Path, key_path: &Path) -> Result<TlsIdentity, TlsError> {
    let cert_der = std::fs::read(cert_path).map_err(|e| TlsError::Io(e.to_string()))?;
    let key_der = std::fs::read(key_path).map_err(|e| TlsError::Io(e.to_string()))?;
    let fingerprint = compute_fingerprint(&cert_der);
    Ok(TlsIdentity {
        cert_der,
        key_der,
        fingerprint,
    })
}

/// Load or generate TLS identity, persisting to disk.
pub fn load_or_generate_identity(
    cert_path: &Path,
    key_path: &Path,
    hostname: &str,
) -> Result<TlsIdentity, TlsError> {
    if cert_path.exists() && key_path.exists() {
        tracing::info!("Loading existing TLS identity");
        load_identity(cert_path, key_path)
    } else {
        tracing::info!("Generating new self-signed TLS certificate");
        let identity = generate_self_signed_cert(hostname)?;
        save_identity(&identity, cert_path, key_path)?;
        tracing::info!(fingerprint = %identity.fingerprint, "Certificate generated");
        Ok(identity)
    }
}

/// Create a rustls ServerConfig from a TLS identity.
pub fn make_server_config(identity: &TlsIdentity) -> Result<rustls::ServerConfig, TlsError> {
    let cert = CertificateDer::from(identity.cert_der.clone());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der.clone()));

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| TlsError::Config(e.to_string()))?;

    Ok(config)
}

/// Create a rustls ClientConfig that accepts any certificate (for TOFU).
pub fn make_client_config(identity: &TlsIdentity) -> Result<rustls::ClientConfig, TlsError> {
    let cert = CertificateDer::from(identity.cert_der.clone());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der.clone()));

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TofuCertVerifier::new()))
        .with_client_auth_cert(vec![cert], key)
        .map_err(|e| TlsError::Config(e.to_string()))?;

    Ok(config)
}

/// Generate a 6-digit pairing code.
pub fn generate_pairing_code() -> String {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:06}", (seed % 1_000_000) as u32)
}

/// TOFU certificate verifier — accepts any certificate on first use.
#[derive(Debug)]
struct TofuCertVerifier;

impl TofuCertVerifier {
    fn new() -> Self {
        Self
    }
}

impl rustls::client::danger::ServerCertVerifier for TofuCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let fingerprint = compute_fingerprint(end_entity.as_ref());
        tracing::info!(fingerprint = %fingerprint, "TOFU: accepting server certificate");
        // In production, this would check against stored fingerprints
        // and prompt the user for unknown certificates
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("Certificate generation failed: {0}")]
    CertGeneration(String),
    #[error("TLS configuration error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(String),
}
