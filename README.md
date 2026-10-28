# One-KVM custom RustDesk ID and relay Key

Experimental multi-channel development is on `codex/multi-channel-platform`.
The domain model is not connected to the production runtime yet. See the
[architecture and scope](docs/architecture.md),
[upstream maintenance procedure](docs/upstream-maintenance.md), and
[development log](docs/development-log.md). Existing ARM64 builds and deployed
custom.2 remain single-channel until runtime isolation passes integration and
hardware verification.

This user-owned repository applies a patch to One-KVM 0.2.8 at commit
b5d13ee16e099df4082e1da96a37db0888c57bb7 and builds an ARM64 Debian package.
Device ID and relay authentication Key are editable and sent verbatim,
without local character, length or Base64 validation. Empty Key clears it;
omitted API fields preserve their values. Server errors remain errors.
Device-password verification, signing keys and UUID are preserved.

The build never embeds deployment credentials, Keys or databases.
Use the authenticated settings page after installation.
No secure_tcp bypass or artificial registration success is introduced.

## Build
Push main or dispatch the Build custom One-KVM ARM64 workflow.
Output includes DEB, corresponding source and modifications, provenance,
license and SHA256SUMS. Compilation is not hardware qualification.

## Deployment
Back up the original DEB, binary and SQLite database with SQLite backup API.
Keep the database private. Preserve systemd drop-ins, the C792 UYVY-to-NV12
bridge, /dev/video42 and Type-C OTG. Upgrade restarts One-KVM and may require
USB reattachment on Unraid. Verify actual video pixels and HID.

## License
AGPL-3.0-only. Preserve upstream copyright and dependency notices.
Corresponding source and complete modifications accompany every artifact.

## Verified deployment

The ARM64 custom.1 build was installed and checked on 2026-10-26. See
[deployment evidence and limits](docs/deployment-2026-10-26.md).

## Unsigned ID-server handshake compatibility

custom.2 handles the official client empty-Message key-exchange fallback by
sending the normal password challenge once. It preserves password verification
and never switches an established encrypted session to plaintext. A LocalAddr
control connection to hbbs is no longer treated as a remote-desktop data stream.
