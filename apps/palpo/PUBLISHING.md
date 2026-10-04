# Distribution status

Palpo is currently a build-owned Rinx system app. It uses the exact
`palpo-admin-v1` host feature, shared Rinx controls and an unpublished additive
App Contract 1.2 extension. A stock card-host cannot supply these services.

The Design Flow template's store listing was removed: its sample publisher,
Android claim and nonexistent screenshot were not evidence for this app.
No store listing, publisher identity or platform approval has been fabricated.
App Hub admission is therefore intentionally incomplete, even when the source
and integrity checks pass. Nothing has been submitted or published.

Before an App Hub release, obtain the publisher's identity, support and privacy
URLs, signing key, and approved platform claims; add a real `bundle/listing.json`
and reviewed native screenshots; release the shared host contract; then rerun
Design Flow stamping, scanning, signing and admission. Desktop instrumentation
at 430 points is not Android or OpenHarmony device evidence.

Development validation uses `examples/palpo_miniapp.rs` and
`tools/wechat-ux/live/native_palpo.py`. They run the production bundle, shared
theme controls, native HTTP adapter and local Palpo HTTP/SQLite implementation.
The Matrix server in that test is an explicit fixture.
