include!("sm4_consts.rs");

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(include_str!("../tongsuo/sm4-armv8.S"));

use cipher::{
    AlgorithmName, Block, BlockCipherDecBackend, BlockCipherDecClosure, BlockCipherDecrypt,
    BlockCipherEncBackend, BlockCipherEncClosure, BlockCipherEncrypt, BlockSizeUser, InOut, Key,
    KeyInit, KeySizeUser, ParBlocksSizeUser,
    consts::{U1, U16},
};
use core::fmt;

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn sm4_v8_set_encrypt_key(key: *const u8, rk: *mut u32);
    fn sm4_v8_set_decrypt_key(key: *const u8, rk: *mut u32);
    fn sm4_v8_encrypt(input: *const u8, output: *mut u8, rk: *const u32);
    fn sm4_v8_decrypt(input: *const u8, output: *mut u8, rk: *const u32);
}

#[derive(Clone)]
pub struct Sm4 {
    rk: [u32; 32],
    rkd: [u32; 32],
}

impl KeySizeUser for Sm4 {
    type KeySize = U16;
}

impl KeyInit for Sm4 {
    fn new(key: &Key<Self>) -> Self {
        let mut out = Sm4 {
            rk: [0u32; 32],
            rkd: [0u32; 32],
        };

        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("sm4") {
                unsafe {
                    sm4_v8_set_encrypt_key(key.as_ptr(), out.rk.as_mut_ptr());
                    sm4_v8_set_decrypt_key(key.as_ptr(), out.rkd.as_mut_ptr());
                }
                return out;
            }
        }

        let mk = [
            u32::from_be_bytes(key[0..4].try_into().unwrap()),
            u32::from_be_bytes(key[4..8].try_into().unwrap()),
            u32::from_be_bytes(key[8..12].try_into().unwrap()),
            u32::from_be_bytes(key[12..16].try_into().unwrap()),
        ];
        let mut k = [mk[0] ^ FK[0], mk[1] ^ FK[1], mk[2] ^ FK[2], mk[3] ^ FK[3]];

        for i in 0..8 {
            k[0] ^= t_prime(k[1] ^ k[2] ^ k[3] ^ CK[i * 4]);
            k[1] ^= t_prime(k[2] ^ k[3] ^ k[0] ^ CK[i * 4 + 1]);
            k[2] ^= t_prime(k[3] ^ k[0] ^ k[1] ^ CK[i * 4 + 2]);
            k[3] ^= t_prime(k[0] ^ k[1] ^ k[2] ^ CK[i * 4 + 3]);

            out.rk[i * 4] = k[0];
            out.rk[i * 4 + 1] = k[1];
            out.rk[i * 4 + 2] = k[2];
            out.rk[i * 4 + 3] = k[3];
        }
        out.rkd = out.rk;
        out
    }
}

impl BlockSizeUser for Sm4 {
    type BlockSize = U16;
}

impl ParBlocksSizeUser for Sm4 {
    type ParBlocksSize = U1;
}

impl BlockCipherEncrypt for Sm4 {
    #[inline]
    fn encrypt_with_backend(&self, f: impl BlockCipherEncClosure<BlockSize = Self::BlockSize>) {
        f.call(self)
    }
}

impl BlockCipherEncBackend for Sm4 {
    #[inline]
    fn encrypt_block(&self, mut block: InOut<'_, '_, Block<Self>>) {
        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("sm4") {
                let input = block.get_in().as_ptr();
                let output = block.get_out().as_mut_ptr();
                unsafe {
                    sm4_v8_encrypt(input, output, self.rk.as_ptr());
                }
                return;
            }
        }

        let b = block.get_in();
        let mut x = [
            u32::from_be_bytes(b[0..4].try_into().unwrap()),
            u32::from_be_bytes(b[4..8].try_into().unwrap()),
            u32::from_be_bytes(b[8..12].try_into().unwrap()),
            u32::from_be_bytes(b[12..16].try_into().unwrap()),
        ];

