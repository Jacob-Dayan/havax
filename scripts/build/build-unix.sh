#!/usr/bin/env sh
sudo -v

if [ ! -f .cargo/config.toml ]; then
    rtouch -p .cargo/config.toml || mkdir .cargo && touch .cargo/config.toml 
    cat << EOF > .cargo/config.toml
[build]
target-dir="/dev/shm/havax-target"
EOF
fi

cargo build --profile opt
sudo mv /dev/shm/havax-target/opt/havax /usr/local/bin/hv
hv -V

