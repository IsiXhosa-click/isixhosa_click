# isixhosa.xyz

Working towards a platform for online Xhosa learning.

## Installation

In order to run, [typesense](https://www.typesense.org) must be installed. To compile the backend server, a nightly
Rust installation is required.

## Run

In order to run the server, typesense must be launched with the environement variables `TYPESENSE_API_KEY` and `TYPESENSE_DATA_DIR`
set. `TYPESENSE_API_KEY` must also be passed to the server binary. One way of doing this could be a `.env` file.

# Maintenance

To delete all words, simply run `rm isixhosa_xyz.db` and 
`curl -H "X-TYPESENSE-API-KEY: ${TYPESENSE_API_KEY}" -X DELETE "http://localhost:8108/collections/words"`.

To create a new typesense API key, run `curl 'http://localhost:8108/keys' -X POST -H "X-TYPESENSE-API-KEY: ${TYPESENSE_API_KEY}" \
-H 'Content-Type: application/json' \
-d '{"description":"Admin key","actions": ["*"], "collections": ["*"]}'
`