//! Canonical JSON request encoding.
//!
//! # Why this module exists at all
//!
//! Fleet Core validates **every** request body with
//! [`fleet_protocol::validate_canonical_json_bytes`], which re-serialises the parsed value and
//! requires **byte equality** with what was received. In practice that means object keys must be
//! lexicographically ordered and no key may repeat.
//!
//! `reqwest`'s `.json()` helper serialises structs in **declaration order**, not sorted order, so
//! it produces a body Core rejects with HTTP 400 `invalid_request`. This is not hypothetical: the
//! `vessel enroll` command was completely unusable against a live Fleet Core until it was fixed,
//! and the failure was opaque from the client side because Core reports only `invalid_request`.
//!
//! # The fix, and why it is structural rather than advisory
//!
//! Round-tripping through [`serde_json::Value`] sorts keys, because `serde_json`'s map type is
//! `BTreeMap`-backed by default. That is the whole trick. The important part is that this crate
//! makes it the *only* reachable path: [`encode`] is the single place a request body is produced,
//! and the transport module's private `send` is the single place a body is transmitted. There is
//! no public API that accepts pre-serialised bytes, so a caller cannot construct a non-canonical
//! request even by accident.
//!
//! [`encode`] additionally *verifies* its own output with the same validator Core uses, so the
//! guarantee is checked rather than merely intended.

use crate::error::FleetClientError;
use serde::Serialize;

/// Encodes a request body as canonical JSON, then verifies it against Core's own validator.
///
/// The verification step is not redundant belt-and-braces: it pins the client to the *server's*
/// definition of canonical rather than to an assumption about `serde_json`'s map ordering. If that
/// assumption ever breaks, this fails locally with a clear error instead of producing a mysterious
/// HTTP 400 from Core.
//
// VENDOR-LOCAL MODIFICATION (Agentifi): `pub(crate)` upstream, widened to `pub` so the vendored
// copy's wire-conformance test can exercise the REAL encoder rather than a reimplementation of it.
// A test that re-derives canonical bytes itself would still pass if this function regressed, which
// would make it worthless as a drift guard. Widening this does not weaken the crate's guarantee:
// the guarantee is that no public API ACCEPTS pre-serialised bytes into the transport, and this
// function only PRODUCES verified-canonical bytes. See fleet/sdk/PROVENANCE.md.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FleetClientError> {
    // Step 1: into a Value. serde_json::Map is BTreeMap-backed, so this sorts object keys.
    let value = serde_json::to_value(value)
        .map_err(|error| FleetClientError::CanonicalEncoding(error.to_string()))?;

    // Step 2: back to bytes, now in sorted-key order.
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| FleetClientError::CanonicalEncoding(error.to_string()))?;

    // Step 3: check with the exact validator Fleet Core will run on receipt.
    fleet_protocol::validate_canonical_json_bytes(&bytes).map_err(|error| {
        FleetClientError::CanonicalEncoding(format!(
            "encoder produced a body Fleet Core would reject ({error:?}); this is a bug in \
             fleet-client, not in the caller's request"
        ))
    })?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    /// Field declaration order here is deliberately the reverse of lexicographic order, which is
    /// exactly the shape that makes `reqwest::.json()` emit a body Core rejects.
    #[derive(Serialize)]
    struct Unsorted {
        zebra: u32,
        middle: &'static str,
        alpha: bool,
    }

    #[test]
    fn encode_sorts_keys_regardless_of_declaration_order() {
        let bytes = encode(&Unsorted {
            zebra: 1,
            middle: "m",
            alpha: true,
        })
        .expect("encodes");
        assert_eq!(
            String::from_utf8(bytes).expect("utf8"),
            r#"{"alpha":true,"middle":"m","zebra":1}"#
        );
    }

    /// The regression guard for the actual production bug: the naive serialisation is NOT
    /// canonical, and ours is. If this ever stops holding, the encoder has silently regressed to
    /// the behaviour that broke `vessel enroll`.
    #[test]
    fn naive_serialisation_is_rejected_but_canonical_encoding_is_accepted() {
        let request = Unsorted {
            zebra: 1,
            middle: "m",
            alpha: true,
        };

        let naive = serde_json::to_vec(&request).expect("serialises");
        assert!(
            fleet_protocol::validate_canonical_json_bytes(&naive).is_err(),
            "declaration-order serialisation must NOT be canonical, or this test proves nothing"
        );

        let canonical = encode(&request).expect("encodes");
        fleet_protocol::validate_canonical_json_bytes(&canonical)
            .expect("canonical encoding must satisfy Fleet Core's validator");
    }

    #[test]
    fn nested_objects_are_sorted_at_every_depth() {
        #[derive(Serialize)]
        struct Outer {
            zulu: Inner,
            alpha: Inner,
        }
        #[derive(Serialize)]
        struct Inner {
            yankee: u8,
            bravo: u8,
        }

        let bytes = encode(&Outer {
            zulu: Inner {
                yankee: 2,
                bravo: 1,
            },
            alpha: Inner {
                yankee: 4,
                bravo: 3,
            },
        })
        .expect("encodes");
        assert_eq!(
            String::from_utf8(bytes).expect("utf8"),
            r#"{"alpha":{"bravo":3,"yankee":4},"zulu":{"bravo":1,"yankee":2}}"#
        );
    }
}
