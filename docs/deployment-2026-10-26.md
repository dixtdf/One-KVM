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
