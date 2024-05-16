use askama::Template;
use isixhosa_common::auth::Auth;
use isixhosa_common::templates::WordDetails;
use isixhosa_common::types::ExistingWord;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn render_word(word: JsValue) -> String {
    let word: ExistingWord = serde_wasm_bindgen::from_value(word).unwrap();
    let template = WordDetails {
        auth: Auth::Offline,
        word,
        previous_success: None,
    };
    template.render().unwrap()
}
