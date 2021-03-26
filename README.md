# isixhosa.xyz

Working towards a platform for online Xhosa learning.

## Installation

In order to run, [typesense](https://www.typesense.org) must be installed. To compile the backend server, a nightly
Rust installation is required.

## Run

In order to run the server, typesense must be launched with the environement variables `TYPESENSE_API_KEY` and `TYPESENSE_DATA_DIR`
set. `TYPESENSE_API_KEY` must also be passed to the server binary. One way of doing this could be a `.env` file.
