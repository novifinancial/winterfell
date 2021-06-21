// Copyright (c) Facebook, Inc. and its affiliates.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::errors::ProofSerializationError;
use crypto::Hasher;
use utils::{ByteReader, ByteWriter, DeserializationError};

// COMMITMENTS
// ================================================================================================

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Commitments(Vec<u8>);

impl Commitments {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns a new Commitments struct initialized with the provided commitments.
    pub fn new<H: Hasher>(
        trace_root: H::Digest,
        constraint_root: H::Digest,
        fri_roots: Vec<H::Digest>,
    ) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(trace_root.as_ref());
        bytes.extend_from_slice(constraint_root.as_ref());
        for fri_root in fri_roots.iter() {
            bytes.extend_from_slice(fri_root.as_ref());
        }
        Commitments(bytes)
    }

    // PUBLIC METHODS
    // --------------------------------------------------------------------------------------------

    /// Adds the specified commitment to the list of commitments.
    pub fn add<H: Hasher>(&mut self, commitment: &H::Digest) {
        self.0.extend_from_slice(commitment.as_ref())
    }

    // PARSING
    // --------------------------------------------------------------------------------------------

    /// Parses the serialized commitments into distinct parts.
    #[allow(clippy::type_complexity)]
    pub fn parse<H: Hasher>(
        self,
        num_fri_layers: usize,
    ) -> Result<(H::Digest, H::Digest, Vec<H::Digest>), ProofSerializationError> {
        let num_bytes = self.0.len();
        // +1 for trace_root, +1 for constraint root, +1 for FRI remainder commitment
        let num_commitments = num_fri_layers + 3;
        let (commitments, read_bytes) = H::read_digests_into_vec(&self.0, num_commitments)
            .map_err(|err| ProofSerializationError::FailedToParseCommitments(err.to_string()))?;
        // make sure we consumed all available commitment bytes
        if read_bytes != num_bytes {
            return Err(ProofSerializationError::TooManyCommitmentBytes(
                read_bytes, num_bytes,
            ));
        }
        Ok((commitments[0], commitments[1], commitments[2..].to_vec()))
    }

    // SERIALIZATION / DESERIALIZATION
    // --------------------------------------------------------------------------------------------

    /// Serializes `self` and writes the resulting bytes into the `target` writer.
    pub fn write_into<W: ByteWriter>(&self, target: &mut W) {
        assert!(self.0.len() < u16::MAX as usize);
        target.write_u16(self.0.len() as u16);
        target.write_u8_slice(&self.0);
    }

    /// Reads commitments from the specified source starting at the specified position and
    /// increments `pos` to point to a position right after the end of read-in commitment bytes.
    /// Returns an error of a valid Commitments struct could not be read from the specified source.
    pub fn read_from<R: ByteReader>(
        source: &R,
        pos: &mut usize,
    ) -> Result<Self, DeserializationError> {
        let num_bytes = source.read_u16(pos)? as usize;
        let result = source.read_u8_vec(pos, num_bytes)?;
        Ok(Commitments(result))
    }
}

impl Default for Commitments {
    fn default() -> Self {
        Commitments(Vec::new())
    }
}
