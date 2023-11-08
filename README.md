# Simple LZ4

A very simple LZ4 implementation.

## Usage

    lz4_simple -1 <input> <output>   Compress the input file into the output file
    lz4_simple -d <input> <output>   Decompress the input file into the output file
    lz4_simple -h <input>            Calculate the XXHash32 checksum

## Features

* Compress a file
* Decompress a compressed file (only default settings are supported)
* Calculate the XXHash32 checksum of a file
* 100% safe code

## Performance

* Decompress is faster than the "lz4" command line tool on a M1 Mac for unknown reasons.
* Hash is much faster than MD5 or SHA command line tools, and a bit slower than "crc32".

## Code Coverage

Install:

    cargo install rustfilt
    rustup component add llvm-tools-preview
    find ~/.rustup -name llvm-profdata
    open ~/.zprofile

Cleanup:

    rm *.prof*

Coverage of one run:

    RUSTFLAGS="-C instrument-coverage" cargo build
    ./target/debug/lz4_simple -h test.txt
    llvm-profdata merge -sparse default_*.profraw -o prof.profdata
    llvm-cov show -Xdemangler=rustfilt ./target/debug/lz4_simple \
        -instr-profile=prof.profdata \
        -show-line-counts-or-regions \
        -show-instantiations \
        -name-regex=".*"

Coverage of tests:

    cargo clean
    RUSTFLAGS="-C instrument-coverage" cargo test
    llvm-profdata merge -sparse default_*.profraw -o prof.profdata
    FILE=`find ./target/debug/deps -type f ! -name "*.*"`
    llvm-cov show -Xdemangler=rustfilt ${FILE} \
        -instr-profile=prof.profdata \
        -show-line-counts-or-regions \
        -show-instantiations \
        -name-regex=".*"

