use crate::i18n::{translate, I18nInfo, ToTranslationKey, TranslationKey};
use crate::language::{NounClassExt, NounClassPrefixes};
use crate::serialization::SerializeTranslated;
use crate::types::{PublicUserInfo, WordHit};
use askama::{Html, MarkupDisplay};
use isixhosa::noun::NounClass;
use std::fmt::{self, Display, Formatter};

pub fn escape(s: &str) -> MarkupDisplay<Html, &str> {
    MarkupDisplay::new_unsafe(s, Html)
}

pub struct HtmlFormatter<'a, 'b> {
    pub fmt: &'a mut Formatter<'b>,
    i18n_info: &'a I18nInfo,
    plain_text: bool,
}

impl<'a, 'b> HtmlFormatter<'a, 'b> {
    pub fn write_text(&mut self, key: &TranslationKey) -> fmt::Result {
        self.write_raw_str(&translate(key, self.i18n_info, &Default::default()))
    }

    pub fn write_raw_str(&mut self, raw: &str) -> fmt::Result {
        escape(raw).fmt(self.fmt)
    }

    fn write_noun_class_prefix(&mut self, prefix: &str, strong: bool) -> fmt::Result {
        if !self.plain_text && strong {
            write!(
                self.fmt,
                "<strong class=\"noun_class_prefix\">{}</strong>",
                escape(prefix),
            )
        } else {
            self.write_raw_str(prefix)
        }
    }

    pub fn join_if_non_empty<'i>(
        &mut self,
        sep: &str,
        items: impl IntoIterator<Item = &'i dyn DisplayHtml>,
    ) -> fmt::Result {
        let mut first = true;
        let sep_escaped = escape(sep);

        items
            .into_iter()
            .filter(|s| !s.is_empty_str())
            .try_for_each(|s| {
                if !first {
                    sep_escaped.fmt(self.fmt)?;
                } else {
                    first = false
                };

                s.fmt(self)
            })
    }
}

pub struct DisplayHtmlWrapper<'a, T> {
    val: &'a T,
    i18n_info: &'a I18nInfo,
    plain_text: bool,
}

impl<T: DisplayHtml> Display for DisplayHtmlWrapper<'_, T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        self.val.fmt(&mut HtmlFormatter {
            fmt,
            i18n_info: self.i18n_info,
            plain_text: self.plain_text,
        })
    }
}

pub trait DisplayHtml {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result;
    fn is_empty_str(&self) -> bool;

    fn to_plaintext<'a>(&'a self, i18n_info: &'a I18nInfo) -> DisplayHtmlWrapper<'a, Self>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: self,
            i18n_info,
            plain_text: true,
        }
    }

    fn to_html<'a>(&'a self, i18n_info: &'a I18nInfo) -> DisplayHtmlWrapper<'a, Self>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: self,
            i18n_info,
            plain_text: false,
        }
    }
}

impl<T: ToTranslationKey> DisplayHtml for SerializeTranslated<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        DisplayHtml::fmt(&self.val.translation_key(), f)
    }

    fn is_empty_str(&self) -> bool {
        let key = self.val.translation_key();
        key.is_empty_str()
    }
}

impl<T: DisplayHtml> DisplayHtml for &T {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        T::fmt(self, f)
    }

    fn is_empty_str(&self) -> bool {
        T::is_empty_str(self)
    }
}

impl DisplayHtml for TranslationKey<'_> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_text(self)
    }

    fn is_empty_str(&self) -> bool {
        false
    }
}

impl DisplayHtml for &str {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_raw_str(self)
    }

    fn is_empty_str(&self) -> bool {
        self.is_empty()
    }
}

impl DisplayHtml for String {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_raw_str(self)
    }

    fn is_empty_str(&self) -> bool {
        self.is_empty()
    }
}

impl DisplayHtml for PublicUserInfo {
    fn fmt(&self, f: &mut HtmlFormatter) -> std::fmt::Result {
        f.write_raw_str(
            Some(&self.username[..])
                .filter(|_| self.display_name)
                .unwrap_or_default(),
        )
    }

    fn is_empty_str(&self) -> bool {
        !self.display_name
    }
}

impl<T: DisplayHtml> DisplayHtml for Option<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        if let Some(val) = self {
            val.fmt(f)?;
        }

        Ok(())
    }

    fn is_empty_str(&self) -> bool {
        self.as_ref().map(|v| v.is_empty_str()).unwrap_or(true)
    }
}

impl DisplayHtml for NounClass {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        self.to_prefixes().fmt(f)
    }

    fn is_empty_str(&self) -> bool {
        self.to_prefixes().is_empty_str()
    }
}

impl DisplayHtml for NounClassPrefixes {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_noun_class_prefix(&self.singular, self.selected_singular)?;
        if let Some(plural) = self.plural.as_ref() {
            f.write_raw_str("/")?;
            f.write_noun_class_prefix(plural, !self.selected_singular)?;
        }

        Ok(())
    }

    fn is_empty_str(&self) -> bool {
        false
    }
}

pub struct NounClassInHit<T>(pub T);

impl<T: DisplayHtml> DisplayHtml for NounClassInHit<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_raw_str(" - ")?;
        f.write_text(&TranslationKey::new("word-hit.class"))?;
        f.write_raw_str(" ")?;
        self.0.fmt(f)
    }

    fn is_empty_str(&self) -> bool {
        self.0.is_empty_str()
    }
}

impl WordHit {
    pub fn hyperlinked(&self) -> HyperlinkWrapper<'_> {
        HyperlinkWrapper(self)
    }
}

pub struct HyperlinkWrapper<'a>(pub &'a WordHit);

impl DisplayHtml for HyperlinkWrapper<'_> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        if !self.0.is_suggestion {
            f.fmt
                .write_fmt(format_args!("<a href=\"/word/{}\">", self.0.id))?;
            self.0.fmt(f)?;
            f.fmt.write_str("</a>")
        } else {
            self.0.fmt(f)
        }
    }

    fn is_empty_str(&self) -> bool {
        false
    }
}

// TODO: use askama template?
macro_rules! impl_display_html {
    ($($typ:ty),*) => {
        $(impl DisplayHtml for $typ {
            fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
                DisplayHtml::fmt(&self.english, f)?;
                f.write_raw_str(" - ")?;
                DisplayHtml::fmt(&self.xhosa, f)?;

                if self.has_grammatical_information() {
                    f.write_raw_str(" (")?;
                    f.join_if_non_empty(" ", [
                        &if self.is_informal { "informal" } else { "" },
                        &if self.is_inchoative { "inchoative" } else { "" },
                        &self.transitivity as &dyn DisplayHtml,
                        &if self.is_plural { "plural" } else { "" },
                        &self.part_of_speech,
                        &self.noun_class.as_ref().map(NounClassInHit),
                    ])?;
                    f.write_raw_str(")")?;
                }

                Ok(())
            }

            fn is_empty_str(&self) -> bool {
                false
            }
        })*
    };
}

impl_display_html!(WordHit, crate::types::ExistingWord);
