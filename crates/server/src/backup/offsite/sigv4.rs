//! Signature AWS Version 4 (`AWS4-HMAC-SHA256`), en en-têtes.
//!
//! Juste ce qu'il faut pour parler à S3 et à ses compatibles (MinIO,
//! Backblaze B2, Wasabi, Cloudflare R2) : requête canonique, chaîne à signer,
//! clé de signature dérivée par HMAC en cascade. Pas de normalisation de
//! chemin (S3 n'en fait pas) : le chemin canonique est celui qui part sur le
//! fil, chaque segment encodé une fois par l'appelant ([`uri_encode`]).
//!
//! Les tests rejouent les vecteurs publiés par AWS (suite de signature
//! `aws-signing-test-suite`, et l'exemple « GET Object » de la documentation
//! S3) : une erreur d'un caractère dans la forme canonique s'y voit.

use std::collections::BTreeMap;

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};

pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// Empreinte SHA-256 d'un corps vide.
pub const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Encode selon les règles d'AWS : seuls `A-Z a-z 0-9 - _ . ~` restent tels
/// quels, tout le reste devient `%XX` (majuscules), octet par octet. `/` est
/// gardé quand `encode_slash` est faux (chemin d'objet).
pub fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (byte == b'/' && !encode_slash);
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

/// Ce qui est signé d'une requête.
pub struct Request<'a> {
    pub method: &'a str,
    /// Chemin tel qu'il part sur le fil, déjà encodé.
    pub canonical_uri: &'a str,
    /// Paramètres de requête, **décodés** : ils sont encodés ici.
    pub query: &'a [(String, String)],
    /// En-têtes signés, `host` et `x-amz-date` compris.
    pub headers: &'a [(String, String)],
    /// SHA-256 hexadécimal du corps (ou `UNSIGNED-PAYLOAD`).
    pub payload_hash: &'a str,
}

/// Chaîne de requête canonique : clés et valeurs encodées, triées.
pub fn canonical_query(query: &[(String, String)]) -> String {
    let mut pairs: Vec<(String, String)> =
        query.iter().map(|(k, v)| (uri_encode(k, true), uri_encode(v, true))).collect();
    pairs.sort();
    pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// Valeur d'en-tête canonique : bords retirés, espaces internes réduits à un.
fn canonical_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Requête canonique et liste des en-têtes signés (`host;x-amz-date`).
pub fn canonical_request(request: &Request<'_>) -> (String, String) {
    let mut headers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in request.headers {
        headers.entry(name.trim().to_ascii_lowercase()).or_default().push(canonical_value(value));
    }
    let canonical_headers: String =
        headers.iter().map(|(name, values)| format!("{name}:{}\n", values.join(","))).collect();
    let signed = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method,
        request.canonical_uri,
        canonical_query(request.query),
        canonical_headers,
        signed,
        request.payload_hash
    );
    (canonical, signed)
}

/// `AAAAMMJJ/région/service/aws4_request`.
pub fn credential_scope(date: &str, region: &str, service: &str) -> String {
    format!("{date}/{region}/{service}/aws4_request")
}

pub fn string_to_sign(amz_date: &str, scope: &str, canonical_request: &str) -> String {
    format!("{ALGORITHM}\n{amz_date}\n{scope}\n{}", sha256_hex(canonical_request.as_bytes()))
}

/// Clé du jour, de la région et du service.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac(&k_date, region.as_bytes());
    let k_service = hmac(&k_region, service.as_bytes());
    hmac(&k_service, b"aws4_request")
}

/// Identifiants et portée d'une signature.
pub struct Signer<'a> {
    pub access_key: &'a str,
    pub secret_key: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

