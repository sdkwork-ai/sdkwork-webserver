use chrono::{Duration, TimeZone, Utc};
use rcgen::{
    CertificateParams, DistinguishedName, DnType, KeyPair, RsaKeySize, PKCS_ECDSA_P256_SHA256,
    PKCS_RSA_SHA256,
};
use sdkwork_deploy_core::{
    validate_certificate_key_algorithm, CERTIFICATE_KEY_ALGORITHM_ECDSA,
    CERTIFICATE_KEY_ALGORITHM_RSA,
};
use sdkwork_utils_rust::crypto::sha256_hash;
use time::OffsetDateTime;
use x509_parser::pem::parse_x509_pem;

use crate::model::IssuedCertificateMaterial;
use crate::{AcmeServiceError, AcmeServiceResult};

pub fn issue_self_signed(
    hostnames: &[String],
    cert_name: &str,
    cert_root: &str,
    key_algorithm: &str,
) -> AcmeServiceResult<IssuedCertificateMaterial> {
    let primary_hostname = hostnames.first().ok_or_else(|| {
        AcmeServiceError::validation("at least one certificate identifier is required")
    })?;
    let mut params = CertificateParams::new(hostnames.to_vec())
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, primary_hostname);

    let now = Utc::now();
    let not_before = now - Duration::minutes(5);
    let not_after = not_before + Duration::days(825);
    params.not_before = OffsetDateTime::from_unix_timestamp(not_before.timestamp())
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
    params.not_after = OffsetDateTime::from_unix_timestamp(not_after.timestamp())
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;

    let key_pair = generate_key_pair(key_algorithm)?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;

    let cert_pem = cert.pem();
    let private_key_pem = key_pair.serialize_pem();
    let evidence = certificate_evidence_from_pem(&cert_pem)?;
    let cert_dir = format!("{cert_root}/{cert_name}");
    let cert_path = format!("{cert_dir}/fullchain.pem");
    let key_path = format!("{cert_dir}/privkey.pem");

    Ok(IssuedCertificateMaterial {
        cert_name: cert_name.to_string(),
        cert_type: 3,
        issuer: evidence.issuer,
        subject: evidence.subject,
        san_list: evidence.san_list,
        serial_sha256: evidence.serial_sha256,
        fingerprint_sha256: evidence.fingerprint_sha256,
        spki_sha256: evidence.spki_sha256,
        chain_sha256: evidence.chain_sha256,
        key_algorithm: evidence.key_algorithm,
        cert_pem,
        private_key_pem,
        chain_pem: None,
        not_before: evidence.not_before,
        not_after: evidence.not_after,
        cert_path,
        key_path,
        chain_path: None,
    })
}

/// Generates the private key a certificate is issued with.
///
/// The algorithm is checked against the shared vocabulary rather than against
/// literals here: the control plane validates `preferredKeyAlgorithm` with the same
/// function, so a value that reaches this point is one of the accepted set and the
/// arm that reports otherwise is a guard against the vocabulary and this mapping
/// drifting apart — a drift whose symptom would otherwise be "every ECDSA order
/// fails" rather than a compile error.
///
/// RSA's modulus is **stated, not inherited**. `KeyPair::generate_for` decides it
/// inside rcgen (2048 today), and a security parameter that changes with a
/// dependency version is one nobody reviewed. `generate_rsa_for` makes it a value in
/// this file, and makes 3072 or 4096 a one-word change if a compliance regime ever
/// names them. ECDSA is P-256, the curve the recommended default resolves to.
pub(crate) fn generate_key_pair(key_algorithm: &str) -> AcmeServiceResult<KeyPair> {
    validate_certificate_key_algorithm(key_algorithm).map_err(AcmeServiceError::validation)?;
    let generated = match key_algorithm {
        CERTIFICATE_KEY_ALGORITHM_ECDSA => KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256),
        CERTIFICATE_KEY_ALGORITHM_RSA => {
            KeyPair::generate_rsa_for(&PKCS_RSA_SHA256, RsaKeySize::_2048)
        }
        other => {
            return Err(AcmeServiceError::validation(format!(
                "keyAlgorithm {other} is accepted by the vocabulary but has no key generation rule"
            )));
        }
    };
    generated
        .map_err(|error| AcmeServiceError::Internal(format!("generate certificate key: {error}")))
}

pub(crate) struct CertificateEvidence {
    pub issuer: String,
    pub subject: String,
    pub san_list: Vec<String>,
    pub serial_sha256: String,
    pub fingerprint_sha256: String,
    pub spki_sha256: String,
    pub chain_sha256: String,
    pub key_algorithm: String,
    pub not_before: String,
    pub not_after: String,
}

