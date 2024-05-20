use crate::auth::{Auth, Permissions};
use crate::format::DisplayHtml;
use crate::i18n::I18nInfo;
use crate::language::*;
use crate::types::ExistingWord;
use askama::Template;
use isixhosa_click_macros::I18nTemplate;
use std::fmt::{Display, Formatter};

#[derive(Template, I18nTemplate)]
#[template(path = "word_details.askama.html")]
pub struct WordDetails {
    pub auth: Auth,
    pub i18n_info: I18nInfo,
    pub word: ExistingWord,
    pub previous_success: Option<WordChangeMethod>,
}

pub enum WordChangeMethod {
    Edit,
    Delete,
}

impl Display for WordChangeMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WordChangeMethod::Edit => "edit",
            WordChangeMethod::Delete => "delete",
        };

        f.write_str(s)
    }
}
