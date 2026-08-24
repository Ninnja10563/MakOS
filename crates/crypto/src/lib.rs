#![no_std]

const SHA256_INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_length: usize,
    byte_length: u64,
}

impl Sha256 {
    pub const fn new() -> Self {
        Self {
            state: SHA256_INITIAL,
            buffer: [0; 64],
            buffer_length: 0,
            byte_length: 0,
        }
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.byte_length = self.byte_length.wrapping_add(bytes.len() as u64);
        if self.buffer_length != 0 {
            let count = (64 - self.buffer_length).min(bytes.len());
            self.buffer[self.buffer_length..self.buffer_length + count]
                .copy_from_slice(&bytes[..count]);
            self.buffer_length += count;
            bytes = &bytes[count..];
            if self.buffer_length < 64 {
                return;
            }
            compress(&mut self.state, &self.buffer);
            self.buffer_length = 0;
        }
        while bytes.len() >= 64 {
            compress(&mut self.state, &bytes[..64]);
            bytes = &bytes[64..];
        }
        self.buffer[..bytes.len()].copy_from_slice(bytes);
        self.buffer_length = bytes.len();
    }

    pub fn finish(mut self) -> [u8; 32] {
        let mut tail = [0u8; 128];
        tail[..self.buffer_length].copy_from_slice(&self.buffer[..self.buffer_length]);
        tail[self.buffer_length] = 0x80;
        let tail_length = if self.buffer_length < 56 { 64 } else { 128 };
        tail[tail_length - 8..tail_length]
            .copy_from_slice(&self.byte_length.wrapping_mul(8).to_be_bytes());
        for chunk in tail[..tail_length].chunks_exact(64) {
            compress(&mut self.state, chunk);
        }
        let mut output = [0u8; 32];
        for (index, value) in self.state.iter().enumerate() {
            output[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        output
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finish()
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    hmac_sha256_parts(key, message, &[])
}

fn hmac_sha256_parts(key: &[u8], first: &[u8], second: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > block.len() {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = block;
    let mut outer_pad = block;
    for index in 0..64 {
        inner_pad[index] ^= 0x36;
        outer_pad[index] ^= 0x5c;
    }
    let mut inner = Sha256::new();
    inner.update(&inner_pad);
    inner.update(first);
    inner.update(second);
    let inner_digest = inner.finish();
    let mut outer = Sha256::new();
    outer.update(&outer_pad);
    outer.update(&inner_digest);
    outer.finish()
}

pub fn pbkdf2_hmac_sha256_32(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    if iterations == 0 {
        return [0; 32];
    }
    let mut value = hmac_sha256_parts(password, salt, &1u32.to_be_bytes());
    let mut output = value;
    for _ in 1..iterations {
        value = hmac_sha256(password, &value);
        for index in 0..32 {
            output[index] ^= value[index];
        }
    }
    output
}

pub const fn decode_hex_256(input: &[u8; 512]) -> [u8; 256] {
    const fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("invalid hexadecimal digit"),
        }
    }
    let mut output = [0u8; 256];
    let mut index = 0;
    while index < 256 {
        output[index] = (nibble(input[index * 2]) << 4) | nibble(input[index * 2 + 1]);
        index += 1;
    }
    output
}

fn compress(state: &mut [u32; 8], block: &[u8]) {
    debug_assert_eq!(block.len(), 64);
    let mut words = [0u32; 64];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let big1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(big1)
            .wrapping_add(choose)
            .wrapping_add(SHA256_K[index])
            .wrapping_add(words[index]);
        let big0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = big0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct U2048([u64; 32]);

impl U2048 {
    fn from_be(bytes: &[u8; 256]) -> Self {
        let mut limbs = [0u64; 32];
        for (index, limb) in limbs.iter_mut().enumerate() {
            let offset = (31 - index) * 8;
            *limb = u64::from_be_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
        }
        Self(limbs)
    }

    fn write_be(self, output: &mut [u8; 256]) {
        for (index, limb) in self.0.iter().enumerate() {
            let offset = (31 - index) * 8;
            output[offset..offset + 8].copy_from_slice(&limb.to_be_bytes());
        }
    }

    fn compare(self, other: Self) -> core::cmp::Ordering {
        for index in (0..32).rev() {
            match self.0[index].cmp(&other.0[index]) {
                core::cmp::Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        core::cmp::Ordering::Equal
    }

    fn subtract(self, other: Self) -> Self {
        let mut output = [0u64; 32];
        let mut borrow = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (value, first_borrow) = self.0[index].overflowing_sub(other.0[index]);
            let (value, second_borrow) = value.overflowing_sub(u64::from(borrow));
            *slot = value;
            borrow = first_borrow || second_borrow;
        }
        debug_assert!(!borrow);
        Self(output)
    }

    fn add(self, other: Self) -> Self {
        let mut output = [0u64; 32];
        let mut carry = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (value, first_carry) = self.0[index].overflowing_add(other.0[index]);
            let (value, second_carry) = value.overflowing_add(u64::from(carry));
            *slot = value;
            carry = first_carry || second_carry;
        }
        debug_assert!(!carry);
        Self(output)
    }

    fn add_mod(self, other: Self, modulus: Self) -> Self {
        let gap = modulus.subtract(other);
        if self.compare(gap) != core::cmp::Ordering::Less {
            self.subtract(gap)
        } else {
            self.add(other)
        }
    }

    fn bit(self, index: usize) -> bool {
        self.0[index / 64] & (1u64 << (index % 64)) != 0
    }
}

fn multiply_mod(left: U2048, right: U2048, modulus: U2048) -> U2048 {
    let mut result = U2048([0; 32]);
    let mut addend = left;
    for bit in 0..2048 {
        if right.bit(bit) {
            result = result.add_mod(addend, modulus);
        }
        if bit != 2047 {
            addend = addend.add_mod(addend, modulus);
        }
    }
    result
}

/// Verifies RSA-2048 PKCS#1 v1.5 SHA-256 with fixed public exponent 65537.
pub fn rsa2048_sha256_verify(modulus: &[u8; 256], signature: &[u8; 256], message: &[u8]) -> bool {
    let modulus = U2048::from_be(modulus);
    let signature = U2048::from_be(signature);
    if signature.compare(modulus) != core::cmp::Ordering::Less {
        return false;
    }

    let original = signature;
    let mut decoded = signature;
    for _ in 0..16 {
        decoded = multiply_mod(decoded, decoded, modulus);
    }
    decoded = multiply_mod(decoded, original, modulus);

    let mut encoded = [0u8; 256];
    decoded.write_be(&mut encoded);
    const DIGEST_INFO: [u8; 19] = [
        0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01,
        0x05, 0x00, 0x04, 0x20,
    ];
    let digest = sha256(message);
    let separator = 256 - DIGEST_INFO.len() - digest.len() - 1;
    if encoded[0] != 0 || encoded[1] != 1 || encoded[separator] != 0 {
        return false;
    }
    if encoded[2..separator].iter().any(|byte| *byte != 0xff) {
        return false;
    }
    encoded[separator + 1..separator + 1 + DIGEST_INFO.len()] == DIGEST_INFO
        && encoded[separator + 1 + DIGEST_INFO.len()..] == digest
}

#[cfg(test)]
mod tests {
    use super::{hmac_sha256, pbkdf2_hmac_sha256_32, sha256};

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256(b""),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
        assert_eq!(
            sha256(b"abc"),
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    #[test]
    fn sha256_multi_block() {
        assert_eq!(
            sha256(&[b'a'; 200]),
            [
                0xc2, 0xa9, 0x08, 0xd9, 0x8f, 0x5d, 0xf9, 0x87, 0xad, 0xe4, 0x1b, 0x5f, 0xce, 0x21,
                0x30, 0x67, 0xef, 0xbc, 0xc2, 0x1e, 0xf2, 0x24, 0x02, 0x12, 0xa4, 0x1e, 0x54, 0xb5,
                0xe7, 0xc2, 0x8a, 0xe5,
            ]
        );
    }

    #[test]
    fn hmac_sha256_known_vector() {
        assert_eq!(
            hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog"),
            [
                0xf7, 0xbc, 0x83, 0xf4, 0x30, 0x53, 0x84, 0x24, 0xb1, 0x32, 0x98, 0xe6, 0xaa, 0x6f,
                0xb1, 0x43, 0xef, 0x4d, 0x59, 0xa1, 0x49, 0x46, 0x17, 0x59, 0x97, 0x47, 0x9d, 0xbc,
                0x2d, 0x1a, 0x3c, 0xd8,
            ]
        );
    }

    #[test]
    fn pbkdf2_hmac_sha256_known_vectors() {
        assert_eq!(
            pbkdf2_hmac_sha256_32(b"password", b"salt", 1),
            [
                0x12, 0x0f, 0xb6, 0xcf, 0xfc, 0xf8, 0xb3, 0x2c, 0x43, 0xe7, 0x22, 0x52, 0x56, 0xc4,
                0xf8, 0x37, 0xa8, 0x65, 0x48, 0xc9, 0x2c, 0xcc, 0x35, 0x48, 0x08, 0x05, 0x98, 0x7c,
                0xb7, 0x0b, 0xe1, 0x7b,
            ]
        );
        assert_eq!(
            pbkdf2_hmac_sha256_32(b"password", b"salt", 2),
            [
                0xae, 0x4d, 0x0c, 0x95, 0xaf, 0x6b, 0x46, 0xd3, 0x2d, 0x0a, 0xdf, 0xf9, 0x28, 0xf0,
                0x6d, 0xd0, 0x2a, 0x30, 0x3f, 0x8e, 0xf3, 0xc2, 0x51, 0xdf, 0xd6, 0xe2, 0xd8, 0x5a,
                0x95, 0x47, 0x4c, 0x43,
            ]
        );
    }

    #[test]
    fn rsa2048_pkcs1_sha256_signature() {
        const MODULUS: [u8; 256] = super::decode_hex_256(
            b"ba8d9d8181585920c54a3f1440aab2be7523de28bc6076312b5d1a81e7ed6a902387913a22b22dcfa940028aca21fe7642dd9be867eb13073aa4c5a7c224599079790b5cb26f3d30b78f03f5c89bbf8457c110e67a35396d729d733df0999e99977d6724dfad8fb5001210246fdad52f1c144e6bbfac86a27dac4212b5ac0726a4c51e465b42a29609a40c4d486be2ef1ba19e5d735230c9da8b97fe1b28362e064bc18a1b9d91f346f590eec0525733123a743a9751b87f407f73bea5d90c9dd03b5ce0ee61ecf4b048bc4f2f0c09c9e32147be65fa9b8bd54363c6ab019c1974fcb20d5f2ad1d8a074d57f4b7f79d942359211adc88c72a2a088f87828716d",
        );
        const SIGNATURE: [u8; 256] = super::decode_hex_256(
            b"2a2de48d8a2d62f90f3a3c503666ddb7796e5d504291d30a2403a953c9d5fc004f295ae5ea065a13cc020d28fbb187912fa8c5b391ed89c4088611c459d50154c2f308d78752cb3729f00ce2b4d721b4e9f5d05238f548db6f57a53fdf2a8f3695fffedc5044090e9932b2e72f5bb4f8649f9e905b1fe252082da92704f0fc4739fc26d4e7f9169f8590630bf83fd84e02979599b908ca4833057e1e197342481e858a7a27eb679397eba8cd06a7dce02db9d06fdee8c28227fbd78d6781b844b4ac9bb5d02282d05b8336520c6b59be2e4856d9db78028431972da88d4e8fe9bf646ae424f5dd0540e0cefa0ccaa364c6b1999c0d885f182eca99811b10e21d",
        );
        let message = b"MAKPKG1\0\x05hello\x031.0\x08\0hello-v1\x04libc";
        assert!(super::rsa2048_sha256_verify(&MODULUS, &SIGNATURE, message));
        let mut tampered = *message;
        tampered[18] ^= 1;
        assert!(!super::rsa2048_sha256_verify(
            &MODULUS, &SIGNATURE, &tampered
        ));
    }
}
