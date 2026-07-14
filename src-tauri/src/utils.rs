use std::path::PathBuf;

pub fn lowercase_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::lowercase_hex;
    use sha2::{Digest, Sha256};

    #[test]
    fn sha2_0_10_persisted_digest_goldens_remain_stable() {
        let vectors: Vec<(Vec<u8>, &str)> = vec![
            (
                Vec::new(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                b"abc".to_vec(),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                (0_u8..=255).collect(),
                "40aff2e9d2d8922e47afd4648e6967497158785fbd1da870e7110266bf944880",
            ),
            (
                b"SpecCompanion persisted digest compatibility".to_vec(),
                "3c721c4f21c817cf4388122bb50b638ce8ca4ae24a2d35c4fc7321436eb68916",
            ),
        ];

        for (input, expected) in vectors {
            assert_eq!(lowercase_hex(Sha256::digest(input)), expected);
        }
    }
}
