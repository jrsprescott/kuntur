//! Huellas de los servidores (`known_hosts`, formato de OpenSSH, con entradas con hash incluidas).
//!
//! Se consultan varios archivos: el de OpenSSH del usuario (`~/.ssh/known_hosts`, **solo lectura**:
//! así un servidor que ya se usaba con `ssh` no vuelve a preguntar) y el de Kuntur
//! (`%APPDATA%\Kuntur\known_hosts`), que es el único donde Kuntur escribe.

use std::path::{Path, PathBuf};

use russh::keys::{HashAlg, PublicKey};

/// Resultado de comparar la llave que presenta el servidor con las recordadas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// Coincide con una llave recordada.
    Known,
    /// Primera vez: hay que pedir confirmación mostrando la huella.
    Unknown,
    /// Hay una llave recordada del mismo tipo y es **otra**: posible ataque, se bloquea.
    Changed { file: PathBuf, line: usize },
}

#[derive(Clone, Debug)]
pub struct KnownHosts {
    /// Archivos que solo se leen (el de OpenSSH).
    read_only: Vec<PathBuf>,
    /// Archivo propio: se lee y es donde se recuerdan las llaves nuevas.
    own: PathBuf,
}

impl KnownHosts {
    pub fn new(own: PathBuf, read_only: Vec<PathBuf>) -> Self {
        Self { read_only, own }
    }

    /// `%APPDATA%\Kuntur\known_hosts` + `~/.ssh/known_hosts` (solo lectura).
    pub fn for_user(app_dir: &Path, home: Option<&Path>) -> Self {
        let read_only = home
            .map(|h| vec![h.join(".ssh").join("known_hosts")])
            .unwrap_or_default();
        Self::new(app_dir.join("known_hosts"), read_only)
    }

    pub fn own_file(&self) -> &Path {
        &self.own
    }

    /// Busca la llave del servidor en todos los archivos. Una llave distinta del mismo tipo en
    /// cualquiera de ellos gana sobre todo lo demás (no se puede "tapar" con otro archivo).
    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyStatus {
        let mut known = false;
        for file in self.read_only.iter().chain(std::iter::once(&self.own)) {
            match russh::keys::check_known_hosts_path(host, port, key, file) {
                Ok(true) => known = true,
                Ok(false) => {}
                Err(russh::keys::Error::KeyChanged { line }) => {
                    return HostKeyStatus::Changed {
                        file: file.clone(),
                        line,
                    };
                }
                // Archivo ilegible o con líneas rotas: no cuenta como "conocido".
                Err(_) => {}
            }
        }
        if known {
            HostKeyStatus::Known
        } else {
            HostKeyStatus::Unknown
        }
    }

    /// Recuerda la llave (solo en el archivo de Kuntur).
    pub fn learn(&self, host: &str, port: u16, key: &PublicKey) -> anyhow::Result<()> {
        russh::keys::known_hosts::learn_known_hosts_path(host, port, key, &self.own)?;
        Ok(())
    }
}

