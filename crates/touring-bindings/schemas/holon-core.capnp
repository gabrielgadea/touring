# THSF holon-core.capnp — Fase 3 D3.1 typed federation schema.
#
# File ID: random 64-bit with MSB set (Cap'n Proto convention).
# Generate your own for new schemas via `capnp id`. Keeping this one
# stable is NOT required — consumers compile against it via capnpc.
#
# This schema defines the Typed Federation interface between Holon
# processes. Promise pipelining (getHolon → listCapabilities) allows
# round-trip elision; see Cap'n Proto docs.

@0xa1b2c3d4e5f67890;

# ============================================================================
# Scalar and enum types
# ============================================================================

enum Adapter {
  # CLI subprocess dispatch (the Fase 1 baseline).
  cli @0;
  # Native Cap'n Proto RPC (this schema).
  capnp @1;
  # WASM component (Fase 4 WASI 0.3).
  wasm @2;
}

enum HandshakeStatus {
  accepted @0;
  rejected @1;
}

# Semantic-version decomposed for easy comparison without string parsing.
struct Version {
  major @0 :UInt32;
  minor @1 :UInt32;
  patch @2 :UInt32;
  # "" when no pre-release tag.
  prerelease @3 :Text;
}

# ============================================================================
# Capability metadata
# ============================================================================

# A single offer or requirement declared by a holon.
struct Capability {
  # Kebab-case identifier, e.g. "symbol-index", "mcts-planner".
  name @0 :Text;

  # One-line human description.
  description @1 :Text;

  # Semantic version string (canonical: "major.minor.patch[-pre]").
  version @2 :Text;

  # Hex-encoded SHA-256 of the capability's JSON Schema file.
  # Empty string if no schema is attached.
  schemaHash @3 :Text;

  # Transport adapter.
  adapter @4 :Adapter;
}

# ============================================================================
# Holon metadata
# ============================================================================

struct HolonInfo {
  # Unique name within the holarchy (kebab/snake case).
  name @0 :Text;

  # Semantic version of the holon itself (not of its capabilities).
  version @1 :Text;

  # Human description.
  description @2 :Text;

  # Absolute path to the manifest file (`.holon/manifest.toml`).
  manifestPath @3 :Text;

  # Capabilities offered by this holon.
  offers @4 :List(Capability);

  # Capability names required (consumption dependencies).
  # Each entry corresponds to an optional-or-mandatory "requires" block.
  requires @5 :List(Text);

  # True when the holon builds/runs without THSF infrastructure.
  # Fase 1 baseline requires this to be true for every holon.
  autonomyGuarantee @6 :Bool;
}

# ============================================================================
# Invocation envelope
# ============================================================================

struct InvokeRequest {
  # Name of the capability being invoked.
  capability @0 :Text;

  # Opaque args payload. MVP: JSON-encoded UTF-8 bytes.
  # Future: switch to a strongly-typed AnyPointer once capability schemas
  # are migrated to capnp structs.
  args @1 :Data;

  # Free-form identifier of the caller (for CRDT invocation logging).
  requester @2 :Text;

  # Per-call timeout in milliseconds. 0 means "use server default".
  timeoutMs @3 :UInt32;
}

struct InvokeResponse {
  # Exit code (0 success; !=0 failure). For Cap'n Proto native handlers
  # without a subprocess, set 0 on OK and 1 on domain error.
  exitCode @0 :Int32;

  # Captured stdout / result bytes. JSON-encoded for MVP.
  stdout @1 :Data;

  # Captured stderr / error message bytes.
  stderr @2 :Data;

  # Wall-clock duration of the invocation.
  durationMs @3 :UInt32;

  # True when the server persisted this invocation to the CRDT state DB.
  # Mirrors the `logged` field of the Python adapter.
  logged @4 :Bool;

  # SHA-256 of the response schema (when the response conforms to one).
  schemaHashOut @5 :Text;
}

# ============================================================================
# Symbiosis reporting
# ============================================================================

struct HandshakeEdge {
  requirer @0 :Text;
  capability @1 :Text;
  provider @2 :Text;
  status @3 :HandshakeStatus;
  # Empty when status == accepted.
  reason @4 :Text;
}

struct SymbiosisReport {
  holonsDiscovered @0 :UInt32;
  handshakesAttempted @1 :UInt32;
  handshakesAccepted @2 :UInt32;
  handshakesRejected @3 :UInt32;
  elapsedMs @4 :UInt32;
  edges @5 :List(HandshakeEdge);
}

# ============================================================================
# RPC interfaces — the meat of the schema
# ============================================================================

# A single holon. Clients receive a Holon cap from HolonRegistry.getHolon
# and can pipeline subsequent calls (list→invoke round-trip elision).
interface Holon {
  # Enumerate this holon's offered capabilities.
  listCapabilities @0 () -> (capabilities :List(Capability));

  # Invoke one capability. Server returns when the backing command
  # completes OR timeoutMs elapses (whichever first).
  invoke @1 (request :InvokeRequest) -> (response :InvokeResponse);

  # Holon metadata bundle.
  info @2 () -> (info :HolonInfo);
}

# The discovery entry point. Bind a `HolonRegistry` to
# `$XDG_RUNTIME_DIR/holon/registry.sock` (Fase 3 D3.2 default).
interface HolonRegistry {
  # All holons discoverable under the server's root.
  listHolons @0 () -> (holons :List(HolonInfo));

  # Return a live capability to a specific holon (promise-pipelined).
  # Resolves to an error if `name` is not known.
  getHolon @1 (name :Text) -> (holon :Holon);

  # Enumerate holons that publish a given capability.
  findByCapability @2 (capability :Text) -> (holons :List(HolonInfo));

  # Run a symbiosis cycle synchronously and return the report.
  # Equivalent to `holon symbiosis /home/gabrielgadea` in the CLI.
  symbiosis @3 () -> (report :SymbiosisReport);

  # Spec version string this server implements. MUST match the one
  # used to generate the client stubs for safety.
  specVersion @4 () -> (version :Text);
}
