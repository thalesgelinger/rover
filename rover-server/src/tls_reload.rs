use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::TlsConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsMaterial {
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified_at: SystemTime,
    len: u64,
}

pub struct TlsCertReloader {
    cert_file: PathBuf,
    key_file: PathBuf,
    cert_stamp: FileStamp,
    key_stamp: FileStamp,
    current: Arc<RwLock<TlsMaterial>>,
}

impl TlsCertReloader {
    pub fn new(config: &TlsConfig) -> Result<Self> {
        let cert_file = PathBuf::from(&config.cert_file);
        let key_file = PathBuf::from(&config.key_file);
        let material = Self::load_material(&cert_file, &key_file)?;
        let cert_stamp = Self::file_stamp(&cert_file)?;
        let key_stamp = Self::file_stamp(&key_file)?;

        Ok(Self {
            cert_file,
            key_file,
            cert_stamp,
            key_stamp,
            current: Arc::new(RwLock::new(material)),
        })
    }

    pub fn force_reload(&mut self) -> Result<()> {
        let material = Self::load_material(&self.cert_file, &self.key_file)?;
        let cert_stamp = Self::file_stamp(&self.cert_file)?;
        let key_stamp = Self::file_stamp(&self.key_file)?;

        *self
            .current
            .write()
            .map_err(|_| anyhow!("tls material lock poisoned"))? = material;
        self.cert_stamp = cert_stamp;
        self.key_stamp = key_stamp;
        Ok(())
    }

    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let cert_stamp = Self::file_stamp(&self.cert_file)?;
        let key_stamp = Self::file_stamp(&self.key_file)?;
        if cert_stamp == self.cert_stamp && key_stamp == self.key_stamp {
            return Ok(false);
        }

        self.force_reload()?;
        Ok(true)
    }

    /// Get the current TLS material (for testing and introspection)
    pub fn current_material(&self) -> Result<TlsMaterial> {
        self.current
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| anyhow!("tls material lock poisoned"))
    }

    pub fn rustls_server_config(&self, alpn_protocols: &[&str]) -> Result<Arc<ServerConfig>> {
        self.current_material()?
            .rustls_server_config(alpn_protocols)
    }

    fn load_material(cert_file: &Path, key_file: &Path) -> Result<TlsMaterial> {
        let cert_pem = fs::read(cert_file)
            .with_context(|| format!("failed to read tls cert file {}", cert_file.display()))?;
        let key_pem = fs::read(key_file)
            .with_context(|| format!("failed to read tls key file {}", key_file.display()))?;

        Self::validate_pem("tls.cert_file", &cert_pem, "CERTIFICATE")?;
        Self::validate_pem("tls.key_file", &key_pem, "PRIVATE KEY")?;

        Ok(TlsMaterial { cert_pem, key_pem })
    }

    fn validate_pem(field_name: &str, bytes: &[u8], expected_marker: &str) -> Result<()> {
        if bytes.is_empty() {
            return Err(anyhow!("{} cannot be empty", field_name));
        }

        let text = String::from_utf8_lossy(bytes);
        if !text.contains("-----BEGIN ") || !text.contains(expected_marker) {
            return Err(anyhow!(
                "{} must be a PEM file containing {}",
                field_name,
                expected_marker
            ));
        }

        Ok(())
    }

    fn file_stamp(path: &Path) -> Result<FileStamp> {
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to stat tls file {}", path.display()))?;
        let modified_at = metadata
            .modified()
            .with_context(|| format!("failed to read mtime for {}", path.display()))?;
        Ok(FileStamp {
            modified_at,
            len: metadata.len(),
        })
    }
}

impl TlsMaterial {
    pub fn rustls_server_config(&self, alpn_protocols: &[&str]) -> Result<Arc<ServerConfig>> {
        let certs = parse_certificates(&self.cert_pem)?;
        let key = parse_private_key(&self.key_pem)?;

        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| anyhow!("failed to build rustls server config: {}", e))?;
        config.alpn_protocols = alpn_protocols
            .iter()
            .map(|protocol| protocol.as_bytes().to_vec())
            .collect();

