# Deployment verification: 2026-10-26

- Build configuration: 25dbdf944ef2d41496ff02adbdd7ff74da4205c8
- Upstream: b5d13ee16e099df4082e1da96a37db0888c57bb7
- GitHub Actions: https://github.com/dixtdf/One-KVM/actions/runs/40297744071
- Package: one-kvm_0.2.8+custom.1_arm64.deb
- Package SHA-256: 0681ef982d92b58969128369f218a0ad3226f1baaf84ec7fe6016220742d8ec8
- GitHub artifact ZIP SHA-256: cb349aa8345c5079174e1c3568d4907699293e273aabb2d8a67780c11027d652

The frontend passed its TypeScript check and build. Ten tests compiled against
the actual config/update implementation passed. This test harness is narrower
than the complete application's unit-test suite.

The actual ARM64 candidate was then tested on the target using an isolated
database while the official service was stopped. Arbitrary non-Base64 Key and
custom ID persisted exactly across a real process restart; omitted fields were
preserved and an explicit empty Key cleared it. Fixtures were not sent to a
real rendezvous/relay server.

The package was installed on a Raspberry Pi 4B after retaining the official
package, binary, SQLite backup and systemd configuration. Installed executable
hash matched the extracted, verified build. Existing RustDesk ID, Key, password,
UUID and private keys were compared locally and preserved; none are published.

The C792 capture configuration and Type-C OTG settings were preserved. After
reattaching the USB hostdev in Unraid, macOS recognized the One-KVM USB device;
KeyboardSetupAssistant did not run. No machine reboot was performed.

Physical C792 input and configured timings both remained 1920x1080 at 50Hz.
The UYVY-to-NV12 bridge remained active. A Windows WebRTC receiver decoded
900 H.264 frames at 1920x1080; server reported 29.998835 FPS over the short test.
An actual temporary capture image was visually inspected for completeness and
alignment, then deleted. This does not establish sustained latency or long-term
hardware stability.

RustDesk reported running and registered with the unchanged production
configuration. The user's arbitrary shared Key was not available in the saved
configuration. Authenticated end-to-end relay, RustDesk video and input using
that Key are therefore NOT qualified by this deployment.

## Using the custom fields

Refresh the One-KVM page after upgrading. In Settings -> Extensions ->
Third-party access -> RustDesk, edit Device ID and Relay Key, then Save.
ID and Key have no local format validation and are sent verbatim.
Server-side rejection remains an error; saving alone is not proof of registration.

## Follow-up: custom-string server handshake and manual ID

The user's server runs rustdesk-server 1.1.15 with the same custom-string `-k`
value on hbbs and hbbr. Registration can succeed without that value: registration
does not establish authenticated relay connectivity. No server Compose changes
are needed for the confirmed handshake failure.

Official clients send an empty initial Message when the rendezvous server cannot
provide a verified signing key. The original One-KVM secure connection ignored
that Message and never sent its password challenge. The patch starts password
authentication once, only during the initial unencrypted key exchange. It does
not authenticate the peer, reset an existing challenge, or downgrade an encrypted
session. The intranet rendezvous control socket is also no longer mistaken for
the actual peer connection.

Backend build: `4554517952f14739d80ff1ac167dbf5282ff1605`, version
`0.2.8+custom.2`, Actions run 40316339034. Artifact ZIP SHA-256:
`3b8becd66a83f88358def40cefceed73df78683969e212d22cf091023888aa76`.

The user explicitly wants to type a fixed custom ID. The random-ID button was
removed from the frontend in `23015277ec9280b273a1b89332e4a3181645479f`.
The label and placeholder explain typing the ID and pressing Save. Frontend run
40317554911 passed Vue type checking and the production build. Its ZIP SHA-256:
`ae9176c14018d8b7cf245517d1cc512b0b6decde4614bc7473e04fbafb8d8375`.
It can be loaded using the upstream native `ONE_KVM_FRONTEND_DIR` override;
this keeps the exact backend and frontend build identities independently recorded.

Both artifacts were hash-verified and deployed on the Pi. The DEB SHA-256 is
`700bbc55bd13cc6664bc5eb83806a36a72de859535bc14faef4f2d5d93164930`.
Installed executable SHA-256 matches the extracted candidate:
`906457799fcf0ffee805b68863ac7acc152d57007bcece772729743382a62188`.
The current application configuration compared equal to the immediate pre-upgrade
SQLite backup. The served settings HTML matched the frontend override index.

Independent wire-level probes through the user's running hbbs/hbbr passed direct
and forced-relay transport: empty initial Message produced the password challenge,
an incorrect password was rejected, the actual device password returned PeerInfo,
and 90 H.264 frames decoded at 1920x1080 for each transport. Repeated empty Messages
did not restart the challenge. An encrypted-session regression probe using the
device's public signing key obtained out of band also passed authentication and
90-frame decoding. This last test does not establish automatic server public-key
verification with a custom-string relay Key. No keyboard events were injected;
this is protocol/video validation rather than a native GUI interaction test.

After the service restart, the stale USB hostdev address on Unraid was detached
and the same VID/PID device reattached live. macOS recognized One-KVM USB Device
and KeyboardSetupAssistant was absent. No VM or host reboot was performed.

Private backup and rollback remain on the Pi at
`/root/one-kvm-custom2-deploy-20261026/`. Run `rollback-custom1.sh` there to restore
custom.1 and the immediate pre-upgrade database and remove the frontend override.
The new override drop-in is `30-manual-id-frontend.conf`; the existing C792 bridge
drop-ins remain in place. Configuration databases and keys are not included here.
