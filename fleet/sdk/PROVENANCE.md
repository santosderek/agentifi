# Vendored Fleet SDK — provenance

**This directory is a vendored copy of code maintained in another repository. Do not treat it as
Agentifi's own source.** Fixes made here will be lost on the next re-vendor unless they are also
pushed upstream.

## Source

| | |
|---|---|
| Source repository | `ssh://git@gitlab.santos:30222/santosderek/fleet-protocol-sdk.git` |
| Source branch | `main` |
| **Source commit** | **`83aef96bf3d1e7eaccac8e5bdaf06c6c31d04c96`** |
| Date vendored | 2026-09-12 |
| Upstream licence | `MIT OR Apache-2.0` |

## Why this is vendored rather than a git dependency

Agentifi is hosted on GitHub and must be buildable by anyone, including its own GitHub Actions CI.
The Fleet SDK is hosted on a **private, home-network-only** GitLab host. A `git = "ssh://…"`
dependency would make Agentifi unbuildable off-network and break public CI. Vendoring trades
single-source-of-truth for independence, deliberately.

## Crates copied, and why only these

The **minimum closure** required for `fleet-client` to compile:

```text
fleet-client        the HTTP client itself
  └── fleet-enrollment   identity bindings, device signing, signed envelopes
        └── fleet-protocol   wire types + canonical JSON validation
```

Upstream also contains `fleet-protocol-signing` and `fleet-link-admission`. **Neither is a
dependency of `fleet-client`**, so neither was copied. Copying them "for completeness" would have
added unused surface that still has to be kept in sync.

## Licence

Upstream is `MIT OR Apache-2.0`; Agentifi is `MIT`. Taking the MIT arm of that dual licence is
compatible, so the combined work remains distributable under Agentifi's MIT `LICENSE`. The
vendored manifests keep the upstream `MIT OR Apache-2.0` identifier rather than being relabelled
`MIT`, so the origin of this code stays accurate. Copyright remains with the upstream author
(Derek Santos), who is also the Agentifi author, so there is no third-party attribution burden.

## Local modifications

The copied `.rs` sources are byte-identical to upstream **except for item 5 below**. Verified with
`diff -r` against the source commit: only the three `Cargo.toml` files and `canonical.rs` differ,
plus the one added test file.

1. **`Cargo.toml` de-workspaced** in all three crates. Upstream used
   `edition.workspace = true`, `rust-version.workspace = true`, `license.workspace = true`,
   `[lints] workspace = true`, and `serde.workspace = true`-style dependencies. Those resolve
   against the *upstream* workspace. Inheriting Agentifi's workspace instead would be actively
   wrong: Agentifi declares `edition = "2021"` and `license = "MIT"`, which would apply edition
   2021 to edition-2024 code and misstate the licence. All such values are now pinned literally.
2. **`[lints] workspace = true` dropped.** Upstream enforces `clippy::pedantic` plus
   `unsafe_code = "forbid"` through its workspace. Agentifi has no `[workspace.lints]`, so the
   vendored crates are linted only by Agentifi's CI (`cargo clippy --workspace --all-targets
   -- -D warnings`), which is the default lint set. **This is a real reduction in lint strictness**
   relative to upstream and is recorded here rather than hidden.
3. **Dependency versions relaxed from `=x.y.z` to caret** where upstream pinned exactly. Upstream
   pins exact versions because it is the release-gating repository for Fleet images. Agentifi has
   no such requirement and an exact pin here would fight Agentifi's own dependency resolution.
   `cargo check` confirms the resolved tree still contains **no `native-tls` and no `openssl-sys`**.
4. **Added `fleet/sdk/fleet-client/tests/wire_conformance.rs`** — see below. This is an Agentifi
   addition and does not exist upstream.
5. **`canonical::encode` widened from `pub(crate)` to `pub`** (`fleet-client/src/canonical.rs`).
   The conformance test lives in `tests/`, which is an external crate, so it cannot reach a
   `pub(crate)` function. It had to call the **real** encoder: a test that re-derived canonical
   bytes itself would still pass if the encoder regressed, making it useless as the drift guard it
   exists to be. This does **not** weaken the crate's guarantee — that guarantee is that no public
   API *accepts* pre-serialised bytes into the transport, and this function only *produces*
   bytes it has already verified against Fleet Core's own validator. The change is marked with a
   `VENDOR-LOCAL MODIFICATION` comment in place. **Re-vendoring will revert it**; re-apply it, or
   the conformance test will not compile.

## The risk this vendoring creates, and what guards it

Vendoring does not remove the need to stay in sync. It makes divergence **silent**. Fleet Core
validates the wire contract at **runtime**: if Core's request shape changes and this copy does not,
there is no compile error — just `400 invalid_request` with no diagnostic pointing at the cause.

That is not hypothetical. `reqwest`'s `.json()` serialises struct fields in **declaration order**,
while Core requires **canonical JSON with lexicographically sorted keys**. Every request from the
Vessel client was rejected until this was found, and the only symptom was `invalid_request`.

Two guards are therefore carried across and must not be removed:

- **`fleet-client/src/canonical.rs`** — the canonical encoder, the single reachable path for
  producing a request body, which verifies its own output using *Core's own validator*. Its unit
  tests include `naive_serialisation_is_rejected_but_canonical_encoding_is_accepted`, which fails
  if the encoder ever regresses to the behaviour that caused the outage.
- **`fleet-client/tests/wire_conformance.rs`** — golden byte-for-byte fixtures pinning the request
  shapes. If Fleet changes the wire contract, this fails loudly in **Agentifi's** CI instead of at
  runtime against a live Core.

## How to re-vendor

```bash
# 1. From a machine with access to the private GitLab host:
cd /path/to/fleet-protocol-sdk
git fetch origin && git checkout main && git rev-parse HEAD   # <- record this

# 2. Replace the copies wholesale (do NOT merge by hand):
cd /path/to/agentifi
for c in fleet-protocol fleet-enrollment fleet-client; do
  rm -rf "fleet/sdk/$c"
  cp -R "/path/to/fleet-protocol-sdk/crates/$c" "fleet/sdk/$c"
done

# 3. Re-apply the local modifications listed above (the three Cargo.toml files),
#    and restore the Agentifi-only test:
git checkout -- fleet/sdk/fleet-client/tests/wire_conformance.rs
git checkout -- fleet/sdk/*/Cargo.toml   # then review the upstream manifest diff by hand

# 4. Update the Source commit / Date in THIS file.

# 5. Prove the contract still holds:
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**If step 5 fails in `wire_conformance.rs`, the Fleet wire contract changed.** That is the signal
this whole arrangement exists to produce. Do not "fix" it by regenerating the fixtures without
first understanding what changed on the Fleet side and whether Agentifi's requests are still valid.

## What a future re-vendor must watch

- **Upstream lint strictness is higher.** Code that passes here may fail upstream's pedantic
  clippy. Contribute fixes upstream, not here.
- **`TaskViewV1` in `types.rs` is a hand-mirror** of a server-side type
  (`run_store_postgres::FleetTaskViewV1`) that this crate deliberately does not depend on. It is
  the single most drift-prone type in the vendored set and has no compile-time link to its source.
- **The reqwest trust-store caveat** documented in `fleet-client/Cargo.toml`: reqwest 0.13's
  `rustls-no-provider` uses `rustls-platform-verifier` (OS trust store), and exposes no
  `webpki-roots` feature. On an image with no trust store, public-CA HTTPS fails.
- **Enrollment requires operator-supplied artifacts.** This SDK never mints the Fleet root key or
  a Fleet-signed authorization, and must never be changed to do so.