        Ok(Arc::new(config))
    }
}

fn parse_certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    let mut reader = BufReader::new(pem);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow!("failed to parse tls.cert_file: {}", e))?;
    if certs.is_empty() {
        return Err(anyhow!(
            "tls.cert_file must contain at least one certificate"
        ));
    }
    Ok(certs)
}

fn parse_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>> {
    let mut reader = BufReader::new(pem);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| anyhow!("failed to parse tls.key_file: {}", e))?
        .ok_or_else(|| anyhow!("tls.key_file must contain a private key"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::TlsCertReloader;
    use crate::TlsConfig;
    use rcgen::generate_simple_self_signed;

    fn fixture_pem(content: &str, marker: &str) -> String {
        format!(
            "-----BEGIN {}-----\n{}\n-----END {}-----\n",
            marker, content, marker
        )
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("rover_server_{}_{}", name, nanos))
    }

    #[test]
    fn should_reload_tls_material_when_files_change() {
        let dir = unique_test_dir("tls_reload");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("old-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("old-key", "PRIVATE KEY")).expect("key write");

        let mut reloader = TlsCertReloader::new(&TlsConfig {
            cert_file: cert_file.to_string_lossy().to_string(),
            key_file: key_file.to_string_lossy().to_string(),
            reload_interval_secs: 1,
        })
        .expect("reloader");

        fs::write(&cert_file, fixture_pem("new-cert", "CERTIFICATE")).expect("cert update");
        fs::write(&key_file, fixture_pem("new-key", "PRIVATE KEY")).expect("key update");

        let changed = reloader.reload_if_changed().expect("reload changed");
        assert!(changed);

        let snapshot = reloader.current.read().expect("lock");
        let cert_text = String::from_utf8_lossy(&snapshot.cert_pem);
        let key_text = String::from_utf8_lossy(&snapshot.key_pem);
        assert!(cert_text.contains("new-cert"));
        assert!(key_text.contains("new-key"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_build_rustls_config_with_alpn_protocols() {
        let dir = unique_test_dir("tls_rustls_config");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert = generate_simple_self_signed(["localhost".to_string()]).expect("cert");
        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, cert.cert.pem()).expect("cert write");
        fs::write(&key_file, cert.key_pair.serialize_pem()).expect("key write");

        let reloader = TlsCertReloader::new(&TlsConfig {
            cert_file: cert_file.to_string_lossy().to_string(),
            key_file: key_file.to_string_lossy().to_string(),
            reload_interval_secs: 1,
        })
        .expect("reloader");

        let config = reloader
            .rustls_server_config(&["h2", "http/1.1"])
            .expect("rustls config");
        assert_eq!(
            config.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_keep_previous_material_when_reload_fails() {
        let dir = unique_test_dir("tls_reload_fail");
        fs::create_dir_all(&dir).expect("mkdir");

        let cert_file = dir.join("cert.pem");
        let key_file = dir.join("key.pem");
        fs::write(&cert_file, fixture_pem("old-cert", "CERTIFICATE")).expect("cert write");
        fs::write(&key_file, fixture_pem("old-key", "PRIVATE KEY")).expect("key write");

        let mut reloader = TlsCertReloader::new(&TlsConfig {
            cert_file: cert_file.to_string_lossy().to_string(),
            key_file: key_file.to_string_lossy().to_string(),
            reload_interval_secs: 1,
        })
        .expect("reloader");

        fs::write(&cert_file, b"not-pem").expect("cert update");
        let err = reloader.reload_if_changed().expect_err("reload must fail");
        assert!(err.to_string().contains("tls.cert_file"));

        let snapshot = reloader.current.read().expect("lock");
        let cert_text = String::from_utf8_lossy(&snapshot.cert_pem);
        assert!(cert_text.contains("old-cert"));

        let _ = fs::remove_dir_all(&dir);
    }
}
