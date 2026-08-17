# Licensing

## Headless stack (default)

Crates `imaginarium-core`, `imaginarium-cli`, `imaginarium-mcp`, `imaginarium-server`,
and the Vue assets embedded by the server are dual-licensed:

- MIT (`LICENSE-MIT`)
- Apache-2.0 (`LICENSE-APACHE`)

You may choose either license.

## Native Slint app

`crates/imaginarium-slint` (`imaginarium-app`) is **GPL-3.0-only**
(text: [`LICENSE-GPL`](../LICENSE-GPL)).

Distributing the native GUI binary requires GPL compliance. Distributing only
the headless node (`imaginarium` CLI / future server binary without the Slint
crate) does not pull GPL obligations from that crate, because it is not linked
into the default workspace members.

## Third-party

xAI Imagine API usage is subject to xAI terms and your API key / account terms.
Imaginarium-RS is an independent client; not affiliated with xAI.
