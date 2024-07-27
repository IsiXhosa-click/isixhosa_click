use crate::auth::{Auth, Permissions};
use crate::format::DisplayHtml;
use crate::i18n::I18nInfo;
use crate::language::*;
use crate::types::{ExistingWord, WordHit};
use askama::Template;
use fluent_templates::Loader;
use isixhosa_click_macros::I18nTemplate;
use std::fmt::{Display, Formatter};

#[derive(Template, I18nTemplate)]
#[template(path = "word_details.askama.html")]
pub struct WordDetails<L>
where
    L: Loader + Send + Sync + 'static,
{
    pub auth: Auth,
    pub i18n_info: I18nInfo<L>,
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

#[derive(Template, I18nTemplate)]
#[template(path = "all.askama.html")]
pub struct AllWords<L>
where
    L: Loader + Send + Sync + 'static,
{
    pub auth: Auth,
    pub i18n_info: I18nInfo<L>,
    /// We use a cache here for performance
    pub all_words: String,
}

/// Inner part of the [`AllWords`] template which is regenerated only as needed
#[derive(Template, I18nTemplate)]
#[template(path = "all.list.askama.html")]
pub struct AllWordsList<L>
where
    L: Loader + Send + Sync + 'static,
{
    pub words: Vec<WordHit>,
    pub i18n_info: I18nInfo<L>,
}
