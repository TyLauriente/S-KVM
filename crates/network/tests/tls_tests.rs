//! TLS certificate generation and management tests.

use s_kvm_network::tls::*;
use rustls;

#[test]
fn generate_self_signed_cert_succeeds() {
    let identity = generate_self_signed_cert("test-machine").unwrap();
    assert!(!identity.cert_der.is_empty());
    assert!(!identity.key_der.is_empty());
    assert!(!identity.fingerprint.is_empty());
}

#[test]
fn fingerprint_is_sha256_hex() {
    let identity = generate_self_signed_cert("test").unwrap();
    // SHA-256 produces 32 bytes = 64 hex chars + 31 colons = 95 chars
    assert_eq!(identity.fingerprint.len(), 95);
    // All characters are hex digits or colons
    assert!(identity.fingerprint.chars().all(|c| c.is_ascii_hexdigit() || c == ':'));
}

#[test]
fn fingerprint_is_deterministic_for_same_cert() {
    let identity = generate_self_signed_cert("test").unwrap();
    let fp1 = compute_fingerprint(&identity.cert_der);
    let fp2 = compute_fingerprint(&identity.cert_der);
    assert_eq!(fp1, fp2);
}

#[test]
fn different_certs_have_different_fingerprints() {
    let id1 = generate_self_signed_cert("machine-1").unwrap();
    let id2 = generate_self_signed_cert("machine-2").unwrap();
    assert_ne!(id1.fingerprint, id2.fingerprint);
}

#[test]
fn save_and_load_identity() {
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.der");
    let key_path = dir.path().join("key.der");

    let original = generate_self_signed_cert("test").unwrap();
    save_identity(&original, &cert_path, &key_path).unwrap();

    let loaded = load_identity(&cert_path, &key_path).unwrap();
    assert_eq!(original.cert_der, loaded.cert_der);
    assert_eq!(original.key_der, loaded.key_der);
    assert_eq!(original.fingerprint, loaded.fingerprint);
}

#[test]
fn load_or_generate_creates_new() {
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.der");
    let key_path = dir.path().join("key.der");

    assert!(!cert_path.exists());
    let identity = load_or_generate_identity(&cert_path, &key_path, "test").unwrap();
    assert!(cert_path.exists());
    assert!(key_path.exists());
    assert!(!identity.fingerprint.is_empty());
}

#[test]
fn load_or_generate_loads_existing() {
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.der");
    let key_path = dir.path().join("key.der");

    let first = load_or_generate_identity(&cert_path, &key_path, "test").unwrap();
    let second = load_or_generate_identity(&cert_path, &key_path, "test").unwrap();

    assert_eq!(first.fingerprint, second.fingerprint);
}

#[test]
fn make_server_config_succeeds() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let identity = generate_self_signed_cert("test").unwrap();
    let config = make_server_config(&identity);
    assert!(config.is_ok());
}

#[test]
fn make_client_config_succeeds() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let identity = generate_self_signed_cert("test").unwrap();
    let config = make_client_config(&identity);
    assert!(config.is_ok());
}

#[test]
fn pairing_code_is_six_digits() {
    let code = generate_pairing_code();
    assert_eq!(code.len(), 6);
    assert!(code.chars().all(|c| c.is_ascii_digit()));
}
