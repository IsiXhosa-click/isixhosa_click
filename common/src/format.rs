use std::borrow::Cow;
use crate::i18n::{I18nInfo, TranslationKey};
use crate::language::{NounClassExt, NounClassPrefixes};
use crate::types::{PublicUserInfo, WordHit};
use askama::{Html, MarkupDisplay};
use compact_str::CompactString;
use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{ArcLoader, Loader};
use isixhosa::noun::NounClass;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter, Write};
use crate::i18n_args;

pub fn escape(s: &str) -> MarkupDisplay<Html, &str> {
    MarkupDisplay::new_unsafe(s, Html)
}

pub struct HtmlFormatter<'a, L: Loader = ArcLoader> {
    pub fmt: &'a mut dyn Write,
    pub i18n_info: &'a I18nInfo<L>,
    plain_text: bool,
}

impl<'a, L: Loader + 'static> HtmlFormatter<'a, L> {
    pub fn write_text(&mut self, key: &TranslationKey) -> fmt::Result {
        self.write_raw_str(&self.i18n_info.translate(key))
    }

    pub fn write_text_with_args(
        &mut self,
        key: &TranslationKey,
        args: &HashMap<String, FluentValue<'static>>,
    ) -> fmt::Result {
        self.write_raw_str(&self.i18n_info.translate_with(key, args))
    }

    pub fn write_raw_str(&mut self, raw: &str) -> fmt::Result {
        write!(self.fmt, "{}", escape(raw))
    }

    pub fn write_unescaped_str(&mut self, raw: &str) -> fmt::Result {
        write!(self.fmt, "{}", raw)
    }

    fn write_noun_class_prefix(&mut self, prefix: &str, strong: bool) -> fmt::Result {
        let prefix = escape(prefix);
        if !self.plain_text && strong {
            write!(
                self.fmt,
                "<strong class=\"noun_class_prefix\">{prefix}</strong>",
            )
        } else {
            write!(self.fmt, "{prefix}")
        }
    }

    pub fn fmt_if_non_empty_or(
        &mut self,
        item: &dyn DisplayHtml<L>,
        default: impl FnOnce(&mut Self) -> fmt::Result,
    ) -> fmt::Result {
        let mut str = CompactString::new("");
        let mut fmt = HtmlFormatter {
            fmt: &mut str,
            i18n_info: self.i18n_info,
            plain_text: self.plain_text,
        };

        item.fmt(&mut fmt)?;

        if str.is_empty() {
            default(self)
        } else {
            self.write_unescaped_str(&str)
        }
    }

    pub fn join_if_non_empty<'i, I>(&mut self, sep: &str, items: I) -> fmt::Result
    where
        I: IntoIterator<Item = &'i dyn DisplayHtml<L>>,
    {
        let mut first = true;
        let sep_escaped = escape(sep);

        let i18n_info = self.i18n_info;
        let plain_text = self.plain_text;

        items
            .into_iter()
            .map(|item| {
                let mut str = CompactString::new("");
                let mut fmt = HtmlFormatter {
                    fmt: &mut str,
                    i18n_info,
                    plain_text,
                };
                item.fmt(&mut fmt).unwrap();
                str
            })
            .filter(|item| !item.is_empty())
            .try_for_each(|item| {
                if !first {
                    write!(self.fmt, "{}", sep_escaped)?;
                } else {
                    first = false;
                }

                self.write_unescaped_str(&item)
            })
    }
}

pub struct DisplayHtmlWrapper<'a, T, L = ArcLoader> {
    val: &'a T,
    i18n_info: &'a I18nInfo<L>,
    plain_text: bool,
}

impl<T: DisplayHtml<L>, L: Loader + 'static> Display for DisplayHtmlWrapper<'_, T, L> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        self.val.fmt(&mut HtmlFormatter {
            fmt,
            i18n_info: self.i18n_info,
            plain_text: self.plain_text,
        })
    }
}

pub trait DisplayHtml<L: Loader + 'static> {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result;

    fn to_plaintext<'a>(&'a self, i18n_info: &'a I18nInfo<L>) -> DisplayHtmlWrapper<'a, Self, L>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: self,
            i18n_info,
            plain_text: true,
        }
    }

    fn to_html<'a>(&'a self, i18n_info: &'a I18nInfo<L>) -> DisplayHtmlWrapper<'a, Self, L>
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

