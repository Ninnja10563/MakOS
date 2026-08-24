#![no_std]

//! Bounded, versioned account database with salted PBKDF2 password records.
//! No API stores or returns plaintext passwords.

use makos_crypto::{Sha256, pbkdf2_hmac_sha256_32, sha256};

pub const USERNAME_BYTES: usize = 31;
pub const SALT_BYTES: usize = 16;
pub const HASH_BYTES: usize = 32;
pub const PASSWORD_BYTES: usize = 64;
pub const PASSWORD_ITERATIONS: u32 = 100_000;
pub const FIRST_DYNAMIC_ID: u32 = 2_000;

const MAGIC: [u8; 8] = *b"MKACCT01";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 24;
const RECORD_BYTES: usize = 96;
const TAG_BYTES: usize = 32;
const SALT_DOMAIN: &[u8] = b"MakOS account salt v1\0";
const DUMMY_SALT: [u8; SALT_BYTES] = *b"MakOS-no-account";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountRecord {
    username: [u8; USERNAME_BYTES],
    username_length: u8,
    uid: u32,
    gid: u32,
    iterations: u32,
    salt_length: u8,
    salt: [u8; SALT_BYTES],
    password_hash: [u8; HASH_BYTES],
}

impl AccountRecord {
    const EMPTY: Self = Self {
        username: [0; USERNAME_BYTES],
        username_length: 0,
        uid: 0,
        gid: 0,
        iterations: 0,
        salt_length: 0,
        salt: [0; SALT_BYTES],
        password_hash: [0; HASH_BYTES],
    };

    pub fn from_hash(
        username: &[u8],
        uid: u32,
        gid: u32,
        iterations: u32,
        salt: &[u8],
        password_hash: [u8; HASH_BYTES],
    ) -> Result<Self, RecordError> {
        if !valid_username(username) {
            return Err(RecordError::InvalidUsername);
        }
        if uid == 0 || gid == 0 || iterations == 0 || salt.is_empty() || salt.len() > SALT_BYTES {
            return Err(RecordError::InvalidParameters);
        }
        let mut record = Self::EMPTY;
        record.username[..username.len()].copy_from_slice(username);
        record.username_length = username.len() as u8;
        record.uid = uid;
        record.gid = gid;
        record.iterations = iterations;
        record.salt_length = salt.len() as u8;
        record.salt[..salt.len()].copy_from_slice(salt);
        record.password_hash = password_hash;
        Ok(record)
    }

    pub fn username(&self) -> &[u8] {
        &self.username[..usize::from(self.username_length)]
    }

    pub const fn uid(&self) -> u32 {
        self.uid
    }

    pub const fn gid(&self) -> u32 {
        self.gid
    }

