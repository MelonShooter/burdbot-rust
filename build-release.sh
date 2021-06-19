#!/bin/bash

OPUS_STATIC=true
OPUS_NO_PKG=true

export OPUS_STATIC
export OPUS_NO_PKG

sudo apt-get install -y gcc
sudo apt-get install -y autoconf
sudo apt-get install -y libtool
sudo apt-get install -y make

cargo build --release
