#!/usr/bin/env sh
sudo -v
cargo build --release
sudo mv target/release/havax /usr/local/bin/hv
hv -V
