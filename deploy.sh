#!/bin/sh

ssh alex@192.168.4.26 \
    "source .cargo/env && cd ~/projects/adb && git pull && cargo build --release && sudo systemctl restart adb-gram.service"