    pub const fn iterations(&self) -> u32 {
        self.iterations
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedAccount {
    username: [u8; USERNAME_BYTES],
    username_length: u8,
    pub uid: u32,
    pub gid: u32,
}

impl AuthenticatedAccount {
    pub fn username(&self) -> &[u8] {
        &self.username[..usize::from(self.username_length)]
    }
}

impl From<AccountRecord> for AuthenticatedAccount {
    fn from(record: AccountRecord) -> Self {
        Self {
            username: record.username,
            username_length: record.username_length,
            uid: record.uid,
            gid: record.gid,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordError {
    InvalidUsername,
    InvalidParameters,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddError {
    InvalidUsername,
    InvalidPassword,
    AlreadyExists,
    Full,
    IdExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    BufferSize,
    Magic,
    Version,
    Capacity,
    Integrity,
    InvalidRecord,
    Duplicate,
    InvalidNextId,
}

#[derive(Clone)]
pub struct AccountDb<const N: usize> {
    records: [AccountRecord; N],
    count: usize,
    next_id: u32,
}

impl<const N: usize> AccountDb<N> {
    pub fn with_initial(initial: AccountRecord) -> Self {
        let mut records = [AccountRecord::EMPTY; N];
        let count = if N == 0 {
            0
        } else {
            records[0] = initial;
            1
        };
        Self {
            records,
            count,
            next_id: FIRST_DYNAMIC_ID.max(initial.uid.saturating_add(1)),
        }
    }

    pub const fn encoded_len() -> usize {
        HEADER_BYTES + N * RECORD_BYTES + TAG_BYTES
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn get(&self, index: usize) -> Option<&AccountRecord> {
        (index < self.count).then(|| &self.records[index])
    }

    pub fn find(&self, username: &[u8]) -> Option<&AccountRecord> {
        self.records[..self.count]
            .iter()
            .find(|record| record.username() == username)
    }

    pub fn add_user(
        &mut self,
        username: &[u8],
        password: &[u8],
        entropy: &[u8],
    ) -> Result<&AccountRecord, AddError> {
        if !valid_username(username) {
            return Err(AddError::InvalidUsername);
        }
        if !(8..=PASSWORD_BYTES).contains(&password.len()) {
            return Err(AddError::InvalidPassword);
        }
        if self.find(username).is_some() {
            return Err(AddError::AlreadyExists);
        }
        if self.count == N {
            return Err(AddError::Full);
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(AddError::IdExhausted)?;
        let salt = derive_salt(username, id, entropy);
        let hash = pbkdf2_hmac_sha256_32(password, &salt, PASSWORD_ITERATIONS);
        let record = AccountRecord::from_hash(username, id, id, PASSWORD_ITERATIONS, &salt, hash)
            .map_err(|_| AddError::InvalidUsername)?;
        self.records[self.count] = record;
        self.count += 1;
        Ok(&self.records[self.count - 1])
    }

    /// Always performs one PBKDF2 computation for bounded input, including an
    /// unknown username, reducing user-enumeration timing differences.
    pub fn authenticate(&self, username: &[u8], password: &[u8]) -> Option<AuthenticatedAccount> {
        if password.len() > PASSWORD_BYTES {
            return None;
        }
        let record = self.find(username);
        let salt: &[u8] = record.map_or(&DUMMY_SALT, |value| {
            &value.salt[..usize::from(value.salt_length)]
        });
        let iterations = record.map_or(PASSWORD_ITERATIONS, |value| value.iterations);
        let candidate = pbkdf2_hmac_sha256_32(password, salt, iterations);
        let expected = record.map_or([0; HASH_BYTES], |value| value.password_hash);
        if record.is_none() || !constant_time_equal(&candidate, &expected) {
            return None;
        }
        let record = record?;
        Some(AuthenticatedAccount {
            username: record.username,
            username_length: record.username_length,
            uid: record.uid,
            gid: record.gid,
        })
    }

    pub fn encode(&self, output: &mut [u8]) -> Result<usize, CodecError> {
        let length = Self::encoded_len();
        if output.len() < length {
            return Err(CodecError::BufferSize);
        }
        output[..length].fill(0);
        output[..8].copy_from_slice(&MAGIC);
        output[8..12].copy_from_slice(&VERSION.to_le_bytes());
        output[12..16].copy_from_slice(&(N as u32).to_le_bytes());
        output[16..20].copy_from_slice(&(self.count as u32).to_le_bytes());
        output[20..24].copy_from_slice(&self.next_id.to_le_bytes());
        for (index, record) in self.records[..self.count].iter().enumerate() {
            let start = HEADER_BYTES + index * RECORD_BYTES;
            output[start] = record.username_length;
            output[start + 1] = record.salt_length;
            output[start + 4..start + 4 + USERNAME_BYTES].copy_from_slice(&record.username);
            output[start + 36..start + 40].copy_from_slice(&record.uid.to_le_bytes());
            output[start + 40..start + 44].copy_from_slice(&record.gid.to_le_bytes());
            output[start + 44..start + 48].copy_from_slice(&record.iterations.to_le_bytes());
            output[start + 48..start + 64].copy_from_slice(&record.salt);
            output[start + 64..start + 96].copy_from_slice(&record.password_hash);
        }
        let tag_start = length - TAG_BYTES;
        let tag = sha256(&output[..tag_start]);
        output[tag_start..length].copy_from_slice(&tag);
        Ok(length)
    }

    pub fn decode(input: &[u8]) -> Result<Self, CodecError> {
        let length = Self::encoded_len();
        if input.len() != length {
            return Err(CodecError::BufferSize);
        }
        if input[..8] != MAGIC {
            return Err(CodecError::Magic);
        }
        if read_u32(input, 8) != VERSION {
            return Err(CodecError::Version);
        }
        if read_u32(input, 12) as usize != N {
            return Err(CodecError::Capacity);
        }
        let tag_start = length - TAG_BYTES;
        if !constant_time_equal(&sha256(&input[..tag_start]), &input[tag_start..]) {
            return Err(CodecError::Integrity);
        }
        let count = read_u32(input, 16) as usize;
        if count > N {
            return Err(CodecError::InvalidRecord);
        }
        let next_id = read_u32(input, 20);
        let mut records = [AccountRecord::EMPTY; N];
        let mut maximum_id = 0u32;
        for index in 0..count {
            let start = HEADER_BYTES + index * RECORD_BYTES;
            let name_length = usize::from(input[start]);
            let salt_length = usize::from(input[start + 1]);
            if name_length == 0 || name_length > USERNAME_BYTES {
                return Err(CodecError::InvalidRecord);
            }
            let username = &input[start + 4..start + 4 + name_length];
            let uid = read_u32(input, start + 36);
            let gid = read_u32(input, start + 40);
            let iterations = read_u32(input, start + 44);
            let mut salt = [0; SALT_BYTES];
            salt.copy_from_slice(&input[start + 48..start + 64]);
            let mut hash = [0; HASH_BYTES];
            hash.copy_from_slice(&input[start + 64..start + 96]);
            if salt_length == 0 || salt_length > SALT_BYTES {
                return Err(CodecError::InvalidRecord);
            }
            let record = AccountRecord::from_hash(
                username,
                uid,
                gid,
                iterations,
                &salt[..salt_length],
                hash,
            )
            .map_err(|_| CodecError::InvalidRecord)?;
            if records[..index].iter().any(|prior| {
                prior.username() == record.username()
                    || prior.uid == record.uid
                    || prior.gid == record.gid
            }) {
                return Err(CodecError::Duplicate);
            }
            maximum_id = maximum_id.max(uid).max(gid);
            records[index] = record;
        }
        if next_id < FIRST_DYNAMIC_ID || next_id <= maximum_id {
            return Err(CodecError::InvalidNextId);
        }
        Ok(Self {
            records,
            count,
            next_id,
        })
    }
}

pub fn valid_username(username: &[u8]) -> bool {
    !username.is_empty()
        && username.len() <= USERNAME_BYTES
        && username[0].is_ascii_lowercase()
        && username.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(*byte, b'_' | b'-')
        })
}

fn derive_salt(username: &[u8], id: u32, entropy: &[u8]) -> [u8; SALT_BYTES] {
    let mut digest = Sha256::new();
    digest.update(SALT_DOMAIN);
    digest.update(username);
    digest.update(&id.to_le_bytes());
    digest.update(entropy);
    let digest = digest.finish();
    let mut salt = [0; SALT_BYTES];
    salt.copy_from_slice(&digest[..SALT_BYTES]);
    salt
}

fn read_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(input[offset..offset + 4].try_into().unwrap())
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (&left, &right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    const LEGACY_SALT: &[u8] = b"MakOS-v1-user:";
    const LEGACY_HASH: [u8; 32] = [
        0x96, 0x78, 0xbc, 0x48, 0x44, 0xda, 0x75, 0x09, 0xff, 0xc3, 0x43, 0x0e, 0x8a, 0x56, 0x7c,
        0x21, 0xbb, 0xea, 0xc4, 0x92, 0xe6, 0xa2, 0x31, 0xdd, 0xec, 0xe5, 0x8a, 0x73, 0xb3, 0x7f,
        0x68, 0x4b,
    ];

    fn initial() -> AccountRecord {
        AccountRecord::from_hash(
            b"marcus",
            1000,
            1000,
            PASSWORD_ITERATIONS,
            LEGACY_SALT,
            LEGACY_HASH,
        )
        .unwrap()
    }

    #[test]
    fn username_policy_is_deterministic() {
        for valid in [b"a" as &[u8], b"marcus", b"user-2", b"dev_ops"] {
            assert!(valid_username(valid));
        }
        for invalid in [
            b"" as &[u8],
            b"2user",
            b"Upper",
            b"has space",
            b"dot.name",
            b"abcdefghijklmnopqrstuvwxyzabcdef",
        ] {
            assert!(!valid_username(invalid));
        }
    }

    #[test]
    fn legacy_account_authenticates_without_plaintext_storage() {
        let db = AccountDb::<4>::with_initial(initial());
        let account = db.authenticate(b"marcus", b"makos").unwrap();
        assert_eq!(account.username(), b"marcus");
        assert_eq!((account.uid, account.gid), (1000, 1000));
        assert!(db.authenticate(b"marcus", b"wrong").is_none());
        assert!(db.authenticate(b"unknown", b"makos").is_none());
    }

    #[test]
    fn add_user_assigns_unique_ids_and_salts() {
        let mut db = AccountDb::<4>::with_initial(initial());
        let first = *db
            .add_user(b"alice", b"correct horse", b"fixed-test-entropy")
            .unwrap();
        let second = *db
            .add_user(b"bob", b"battery staple", b"fixed-test-entropy")
            .unwrap();
        assert_eq!((first.uid(), first.gid()), (2000, 2000));
        assert_eq!((second.uid(), second.gid()), (2001, 2001));
        assert_ne!(first.salt, second.salt);
        assert!(db.authenticate(b"alice", b"correct horse").is_some());
        assert!(db.authenticate(b"bob", b"battery staple").is_some());
    }

    #[test]
    fn rejects_duplicate_invalid_and_weak_users() {
        let mut db = AccountDb::<2>::with_initial(initial());
        assert_eq!(
            db.add_user(b"Bad", b"long enough", b"x"),
            Err(AddError::InvalidUsername)
        );
        assert_eq!(
            db.add_user(b"alice", b"short", b"x"),
            Err(AddError::InvalidPassword)
        );
        db.add_user(b"alice", b"long enough", b"x").unwrap();
        assert_eq!(
            db.add_user(b"alice", b"another password", b"x"),
            Err(AddError::AlreadyExists)
        );
        assert_eq!(
            db.add_user(b"bob", b"another password", b"x"),
            Err(AddError::Full)
        );
    }

    #[test]
    fn codec_round_trip_is_exact_and_integrity_checked() {
        let mut db = AccountDb::<4>::with_initial(initial());
        db.add_user(b"alice", b"correct horse", b"deterministic")
            .unwrap();
        let mut first = [0u8; AccountDb::<4>::encoded_len()];
        let mut second = [0u8; AccountDb::<4>::encoded_len()];
        assert_eq!(db.encode(&mut first), Ok(first.len()));
        assert_eq!(db.encode(&mut second), Ok(second.len()));
        assert_eq!(first, second);
        assert!(
            !first
                .windows(b"correct horse".len())
                .any(|window| window == b"correct horse")
        );
        let restored = AccountDb::<4>::decode(&first).unwrap();
        assert_eq!(restored.len(), 2);
        assert!(restored.authenticate(b"alice", b"correct horse").is_some());
        first[50] ^= 1;
        assert!(matches!(
            AccountDb::<4>::decode(&first),
            Err(CodecError::Integrity)
        ));
    }
}
