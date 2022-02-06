use wasm_bindgen::prelude::*;
use isixhosa_common::auth::Auth;
use isixhosa_common::templates::WordDetails;
use isixhosa_common::types::ExistingWord;

use askama::Template;

#[wasm_bindgen]
pub fn render_word(word: JsValue) -> String {
   let word = word.into_serde().unwrap();
   let template = WordDetails { auth: Auth::Offline, word, previous_success: None, };
   template.render()
}