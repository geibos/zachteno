use std::collections::BTreeMap;

use super::{Hasher, StoredHash, Verification, WeakPassword, check_strength};
use crate::secret::Secret;

fn hasher(versions: &[(u32, u8)]) -> Hasher {
    let peppers: BTreeMap<u32, Vec<u8>> =
        versions.iter().map(|(version, byte)| (*version, vec![*byte; 32])).collect();
    Hasher::new(Secret::new(peppers), 2)
}

#[test]
fn weak_passwords_are_rejected_with_reason() {
    assert_eq!(check_strength("коротко"), Err(WeakPassword::TooShort), "11 и меньше символов");
    assert_eq!(check_strength("QwertyUIOP123"), Err(WeakPassword::Common), "есть в списке");
    assert_eq!(check_strength(&"я".repeat(129)), Err(WeakPassword::TooLong), "длиннее 128");
    assert_eq!(check_strength("корова летит на север 7"), Ok(()), "без требований к составу");
}

#[tokio::test]
async fn hash_uses_argon2id_with_required_params() {
    let stored = hasher(&[(1, 7)]).hash("достаточно длинный пароль".to_owned()).await.unwrap();
    assert!(stored.phc.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"), "формат PHC: {}", stored.phc);
    assert_eq!(stored.pepper_version, 1, "активный перец");
}

#[tokio::test]
async fn verify_accepts_right_and_rejects_wrong_password() {
    let hasher = hasher(&[(1, 7)]);
    let stored = hasher.hash("достаточно длинный пароль".to_owned()).await.unwrap();
    let right = hasher.verify("достаточно длинный пароль".to_owned(), stored.clone()).await.unwrap();
    let wrong = hasher.verify("другой длинный пароль".to_owned(), stored).await.unwrap();
    assert_eq!(right, Verification::Valid, "верный пароль");
    assert_eq!(wrong, Verification::Invalid, "неверный пароль");
}

#[tokio::test]
async fn hash_without_right_pepper_does_not_verify() {
    let stored = hasher(&[(1, 7)]).hash("достаточно длинный пароль".to_owned()).await.unwrap();
    let other = hasher(&[(1, 9)]).verify("достаточно длинный пароль".to_owned(), stored).await.unwrap();
    assert_eq!(other, Verification::Invalid, "утечка базы без перца бесполезна");
}

#[tokio::test]
async fn old_pepper_version_requires_rehash() {
    let stored = hasher(&[(1, 7)]).hash("достаточно длинный пароль".to_owned()).await.unwrap();
    let rotated = hasher(&[(1, 7), (2, 8)]);
    let outcome = rotated.verify("достаточно длинный пароль".to_owned(), stored).await.unwrap();
    assert_eq!(outcome, Verification::ValidNeedsRehash, "старый перец");
}

#[tokio::test]
async fn weaker_params_require_rehash() {
    let hasher = hasher(&[(1, 7)]);
    let weak = weak_hash(&[7; 32]);
    let outcome = hasher.verify("достаточно длинный пароль".to_owned(), weak).await.unwrap();
    assert_eq!(outcome, Verification::ValidNeedsRehash, "параметры слабее текущих");
}

fn weak_hash(pepper: &[u8]) -> StoredHash {
    use argon2::password_hash::PasswordHasher as _;
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(19 * 1024, 2, 1, None).unwrap();
    let argon = Argon2::new_with_secret(pepper, Algorithm::Argon2id, Version::V0x13, params).unwrap();
    let phc = argon.hash_password("достаточно длинный пароль".as_bytes()).unwrap().to_string();
    StoredHash { phc, pepper_version: 1 }
}