impl Signer<'_> {
    /// Signature hexadécimale de `request` à l'instant `amz_date`
    /// (`AAAAMMJJTHHMMSSZ`).
    pub fn signature(&self, request: &Request<'_>, amz_date: &str) -> (String, String) {
        let date = &amz_date[..amz_date.len().min(8)];
        let scope = credential_scope(date, self.region, self.service);
        let (canonical, signed) = canonical_request(request);
        let to_sign = string_to_sign(amz_date, &scope, &canonical);
        let key = signing_key(self.secret_key, date, self.region, self.service);
        (hex::encode(hmac(&key, to_sign.as_bytes())), signed)
    }

    /// Valeur de l'en-tête `Authorization`.
    pub fn authorization(&self, request: &Request<'_>, amz_date: &str) -> String {
        let date = &amz_date[..amz_date.len().min(8)];
        let scope = credential_scope(date, self.region, self.service);
        let (signature, signed) = self.signature(request, amz_date);
        format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            self.access_key
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Suite de signature d'AWS (aws-c-auth, `v4/…`) : identifiants, région,
    // service et instant communs à tous ses cas.
    const SUITE_KEY: &str = "AKIDEXAMPLE";
    const SUITE_SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    const SUITE_DATE: &str = "20150830T123600Z";

    /// Empreinte de la requête canonique de `post-header-key-sort`.
    const POST_HEADER_KEY_SORT_HASH: &str =
        "9368318c2967cf6de74404b30c65a91e8f6253e0a8659d6d5319f1a812f87d65";

    fn suite_signer() -> Signer<'static> {
        Signer {
            access_key: SUITE_KEY,
            secret_key: SUITE_SECRET,
            region: "us-east-1",
            service: "service",
        }
    }

    fn headers(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn get_vanilla() {
        let headers = headers(&[("Host", "example.amazonaws.com"), ("X-Amz-Date", SUITE_DATE)]);
        let request = Request {
            method: "GET",
            canonical_uri: "/",
            query: &[],
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        let (canonical, signed) = canonical_request(&request);
        assert_eq!(
            canonical,
            "GET\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\n\
             host;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(signed, "host;x-amz-date");
        let scope = credential_scope("20150830", "us-east-1", "service");
        assert_eq!(
            string_to_sign(SUITE_DATE, &scope, &canonical),
            "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\n\
             bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63"
        );
        assert_eq!(
            suite_signer().authorization(&request, SUITE_DATE),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn get_vanilla_query_order_key_case() {
        let headers = headers(&[("Host", "example.amazonaws.com"), ("X-Amz-Date", SUITE_DATE)]);
        let query = vec![
            ("Param2".to_string(), "value2".to_string()),
            ("Param1".to_string(), "value1".to_string()),
        ];
        let request = Request {
            method: "GET",
            canonical_uri: "/",
            query: &query,
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        let (canonical, _) = canonical_request(&request);
        assert!(canonical.starts_with("GET\n/\nParam1=value1&Param2=value2\n"), "{canonical}");
        assert_eq!(
            suite_signer().signature(&request, SUITE_DATE).0,
            "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
        );
    }

    #[test]
    fn get_vanilla_query_unreserved() {
        let unreserved = "-._~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let headers = headers(&[("Host", "example.amazonaws.com"), ("X-Amz-Date", SUITE_DATE)]);
        let query = vec![(unreserved.to_string(), unreserved.to_string())];
        let request = Request {
            method: "GET",
            canonical_uri: "/",
            query: &query,
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        assert_eq!(
            suite_signer().signature(&request, SUITE_DATE).0,
            "9c3e54bfcdf0b19771a7f523ee5669cdf59bc7cc0884027167c21bb143a40197"
        );
    }

    #[test]
    fn post_header_key_sort() {
        let headers = headers(&[
            ("Host", "example.amazonaws.com"),
            ("My-Header1", "value1"),
            ("X-Amz-Date", SUITE_DATE),
        ]);
        let request = Request {
            method: "POST",
            canonical_uri: "/",
            query: &[],
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        let (canonical, signed) = canonical_request(&request);
        assert_eq!(signed, "host;my-header1;x-amz-date");
        let scope = credential_scope("20150830", "us-east-1", "service");
        let to_sign = string_to_sign(SUITE_DATE, &scope, &canonical);
        assert!(to_sign.ends_with(POST_HEADER_KEY_SORT_HASH), "{to_sign}");
        assert_eq!(
            suite_signer().signature(&request, SUITE_DATE).0,
            "c5410059b04c1ee005303aed430f6e6645f61f4dc9e1461ec8f8916fdf18852c"
        );
    }

    #[test]
    fn signing_key_of_the_published_example() {
        let key = signing_key(SUITE_SECRET, "20150830", "us-east-1", "iam");
        assert_eq!(
            hex::encode(key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    /// L'exemple « GET Object » de la documentation S3 (en-tête `Range`,
    /// empreinte du corps signée).
    #[test]
    fn s3_get_object_example() {
        let headers = headers(&[
            ("host", "examplebucket.s3.amazonaws.com"),
            ("range", "bytes=0-9"),
            ("x-amz-content-sha256", EMPTY_SHA256),
            ("x-amz-date", "20130524T000000Z"),
        ]);
        let request = Request {
            method: "GET",
            canonical_uri: "/test.txt",
            query: &[],
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        let (canonical, signed) = canonical_request(&request);
        assert_eq!(signed, "host;range;x-amz-content-sha256;x-amz-date");
        let scope = credential_scope("20130524", "us-east-1", "s3");
        assert_eq!(
            string_to_sign("20130524T000000Z", &scope, &canonical),
            "AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n\
             7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972"
        );
        let signer = Signer {
            access_key: "AKIAIOSFODNN7EXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "s3",
        };
        assert_eq!(
            signer.signature(&request, "20130524T000000Z").0,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn the_encoding_keeps_only_unreserved_characters() {
        assert_eq!(uri_encode("a b/c+d=é", true), "a%20b%2Fc%2Bd%3D%C3%A9");
        assert_eq!(uri_encode("dir/dumbmonit-1.db", false), "dir/dumbmonit-1.db");
        assert_eq!(uri_encode("-._~", true), "-._~");
    }

    #[test]
    fn header_values_are_trimmed_and_their_spaces_collapsed() {
        let headers = headers(&[("Host", "h"), ("X-A", "  a   b  "), ("x-a", "c")]);
        let request = Request {
            method: "GET",
            canonical_uri: "/",
            query: &[],
            headers: &headers,
            payload_hash: EMPTY_SHA256,
        };
        let (canonical, signed) = canonical_request(&request);
        assert!(canonical.contains("\nx-a:a b,c\n"), "{canonical}");
        assert_eq!(signed, "host;x-a");
    }
}
