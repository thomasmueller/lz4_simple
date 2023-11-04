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