/// Huella como la muestra OpenSSH: `SHA256:…` (base64 sin relleno).
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Nombre corto del tipo de llave: ED25519, RSA, ECDSA…
pub fn key_kind(key: &PublicKey) -> String {
    let name = key.algorithm().as_str().to_string();
    let short = name
        .strip_prefix("ssh-")
        .or_else(|| name.strip_prefix("ecdsa-sha2-").map(|_| "ecdsa"))
        .unwrap_or(&name);
    short.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::ssh_key::public::{Ed25519PublicKey, KeyData};

    fn key(seed: u8) -> PublicKey {
        PublicKey::new(KeyData::Ed25519(Ed25519PublicKey([seed; 32])), "")
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kuntur-kh-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn nueva_conocida_y_cambiada() {
        let dir = tmp("ciclo");
        let kh = KnownHosts::new(dir.join("known_hosts"), vec![dir.join("no-existe")]);
        let k = key(7);
        assert_eq!(kh.check("srv.test", 22, &k), HostKeyStatus::Unknown);
        kh.learn("srv.test", 22, &k).unwrap();
        assert_eq!(kh.check("srv.test", 22, &k), HostKeyStatus::Known);
        // El puerto cuenta: [srv.test]:2222 es otro servidor.
        assert_eq!(kh.check("srv.test", 2222, &k), HostKeyStatus::Unknown);
        // Misma máquina, otra llave del mismo tipo: bloqueo.
        assert!(matches!(
            kh.check("srv.test", 22, &key(8)),
            HostKeyStatus::Changed { file, .. } if file == dir.join("known_hosts")
        ));
    }

    #[test]
    fn lee_el_archivo_de_openssh_sin_escribirlo() {
        let dir = tmp("openssh");
        let openssh = dir.join("openssh_known_hosts");
        let k = key(3);
        // Entrada en formato OpenSSH (con un comentario y otra máquina antes).
        let line = format!(
            "# comentario\notro.test {}\n[srv.test]:2200,10.0.0.9 {}\n",
            key(1).to_openssh().unwrap(),
            k.to_openssh().unwrap()
        );
        std::fs::write(&openssh, &line).unwrap();
        let kh = KnownHosts::new(dir.join("propio"), vec![openssh.clone()]);
        assert_eq!(kh.check("srv.test", 2200, &k), HostKeyStatus::Known);
        // Sin corchetes, la entrada es del puerto 22.
        assert_eq!(kh.check("10.0.0.9", 22, &k), HostKeyStatus::Known);
        assert_eq!(kh.check("10.0.0.9", 2200, &k), HostKeyStatus::Unknown);
        assert!(matches!(
            kh.check("otro.test", 22, &k),
            HostKeyStatus::Changed { line: 1, .. }
        ));
        kh.learn("nuevo.test", 22, &k).unwrap();
        assert_eq!(
            std::fs::read_to_string(&openssh).unwrap(),
            line,
            "no se toca el de OpenSSH"
        );
        assert!(dir.join("propio").is_file());
    }

    #[test]
    fn entradas_con_hash() {
        let dir = tmp("hash");
        let file = dir.join("kh");
        // `ssh-keygen -H` guarda HMAC-SHA1(sal, host): esta es una entrada con hash de "srv.test"
        // calculada aparte (sal y hash sintéticos, no de una máquina real).
        let k = key(5);
        let hashed = hashed_entry("srv.test");
        std::fs::write(&file, format!("{hashed} {}\n", k.to_openssh().unwrap())).unwrap();
        let kh = KnownHosts::new(dir.join("propio"), vec![file]);
        assert_eq!(kh.check("srv.test", 22, &k), HostKeyStatus::Known);
        assert_eq!(kh.check("otro.test", 22, &k), HostKeyStatus::Unknown);
    }

    /// `|1|base64(sal)|base64(HMAC-SHA1(sal, host))`, con la sal fija de la prueba.
    fn hashed_entry(host: &str) -> String {
        let salt = [42u8; 20];
        let mac = hmac_sha1(&salt, host.as_bytes());
        format!("|1|{}|{}", b64(&salt), b64(&mac))
    }

    fn b64(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// HMAC-SHA1 mínimo para la prueba (RFC 2104).
    fn hmac_sha1(key: &[u8], msg: &[u8]) -> [u8; 20] {
        let mut k = [0u8; 64];
        k[..key.len()].copy_from_slice(key);
        let inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).chain(msg.iter().copied()).collect();
        let ih = sha1(&inner);
        let outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).chain(ih.iter().copied()).collect();
        sha1(&outer)
    }

    fn sha1(data: &[u8]) -> [u8; 20] {
        let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
        let mut m = data.to_vec();
        let bits = (data.len() as u64) * 8;
        m.push(0x80);
        while m.len() % 64 != 56 {
            m.push(0);
        }
        m.extend_from_slice(&bits.to_be_bytes());
        for block in m.chunks(64) {
            let mut w = [0u32; 80];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
            }
            for i in 16..80 {
                w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
            }
            let [mut a, mut b, mut c, mut d, mut e] = h;
            for (i, wi) in w.iter().enumerate() {
                let (f, k) = match i {
                    0..=19 => ((b & c) | (!b & d), 0x5A827999),
                    20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                    40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                    _ => (b ^ c ^ d, 0xCA62C1D6),
                };
                let t = a
                    .rotate_left(5)
                    .wrapping_add(f)
                    .wrapping_add(e)
                    .wrapping_add(k)
                    .wrapping_add(*wi);
                e = d;
                d = c;
                c = b.rotate_left(30);
                b = a;
                a = t;
            }
            for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
                *x = x.wrapping_add(y);
            }
        }
        let mut out = [0u8; 20];
        for (i, v) in h.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
        }
        out
    }

    #[test]
    fn sha1_de_referencia() {
        // Vector de prueba de FIPS 180-1.
        let d = sha1(b"abc");
        assert_eq!(b64(&d), "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=");
    }

    #[test]
    fn huella_y_tipo() {
        let k = key(9);
        let f = fingerprint(&k);
        assert!(f.starts_with("SHA256:"), "{f}");
        assert!(!f.ends_with('='));
        assert_eq!(key_kind(&k), "ED25519");
    }
}
