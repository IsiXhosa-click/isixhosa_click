profile := debug
jaeger_debug := 0
wasm_exported := server/static/wasm
wasm_target := wasm/target/wasm32-unknown-unknown/$(profile)/
isixhosa_server := target/$(profile)/isixhosa_server
pwa_artifact_names := isixhosa_pwa.js isixhosa_pwa_bg.wasm
common_src = common/Cargo.toml $(wildcard common/src/**) $(wildcard common/templates/**)
wordle_artifact_names := isixhosa_wordle.js isixhosa_wordle_bg.wasm
wordle := $(foreach f, $(wordle_artifact_names), $(wasm_exported)/$(f))
pwa := $(foreach f, $(pwa_artifact_names), $(wasm_exported)/$(f))
site := isixhosa

ifeq ($(profile), release)
	cargo_flags := --release
endif

build: $(wordle) $(pwa) $(isixhosa_server)

run: build
ifeq ($(jaeger_debug), 1)
	cd server && ./with_jaeger_debug.sh ../target/$(profile)/isixhosa_server -s $(site) run
else
	cd server && ../target/$(profile)/isixhosa_server -s $(site) --with-otlp false run
endif

clean:
	cargo clean
	cd wasm && cargo clean
	rm -f $(wordle) $(pwa)

$(wasm_target)/isixhosa_pwa.wasm: $(common_src) wasm/Cargo.toml wasm/pwa/Cargo.toml $(wildcard wasm/pwa/src/**)
	cd wasm/pwa && cargo build $(cargo_flags)

$(wasm_target)/isixhosa_wordle.wasm: $(common_src) wasm/Cargo.toml wasm/wordle/Cargo.toml $(wildcard wasm/wordle/src/**) wasm/wordle/words.csv
	cd wasm/wordle && cargo build $(cargo_flags)

$(wasm_exported)/%_bg.wasm $(wasm_exported)/%.js: $(wasm_target)/%.wasm
	$(eval this_wasm := $(patsubst $(wasm_target)/%.wasm, $(dir $@)/%_bg.wasm, $<))
	rm -f $(this_wasm) $(patsubst $(wasm_target)/%.wasm, $(dir $@)/%.js, $<)
	wasm-bindgen --target web --no-typescript $< --out-dir $(dir $@)
	wasm-opt $(this_wasm) -Oz -o $(this_wasm)

$(isixhosa_server): $(common_src) Cargo.toml server/Cargo.toml $(wildcard server/static/**) $(wildcard server/src/**) $(wildcard server/templates/**) $(wildcard macros/**)
	cd server && cargo build $(cargo_flags)

.SECONDARY: $(wasm_target)/%.wasm
.PHONY: build run clean