        let rk = &self.rk;
        for i in 0..8 {
            x[0] ^= t(x[1] ^ x[2] ^ x[3] ^ rk[i * 4]);
            x[1] ^= t(x[2] ^ x[3] ^ x[0] ^ rk[i * 4 + 1]);
            x[2] ^= t(x[3] ^ x[0] ^ x[1] ^ rk[i * 4 + 2]);
            x[3] ^= t(x[0] ^ x[1] ^ x[2] ^ rk[i * 4 + 3]);
        }

        let block = block.get_out();
        block[0..4].copy_from_slice(&x[3].to_be_bytes());
        block[4..8].copy_from_slice(&x[2].to_be_bytes());
        block[8..12].copy_from_slice(&x[1].to_be_bytes());
        block[12..16].copy_from_slice(&x[0].to_be_bytes());
    }
}

impl BlockCipherDecrypt for Sm4 {
    #[inline]
    fn decrypt_with_backend(&self, f: impl BlockCipherDecClosure<BlockSize = Self::BlockSize>) {
        f.call(self)
    }
}

impl BlockCipherDecBackend for Sm4 {
    #[inline]
    fn decrypt_block(&self, mut block: InOut<'_, '_, Block<Self>>) {
        #[cfg(target_arch = "aarch64")]
        {
            if std::arch::is_aarch64_feature_detected!("sm4") {
                let input = block.get_in().as_ptr();
                let output = block.get_out().as_mut_ptr();
                unsafe {
                    sm4_v8_decrypt(input, output, self.rkd.as_ptr());
                }
                return;
            }
        }

        let b = block.get_in();
        let mut x = [
            u32::from_be_bytes(b[0..4].try_into().unwrap()),
            u32::from_be_bytes(b[4..8].try_into().unwrap()),
            u32::from_be_bytes(b[8..12].try_into().unwrap()),
            u32::from_be_bytes(b[12..16].try_into().unwrap()),
        ];

        let rk = &self.rk;
        for i in 0..8 {
            x[0] ^= t(x[1] ^ x[2] ^ x[3] ^ rk[31 - i * 4]);
            x[1] ^= t(x[2] ^ x[3] ^ x[0] ^ rk[31 - (i * 4 + 1)]);
            x[2] ^= t(x[3] ^ x[0] ^ x[1] ^ rk[31 - (i * 4 + 2)]);
            x[3] ^= t(x[0] ^ x[1] ^ x[2] ^ rk[31 - (i * 4 + 3)]);
        }

        let block = block.get_out();
        block[0..4].copy_from_slice(&x[3].to_be_bytes());
        block[4..8].copy_from_slice(&x[2].to_be_bytes());
        block[8..12].copy_from_slice(&x[1].to_be_bytes());
        block[12..16].copy_from_slice(&x[0].to_be_bytes());
    }
}

impl fmt::Debug for Sm4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sm4 { ... }")
    }
}

impl AlgorithmName for Sm4 {
    fn write_alg_name(f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sm4")
    }
}

impl Drop for Sm4 {
    fn drop(&mut self) {
        let _ = (&self.rk, &self.rkd);
    }
}

#[inline]
fn tau(a: u32) -> u32 {
    let mut buf = a.to_be_bytes();
    buf[0] = SBOX[buf[0] as usize];
    buf[1] = SBOX[buf[1] as usize];
    buf[2] = SBOX[buf[2] as usize];
    buf[3] = SBOX[buf[3] as usize];
    u32::from_be_bytes(buf)
}

#[inline]
fn el(b: u32) -> u32 {
    b ^ b.rotate_left(2) ^ b.rotate_left(10) ^ b.rotate_left(18) ^ b.rotate_left(24)
}

#[inline]
fn el_prime(b: u32) -> u32 {
    b ^ b.rotate_left(13) ^ b.rotate_left(23)
}

#[inline]
fn t(val: u32) -> u32 {
    el(tau(val))
}

#[inline]
fn t_prime(val: u32) -> u32 {
    el_prime(tau(val))
}
