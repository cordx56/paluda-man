#!/bin/bash

CRATE_CC_NO_DEFAULTS=1 cargo build --release && \
    espflash save-image --chip esp32c3 target/riscv32imc-esp-espidf/release/paluda-man paluda-man.bin && \
    curl -X POST --data-binary @paluda-man.bin "http://$1/ota"
