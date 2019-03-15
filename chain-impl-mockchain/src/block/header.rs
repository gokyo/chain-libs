use chain_core::property;

use crate::date::BlockDate;
use crate::key::{
    deserialize_public_key, deserialize_signature, serialize_public_key, serialize_signature,
    verify_signature, Hash,
};
use crate::leadership::{BftLeader, GenesisPraosLeader, PublicLeader};
use chain_crypto::algorithms::vrf::vrf;
use chain_crypto::algorithms::FakeMMM;
use chain_crypto::{Ed25519Extended, PublicKey, Signature, Verification};

pub type HeaderHash = Hash;
pub type BlockContentHash = Hash;
pub type BlockId = Hash;
pub type BlockContentSize = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockVersion(pub(crate) u16);

pub const BLOCK_VERSION_CONSENSUS_NONE: BlockVersion = BlockVersion::new(0x0000_0000);
pub const BLOCK_VERSION_CONSENSUS_BFT: BlockVersion = BlockVersion::new(0x0000_0001);
pub const BLOCK_VERSION_CONSENSUS_GENESIS_PRAOS: BlockVersion = BlockVersion::new(0x0000_0002);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Common {
    pub block_version: BlockVersion,
    pub block_date: BlockDate,
    pub block_content_size: BlockContentSize,
    pub block_content_hash: BlockContentHash,
    pub block_parent_hash: BlockId,
}

pub type HeaderToSign = Common;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftProof {
    pub(crate) leader_id: BftLeader,
    pub(crate) signature: BftSignature,
}

#[derive(Debug, Clone)]
pub struct BftSignature(pub(crate) Signature<HeaderToSign, Ed25519Extended>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenesisPraosProof {
    pub(crate) vrf_public_key: vrf::PublicKey,
    pub(crate) vrf_proof: vrf::ProvenOutputSeed,
    pub(crate) kes_public_key: PublicKey<FakeMMM>,
    pub(crate) kes_proof: KESSignature,
}

#[derive(Debug, Clone)]
pub struct KESSignature(pub(crate) Signature<HeaderToSign, FakeMMM>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proof {
    /// In case there is no need for consensus layer and no need for proof of the
    /// block. This may apply to the genesis block for example.
    None,
    Bft(BftProof),
    GenesisPraos(GenesisPraosProof),
}

/// this is the block header, it contains the necessary data
/// to prove a given block has been signed by the appropriate
/// nodes, it also contains the metadata to localize the block
/// within the blockchain (the block date and the parent's hash)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub(crate) common: Common,
    pub(crate) proof: Proof,
}

impl BlockVersion {
    pub const fn new(v: u16) -> Self {
        BlockVersion(v)
    }
}

impl PartialEq<Self> for BftSignature {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_ref() == other.0.as_ref()
    }
}
impl Eq for BftSignature {}

impl PartialEq<Self> for KESSignature {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_ref() == other.0.as_ref()
    }
}
impl Eq for KESSignature {}

impl Proof {
    pub fn leader_id(&self) -> Option<PublicLeader> {
        match self {
            Proof::None => None,
            Proof::Bft(bft_proof) => Some(PublicLeader::Bft(bft_proof.leader_id.clone())),
            Proof::GenesisPraos(genesis_praos_proof) => {
                Some(PublicLeader::GenesisPraos(GenesisPraosLeader {
                    kes_public_key: genesis_praos_proof.kes_public_key.clone(),
                    vrf_public_key: genesis_praos_proof.vrf_public_key.clone(),
                }))
            }
        }
    }
}

impl Header {
    #[inline]
    pub fn block_version(&self) -> &BlockVersion {
        &self.common.block_version
    }

    #[inline]
    pub fn block_date(&self) -> &BlockDate {
        &self.common.block_date
    }

    #[inline]
    pub fn block_content_hash(&self) -> &BlockContentHash {
        &self.common.block_content_hash
    }

    #[inline]
    pub fn block_parent_hash(&self) -> &BlockId {
        &self.common.block_parent_hash
    }

    /// function to compute the Header Hash as per the spec. It is the hash
    /// of the serialized header (except the first 2bytes: the size)
    #[inline]
    pub fn hash(&self) -> HeaderHash {
        // TODO: this is not the optimal way to compute the crypto graphic hash
        use chain_core::property::Serialize;
        let bytes = self.serialize_as_vec().unwrap();
        HeaderHash::hash_bytes(&bytes[2..])
    }

    pub fn proof(&self) -> &Proof {
        &self.proof
    }

    /// this function verify the proof and the consistency of the block
    /// within itself.
    pub fn verify_proof(&self) -> Verification {
        match &self.proof {
            Proof::None => Verification::Success,
            Proof::Bft(bft_proof) => {
                verify_signature(&bft_proof.signature.0, &bft_proof.leader_id.0, &self.common)
            }
            Proof::GenesisPraos(genesis_praos_proof) => {
                verify_signature(
                    &genesis_praos_proof.kes_proof.0,
                    &genesis_praos_proof.kes_public_key,
                    &self.common,
                )
                // TODO: verify the VRF too
            }
        }
    }
}

impl property::Header for Header {
    type Id = HeaderHash;
    type Date = BlockDate;
    type Version = BlockVersion;

    fn id(&self) -> Self::Id {
        self.hash()
    }

    fn date(&self) -> Self::Date {
        *self.block_date()
    }

    fn version(&self) -> Self::Version {
        *self.block_version()
    }
}

impl property::Serialize for Common {
    type Error = std::io::Error;

