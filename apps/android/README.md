# Akari for Android

Planned: a Jetpack Compose UI on top of the Kotlin bindings generated from
`akari-ffi`. Tokens are stored with the Android Keystore through the core's
token-storage callback interface.

Akari verifies TLS certificates against the OS trust store, so the app must
initialize [rustls-platform-verifier](https://github.com/rustls/rustls-platform-verifier)
from Kotlin before the core makes network calls.

Status: not started.