impl<L: Loader + 'static, T: DisplayHtml<L>> DisplayHtml<L> for &T {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        T::fmt(self, f)
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for TranslationKey<'_> {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        f.write_text(self)
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for String {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        f.write_raw_str(self)
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for PublicUserInfo {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> std::fmt::Result {
        f.write_raw_str(
            Some(&self.username[..])
                .filter(|_| self.display_name)
                .unwrap_or_default(),
        )
    }
}

impl<L: Loader + 'static, T: DisplayHtml<L>> DisplayHtml<L> for Option<T> {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        if let Some(val) = self {
            val.fmt(f)?;
        }

        Ok(())
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for NounClass {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        self.to_prefixes().fmt(f)
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for NounClassPrefixes {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        f.write_noun_class_prefix(&self.singular, self.selected_singular)?;
        if let Some(plural) = self.plural.as_ref() {
            f.write_raw_str("/")?;
            f.write_noun_class_prefix(plural, !self.selected_singular)?;
        }

        Ok(())
    }
}

pub struct NounClassInHit<T>(pub T);

impl<L: Loader + 'static, T: DisplayHtml<L>> DisplayHtml<L> for NounClassInHit<T> {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        f.write_raw_str(" - ")?;
        f.write_text(&TranslationKey::new("word-hit.class"))?;
        f.write_raw_str(" ")?;
        self.0.fmt(f)
    }
}

impl WordHit {
    pub fn hyperlinked(&self) -> HyperlinkWrapper<'_> {
        HyperlinkWrapper(self)
    }
}

pub struct HyperlinkWrapper<'a>(pub &'a WordHit);

impl<L: Loader + 'static> DisplayHtml<L> for HyperlinkWrapper<'_> {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        if !self.0.is_suggestion {
            f.fmt
                .write_fmt(format_args!("<a href=\"/word/{}\">", self.0.id))?;
            self.0.fmt(f)?;
            f.fmt.write_str("</a>")
        } else {
            self.0.fmt(f)
        }
    }
}

fn is_not_isolator_or_whitespace(c: char) -> bool {
    !c.is_whitespace() && (c < '\u{2066}' || c > '\u{206f}')
}

/// Prepare a grammar info section (e.g `informal intransitive verb`) by:
/// - Stripping whitespace inside unicode isolating characters before the first printable chars
/// - Rewriting all whitespace to ' '
/// - Replacing double-whitespace with single-whitespace
fn prepare_grammar_info(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }

    let mut trimmed = String::new();

    let first_printable = s.chars().position(is_not_isolator_or_whitespace).unwrap();
    let last_printable = s.chars().count() - s.chars().rev().position(is_not_isolator_or_whitespace).unwrap();

    let mut last_printable_was_whitespace = false;
    for (i, c) in s.chars().enumerate() {
        if !((i < first_printable || i > last_printable) && c.is_whitespace()) {
            if c.is_whitespace() {
                if !last_printable_was_whitespace {
                    trimmed.push(' ');
                    last_printable_was_whitespace = true;
                }
            } else {
                last_printable_was_whitespace = false;
                trimmed.push(c);
            }
        }
    }

    trimmed
}

// TODO better diffing here
// TODO: use askama template?
macro_rules! impl_display_html {
    ($($typ:ty),*) => {
        $(impl<L: Loader + 'static> DisplayHtml<L> for $typ {
            fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
                f.write_raw_str(&self.english)?;
                f.write_raw_str(" - ")?;
                f.write_raw_str(&self.xhosa)?;

                if self.has_grammatical_information() {
                    f.write_raw_str(" (")?;

                    let mut args = i18n_args!(
                        "plural" => self.is_plural.to_string(),
                        "informal" => self.is_informal.to_string(),
                        "inchoative" => self.is_inchoative.to_string(),
                        "transitivity" => self.transitivity.map(|t| t.name()).unwrap_or_default(),
                        "part-of-speech" => self.part_of_speech.map(|p| p.name()).unwrap_or_default(),
                    );

                    // Done separately so as not to escape the class argument (which we trust and
                    // has formatting)
                    let class = self.noun_class.as_ref().map(|c| c.to_html(f.i18n_info).to_string()).unwrap_or_else(|| "none".to_string());
                    args.insert("class".to_string(), FluentValue::String(Cow::Owned(class)));

                    let grammar_info = f.i18n_info.translate_with(&TranslationKey::new("word-hit.grammar-info"), &args);
                    f.write_unescaped_str(&prepare_grammar_info(&grammar_info))?;
                    f.write_raw_str(")")?;
                }

                Ok(())
            }
        })*
    };
}

impl_display_html!(WordHit, crate::types::ExistingWord);