pub(crate) fn certificate_evidence_from_pem(
    pem_chain: &str,
) -> AcmeServiceResult<CertificateEvidence> {
    let (_, pem) = parse_x509_pem(pem_chain.as_bytes())
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
    if pem.label != "CERTIFICATE" {
        return Err(AcmeServiceError::Internal(
            "first PEM block is not a certificate".to_string(),
        ));
    }
    let cert = pem
        .parse_x509()
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?;
    let subject_alternative_name = cert
        .subject_alternative_name()
        .map_err(|error| AcmeServiceError::Internal(error.to_string()))?
        .ok_or_else(|| {
            AcmeServiceError::Internal("certificate has no SAN extension".to_string())
        })?;
    if subject_alternative_name
        .value
        .general_names
        .iter()
        .any(|name| !matches!(name, x509_parser::extensions::GeneralName::DNSName(_)))
    {
        return Err(AcmeServiceError::Internal(
            "certificate contains a non-DNS SAN identifier".to_string(),
        ));
    }
    let san_list = subject_alternative_name
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            x509_parser::extensions::GeneralName::DNSName(value) => Some((*value).to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if san_list.is_empty() {
        return Err(AcmeServiceError::Internal(
            "certificate has no DNS SAN identifiers".to_string(),
        ));
    }
    let public_key_oid = cert.public_key().algorithm.algorithm.to_id_string();
    let key_algorithm = match public_key_oid.as_str() {
        "1.2.840.10045.2.1" => "ECDSA",
        "1.2.840.113549.1.1.1" => "RSA",
        other => {
            return Err(AcmeServiceError::Internal(format!(
                "unsupported certificate public key algorithm {other}"
            )));
        }
    };
    Ok(CertificateEvidence {
        issuer: cert.issuer().to_string(),
        subject: cert.subject().to_string(),
        san_list,
        serial_sha256: sha256_hash(cert.raw_serial()),
        fingerprint_sha256: fingerprint_sha256_hex(&pem.contents),
        spki_sha256: sha256_hash(cert.public_key().raw),
        chain_sha256: sha256_hash(pem_chain.as_bytes()),
        key_algorithm: key_algorithm.to_string(),
        not_before: timestamp_to_rfc3339(cert.validity().not_before.timestamp())?,
        not_after: timestamp_to_rfc3339(cert.validity().not_after.timestamp())?,
    })
}

fn timestamp_to_rfc3339(timestamp: i64) -> AcmeServiceResult<String> {
    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.to_rfc3339())
        .ok_or_else(|| AcmeServiceError::Internal("certificate timestamp is invalid".to_string()))
}

pub fn fingerprint_sha256_hex(der: &[u8]) -> String {
    sha256_hash(der)
}

#[cfg(test)]
mod tests {
    use super::*;
    use x509_parser::extensions::GeneralName;

    #[test]
    fn self_signed_material_matches_actual_leaf_evidence() {
        let material = issue_self_signed(
            &["dev.localhost".to_string(), "www.dev.localhost".to_string()],
            "dev-localhost",
            "/tmp/certs/live",
            "ECDSA",
        )
        .expect("issue");
        let (_, pem) = parse_x509_pem(material.cert_pem.as_bytes()).expect("PEM");
        let cert = pem.parse_x509().expect("X.509");
        let sans = cert
            .subject_alternative_name()
            .expect("SAN extension")
            .expect("SAN present");
        assert!(sans
            .value
            .general_names
            .iter()
            .any(|name| matches!(name, GeneralName::DNSName(value) if *value == "dev.localhost")));

        let evidence = certificate_evidence_from_pem(&material.cert_pem).expect("evidence");
        assert_eq!(material.not_before, evidence.not_before);
        assert_eq!(material.not_after, evidence.not_after);
        assert_eq!(material.fingerprint_sha256, evidence.fingerprint_sha256);
        assert_eq!(
            evidence.fingerprint_sha256,
            fingerprint_sha256_hex(&pem.contents)
        );
        assert_eq!(material.key_algorithm, "ECDSA");
        assert_eq!(material.san_list.len(), 2);
        assert!(chrono::DateTime::parse_from_rfc3339(&material.not_before).is_ok());
        assert!(chrono::DateTime::parse_from_rfc3339(&material.not_after).is_ok());
        assert!(material.private_key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn self_signed_rsa_material_reports_actual_algorithm() {
        let material = issue_self_signed(
            &["rsa.dev.localhost".to_string()],
            "rsa-dev-localhost",
            "/tmp/certs/live",
            "RSA",
        )
        .expect("issue RSA");
        assert_eq!(material.key_algorithm, "RSA");
        assert!(material.private_key_pem.contains("BEGIN PRIVATE KEY"));
    }
}
