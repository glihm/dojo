#!/bin/bash

# Check if --fix flag is provided
if [ "$1" == "--fix" ]; then
    FMT_ARG=""
else
    FMT_ARG="--check"
fi

scarb --manifest-path examples/spawn-and-move/Scarb.toml fmt $FMT_ARG
scarb --manifest-path examples/simple/Scarb.toml fmt $FMT_ARG
scarb --manifest-path crates/dojo/core/Scarb.toml fmt $FMT_ARG
scarb --manifest-path crates/dojo/core-cairo-test/Scarb.toml fmt $FMT_ARG
