# One-KVM custom RustDesk ID and relay Key

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
