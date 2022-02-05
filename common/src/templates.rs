use askama::Template;
use crate::auth::{Auth, Permissions};
use crate::types::ExistingWord;
use crate::language::*;
use crate::format::DisplayHtml;

#[derive(Template)]
#[template(path = "word_details.askama.html")]
pub struct WordDetails {
    pub auth: Auth,
    pub word: ExistingWord,
    pub previous_success: Option<WordChangeMethod>,
}

pub enum WordChangeMethod {
    Edit,
    Delete,
}
