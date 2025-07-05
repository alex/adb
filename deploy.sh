#!/bin/sh

ssh alex@192.168.7.243 \
    "source .cargo/env && cd ~/projects/adb && git pull && cargo build --release && sudo systemctl restart adb-gram.service"
