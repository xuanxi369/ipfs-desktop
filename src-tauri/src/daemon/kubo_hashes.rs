//! Kubo release verification helpers.
//!
//! The project previously embedded placeholder SHA-256 values. Placeholder
//! digests are worse than having no database because they present a false
//! trust signal. Bundled Kubo archives are now verified against the official
//! SHA-512 sidecar by `scripts/setup-kubo.ps1`; users may additionally pin an
//! exact executable SHA-256 through `kubo_binary_sha256`.

/// Empty known-hash provider retained for API compatibility.
///
/// A version is added here only after its executable digest has been obtained
/// from and independently checked against an official Kubo release artifact.
pub struct KuboHashes;

impl KuboHashes {
    pub fn get() -> Self {
        Self
    }

    pub fn get_current_platform() -> String {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        match (os, arch) {
            ("macos", "x86_64") => "darwin_amd64".to_string(),
            ("macos", "aarch64") => "darwin_arm64".to_string(),
            ("linux", "x86_64") => "linux_amd64".to_string(),
            ("linux", "aarch64") => "linux_arm64".to_string(),
            ("windows", "x86_64") => "windows_amd64".to_string(),
            _ => format!("{}_{}", os, arch),
        }
    }

    /// No executable digest is claimed unless a verified database is added.
    pub fn get_hash_for_version(&self, _version: &str, _platform: &str) -> Option<&str> {
        None
    }

    pub fn extract_version(version_str: &str) -> Option<String> {
        let parts: Vec<&str> = version_str.split_whitespace().collect();
        if parts.len() >= 3 && parts[0].eq_ignore_ascii_case("ipfs") && parts[1] == "version" {
            return Some(parts[2].to_string());
        }

        let re = regex::Regex::new(r"\d+\.\d+\.\d+").ok()?;
        re.find(version_str).map(|m| m.as_str().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_claim_unverified_hashes() {
        let db = KuboHashes::get();
        assert_eq!(
            db.get_hash_for_version("0.42.0", &KuboHashes::get_current_platform()),
            None
        );
    }

    #[test]
    fn current_platform_is_not_empty() {
        assert!(!KuboHashes::get_current_platform().is_empty());
    }

    #[test]
    fn extracts_version() {
        assert_eq!(
            KuboHashes::extract_version("ipfs version 0.42.0"),
            Some("0.42.0".to_string())
        );
        assert_eq!(
            KuboHashes::extract_version("Kubo 0.42.0"),
            Some("0.42.0".to_string())
        );
    }
}
