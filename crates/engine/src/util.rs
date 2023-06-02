use std::{
    collections::HashMap,
    hash::{BuildHasher, Hasher},
    mem::transmute,
};

use ethers::types::H160;

#[derive(Eq, PartialEq)]
pub struct AddressForHash([u8; 20]);

impl From<[u8; 20]> for AddressForHash {
    fn from(v: [u8; 20]) -> Self {
        Self(v)
    }
}

impl From<H160> for AddressForHash {
    fn from(v: H160) -> Self {
        Self(v.0)
    }
}

// impl Hash for AddressForHash {
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         state = (unsafe { transmute::<[u8; 8], u64>(*(&self.0[0..8] as *const [u8] as *const [u8; 8])) })
//         ^ (unsafe {
//             transmute::<[u8; 8], u64>(*(&self.0[12..20] as *const [u8] as *const [u8; 8]))
//         })
//     }
// }

/// See-through hasher to the u32 value
/// Used with quick pairing functions
#[derive(Clone, Default)]
pub struct NoopHasherU32 {
    state: u32,
}

impl Hasher for NoopHasherU32 {
    fn write_u32(&mut self, i: u32) {
        self.state = i;
    }
    fn write(&mut self, _bytes: &[u8]) {
        //     self.state = unsafe { transmute(*(bytes as *const [u8] as *const [u8; 4])) };
    }
    fn finish(&self) -> u64 {
        self.state as u64
    }
}

impl BuildHasher for NoopHasherU32 {
    type Hasher = NoopHasherU32;
    fn build_hasher(&self) -> Self::Hasher {
        NoopHasherU32::default()
    }
}

/// See-through hasher for an ethereum address
#[derive(Default)]
pub struct AddressHasher {
    state: u64,
}

impl Hasher for AddressHasher {
    /// hashing the length prefix helps us in no way
    fn write_usize(&mut self, _: usize) {}
    fn write(&mut self, bytes: &[u8]) {
        // intrinsic version
        // #[cfg(target_arch = "x86_64")]
        // {
        //     use core::arch::x86_64::_kxor_mask64;
        //     self.state = unsafe {
        //         _kxor_mask64(
        //              transmute::<[u8; 8], u64>(*(&bytes[0..9] as *const [u8] as *const [u8; 8]) ),
        //              transmute::<[u8; 8], u64>(*(&bytes[12..20] as *const [u8] as *const [u8; 8]) ),
        //         )
        //     };
        // }
        //#[cfg(not(target_arch = "x86_64"))]
        self.state = unsafe {
            transmute::<[u8; 8], u64>(*(bytes.get_unchecked(0..8) as *const [u8] as *const [u8; 8]))
                ^ transmute::<[u8; 8], u64>(
                    *(bytes.get_unchecked(12..20) as *const [u8] as *const [u8; 8]),
                )
        };
    }
    fn finish(&self) -> u64 {
        self.state
    }
}

impl BuildHasher for AddressHasher {
    type Hasher = AddressHasher;
    fn build_hasher(&self) -> Self::Hasher {
        AddressHasher::default()
    }
}

/// Map with see-through hash for u32 keys
pub type U32Map<T> = HashMap<u32, T, NoopHasherU32>;

/// Map with minimal effort hashing for addresses
pub type AddressMap<T> = HashMap<[u8; 20], T>;

#[cfg(test)]
mod test {
    use crate::util::{AddressMap, NoopHasherU32, U32Map};

    #[test]
    fn noop_hasher_byte_order() {
        let mut map = U32Map::<&str>::with_hasher(NoopHasherU32::default());
        map.insert(0xff00_ffff_u32, "a");
        map.insert(0xffff_00ff_u32, "b");
        map.insert(0, "c");
        map.insert(u32::MAX, "d");
        assert_eq!(map.get(&0xff00_ffff_u32), Some(&"a"));
        assert_eq!(map.get(&0xffff_00ff_u32), Some(&"b"));
        assert_eq!(map.get(&0), Some(&"c"));
        assert_eq!(map.get(&u32::MAX), Some(&"d"));
    }

    #[test]
    fn address_hasher() {
        let mut map = AddressMap::<usize>::default();
        let addresses = vec![[0_u8; 20], [1_u8; 20], [2_u8; 20], [0xFF_u8; 20]];
        // Inner closure, the actual test
        for (i, a) in addresses.iter().enumerate() {
            map.insert(*a, i);
            assert_eq!(map.get(a), Some(&i));
        }
    }
}

#[cfg(feature = "bench")]
mod bench {
    extern crate test;
    use super::*;
    use std::collections::HashMap;
    use test::{black_box, Bencher};

    #[bench]
    fn noop_hasher_u32_insert(b: &mut Bencher) {
        b.iter(|| {
            let mut map = U32Map::<&str>::with_hasher(NoopHasherU32::default());
            // Inner closure, the actual test
            for _ in 1..100 {
                black_box({
                    map.insert(0xff00_ffff_u32, "a");
                    map.insert(0xffff_00ff_u32, "b");
                    map.insert(0, "c");
                    map.insert(u32::MAX, "d");
                    map.insert(u32::MAX - 1, "e");
                });
            }
        });
    }

    #[bench]
    fn ordinary_hasher_u32_insert(b: &mut Bencher) {
        b.iter(|| {
            let mut map = HashMap::<u32, &str>::default();
            // Inner closure, the actual test
            for _ in 1..100 {
                black_box({
                    map.insert(0xff00_ffff_u32, "a");
                    map.insert(0xffff_00ff_u32, "b");
                    map.insert(0, "c");
                    map.insert(u32::MAX, "d");
                    map.insert(u32::MAX - 1, "e");
                });
            }
        });
    }

    #[bench]
    fn address_hasher(b: &mut Bencher) {
        b.iter(|| {
            let mut map = AddressMap::<&str>::default();
            let addresses = vec![
                [1_u8; 20],
                [2_u8; 20],
                [3_u8; 20],
                [4_u8; 20],
                [5_u8; 20],
                [6_u8; 20],
                [7_u8; 20],
                [8_u8; 20],
                [9_u8; 20],
                [0xF_u8; 20],
            ];
            // Inner closure, the actual test
            for _ in 1..100 {
                black_box({
                    for a in &addresses {
                        map.insert(*a, "value");
                    }
                });
            }
        });
    }

    #[bench]
    fn ordinary_address_hasher(b: &mut Bencher) {
        b.iter(|| {
            let mut map = HashMap::<[u8; 20], &str>::default();
            let addresses = vec![
                [1_u8; 20],
                [2_u8; 20],
                [3_u8; 20],
                [4_u8; 20],
                [5_u8; 20],
                [6_u8; 20],
                [7_u8; 20],
                [8_u8; 20],
                [9_u8; 20],
                [0xF_u8; 20],
            ];
            // Inner closure, the actual test
            for _ in 1..100 {
                black_box({
                    for a in &addresses {
                        map.insert(*a, "value");
                    }
                });
            }
        });
    }
}