    fn serialize<W: std::io::Write>(&self, writer: W) -> Result<(), Self::Error> {
        use chain_core::packer::Codec;
        use std::io::Write;

        let mut codec = Codec::from(writer);

        codec.put_u16(self.block_version.0)?;
        codec.put_u32(self.block_content_size)?;
        codec.put_u32(self.block_date.epoch)?;
        codec.put_u32(self.block_date.slot_id)?;
        codec.write_all(self.block_content_hash.as_ref())?;
        codec.write_all(self.block_parent_hash.as_ref())?;

        Ok(())
    }
}

impl property::Serialize for Header {
    type Error = std::io::Error;

    fn serialize<W: std::io::Write>(&self, writer: W) -> Result<(), Self::Error> {
        use chain_core::packer::Codec;

        let mut buffered = Codec::from(writer).buffered();

        let header_size_hole = buffered.hole(2)?;

        self.common.serialize(&mut buffered)?;

        match &self.proof {
            Proof::None => {}
            Proof::Bft(bft_proof) => {
                serialize_public_key(&bft_proof.leader_id.0, &mut buffered)?;
                serialize_signature(&bft_proof.signature.0, &mut buffered)?;
            }
            Proof::GenesisPraos(genesis_praos_proof) => {
                use std::io::Write;
                {
                    let mut buf = [0; vrf::PUBLIC_SIZE];
                    genesis_praos_proof.vrf_public_key.to_buffer(&mut buf);
                    buffered.write_all(&buf)?;
                }
                {
                    let mut buf = [0; vrf::PROOF_SIZE];
                    genesis_praos_proof.vrf_proof.to_bytes(&mut buf);
                    buffered.write_all(&buf)?;
                }
                serialize_public_key(&genesis_praos_proof.kes_public_key, &mut buffered)?;
                serialize_signature(&genesis_praos_proof.kes_proof.0, &mut buffered)?;
            }
        }

        buffered.fill_hole_u16(header_size_hole, buffered.buffered_len() as u16 - 2);
        let _codec = buffered.into_inner()?;

        Ok(())
    }
}

impl property::Deserialize for Header {
    type Error = std::io::Error;

    fn deserialize<R: std::io::BufRead>(reader: R) -> Result<Self, Self::Error> {
        use chain_core::packer::Codec;
        use std::io::Read;

        let mut codec = Codec::from(reader);

        let _header_size = codec.get_u16()?;
        let block_version = codec.get_u16().map(BlockVersion::new)?;
        let block_content_size = codec.get_u32()?;
        let epoch = codec.get_u32()?;
        let slot_id = codec.get_u32()?;
        let block_date = BlockDate { epoch, slot_id };

        let mut hash = [0; 32];
        codec.read_exact(&mut hash)?;
        let block_content_hash = Hash::from(chain_crypto::Blake2b256::from(hash));
        let mut hash = [0; 32];
        codec.read_exact(&mut hash)?;
        let block_parent_hash = Hash::from(chain_crypto::Blake2b256::from(hash));

        let proof = match block_version {
            BLOCK_VERSION_CONSENSUS_NONE => Proof::None,
            BLOCK_VERSION_CONSENSUS_BFT => {
                // BFT
                let leader_id = deserialize_public_key(&mut codec).map(BftLeader)?;
                let signature = deserialize_signature(&mut codec).map(BftSignature)?;
                Proof::Bft(BftProof {
                    leader_id,
                    signature,
                })
            }
            BLOCK_VERSION_CONSENSUS_GENESIS_PRAOS => unimplemented!(),
            _ => unimplemented!("block_version: 0x{:08x}", block_version.0),
        };

        Ok(Header {
            common: Common {
                block_version,
                block_date,
                block_content_size,
                block_content_hash,
                block_parent_hash,
            },
            proof,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use quickcheck::{Arbitrary, Gen, TestResult};

    quickcheck! {
        fn header_serialization_bijection(b: Header) -> TestResult {
            property::testing::serialization_bijection(b)
        }
    }

    impl Arbitrary for BlockVersion {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            // TODO: we are not testing the Proof for Genesis Praos at the moment
            //       set the modulo to 3 when relevant
            BlockVersion::new(u16::arbitrary(g) % 2)
        }
    }
    impl Arbitrary for Common {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            Common {
                block_version: Arbitrary::arbitrary(g),
                block_date: Arbitrary::arbitrary(g),
                block_content_size: Arbitrary::arbitrary(g),
                block_content_hash: Arbitrary::arbitrary(g),
                block_parent_hash: Arbitrary::arbitrary(g),
            }
        }
    }

    impl Arbitrary for BftProof {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            let sk = crate::key::test::arbitrary_secret_key(g);
            let pk = sk.to_public();
            let signature = chain_crypto::Signature::generate(&sk, &[0u8, 1, 2, 3]);
            BftProof {
                leader_id: BftLeader(pk),
                signature: BftSignature(signature.coerce()),
            }
        }
    }
    impl Arbitrary for GenesisPraosProof {
        fn arbitrary<G: Gen>(_g: &mut G) -> Self {
            unimplemented!()
        }
    }

    impl Arbitrary for Header {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            let common = Common::arbitrary(g);
            let proof = match common.block_version {
                BLOCK_VERSION_CONSENSUS_NONE => Proof::None,
                BLOCK_VERSION_CONSENSUS_BFT => Proof::Bft(Arbitrary::arbitrary(g)),
                BLOCK_VERSION_CONSENSUS_GENESIS_PRAOS => {
                    Proof::GenesisPraos(Arbitrary::arbitrary(g))
                }
                _ => unreachable!(),
            };
            Header {
                common: common,
                proof: proof,
            }
        }
    }
}
