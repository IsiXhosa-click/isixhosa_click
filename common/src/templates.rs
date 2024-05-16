use crate::auth::{Auth, Permissions};
use crate::format::DisplayHtml;
use crate::language::*;
use crate::types::ExistingWord;
use askama::Template;

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
