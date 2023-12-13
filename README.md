# Simple LZ4

A very simple LZ4 implementation.

## Usage

    lz4_simple -1 <input> <output>   Compress the input file into the output file (faster)
    lz4_simple -9 <input> <output>   Compress the input file into the output file (smaller)
    lz4_simple -d <input> <output>   Decompress the input file into the output file
    lz4_simple -h <input>            Calculate the XXHash32 checksum

## Features

* Compress a file.
* Decompress a compressed file (only default settings are supported).
* Calculate the XXHash32 checksum of a file.
* Written in Rust.
* Simple and short implementation.
* 100% safe code.

## Performance

The following numbers are including disk I/O:

* ~0.6 GB/s compression, which is a bit slower than the "lz4" command line tool.
* ~1 GB/s decompression, which is similar to the "lz4" command line tool.
* ~3 GB/s checksum, which is around half as fast as the "crc32" command line tool.

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

