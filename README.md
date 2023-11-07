# lz4_simple

A very simple LZ4 implementation.

## Usage

    lz4_simple -d <input> <output>   Decompress the input
    lz4_simple -h <input>            Calculate the hash

## Features

* Decompress (only default settings)
* The hash is the XXHash32
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


Show result:

