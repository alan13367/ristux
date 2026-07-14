# ristux-ssh

`ristux-ssh` is the narrow, pure-Rust SSH command transport used by gix and
Cargo. It intentionally implements the OpenSSH CLI subset needed for smart Git
transport: host/user/port/key selection, `known_hosts` verification, public-key
authentication, remote command execution, and a bidirectional stdin/stdout
channel.

The client vendors `russh` 0.62.2 with a small Ristux overlay that removes the
AWS-LC/ring-only cipher implementations. It negotiates the crate's pure-Rust
AES-CTR and HMAC-SHA2 algorithms, without OpenSSL, AWS-LC, ring, or libssh.
The initial supported key is an OpenSSH Ed25519 private key. Interactive shells,
PTY allocation, forwarding, agents, and server mode are out of scope.
