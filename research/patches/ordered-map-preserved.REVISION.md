# Packet archival revision 2

Revision 1 is the original 13-file source-only packet. Its exact bytes remain in the external packet-revision-1 snapshot. Its manifest is also retained here as PRIOR-PACKET-MANIFEST.json.

- Prior packet-manifest SHA-256: 597c6827d6c82bf386c3bc30d669617e4c59a1dcc43b80af50b1c5b6d191f750.
- Prior payload identity: 5b3ad6677bd86b14dc3b319f12352d25901fecef39c170b947e8d2add281e4a1.
- Prior payload count: 12 plus its manifest, 13 files total.

Revision 2 adds SCREEN.md, external-evidence.json, this note and the exact prior manifest; updates README.md and the active packet manifest. Every other original packet file is byte-identical, including both frozen source manifests, source/hash lists, source patch, normalization diff, preservation proof, validation and source lineage. The unchanged PROOF.md and SOURCE-LINEAGE.json record the earlier source-only packaging scope; the appended screen now records later historical measurements.

Source G remains 6541fbba6b06bdd755fb4b3074d4a5cfcd86ec3399192bafa49bb53c0431e04a. The source patch remains SHA-256 69bd398d06d9907562339acafaa1c37a4a595652c37df96ca75d592e314a28b6. Original frozen source-manifest SHA-256 remains 7aa41f6d87749db5bf9b3a5af7a2a57793329eeb04c9270daa3dd8c371b2c23e.

All evidence is historical b1d36ce + combined-main-5b13, not validation of current main 0bc80b49, which includes df56. No source adoption, proved regression fix, or known wrapper/cache cause is asserted. The Row16 G/O value follows the exact retained TSV: 0.9404328436810501, rendered as 0.940433. Raw data and harness remain external. No new builds, timings, shared-checkout edits, or GitHub operations were performed.
