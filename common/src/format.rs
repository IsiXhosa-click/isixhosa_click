use crate::language::{NounClassExt, NounClassPrefixes};
use crate::serialization::SerOnlyDisplay;
use crate::types::{PublicUserInfo, WordHit};
use askama::{Html, MarkupDisplay};
use isixhosa::noun::NounClass;
use std::fmt::{self, Display, Formatter};

fn escape(s: &str) -> MarkupDisplay<Html, &str> {
    MarkupDisplay::new_unsafe(s, Html)
}

pub struct HtmlFormatter<'a, 'b> {
    pub fmt: &'a mut Formatter<'b>,
    plain_text: bool,
}

impl<'a, 'b> HtmlFormatter<'a, 'b> {
    pub fn write_text(&mut self, text: &str) -> fmt::Result {
        escape(text).fmt(self.fmt)
    }

    fn write_noun_class_prefix(&mut self, text: &str, strong: bool) -> fmt::Result {
        if !self.plain_text && strong {
            write!(
                self.fmt,
                "<strong class=\"noun_class_prefix\">{}</strong>",
                escape(text),
            )
        } else {
            self.write_text(text)
        }
    }

    pub fn join_if_non_empty<'i>(
        &mut self,
        sep: &str,
        items: impl IntoIterator<Item = &'i dyn DisplayHtml>,
    ) -> fmt::Result {
        let mut first = true;

        items
            .into_iter()
            .filter(|s| !s.is_empty_str())
            .try_for_each(|s| {
                if !first {
                    self.write_text(sep)?;
                } else {
                    first = false
                };

                s.fmt(self)
            })
    }
}

pub struct DisplayHtmlWrapper<'a, T> {
    val: &'a T,
    plain_text: bool,
}

impl<T: DisplayHtml> Display for DisplayHtmlWrapper<'_, T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        self.val.fmt(&mut HtmlFormatter {
            fmt,
            plain_text: self.plain_text,
        })
    }
}

pub trait DisplayHtml {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result;
    fn is_empty_str(&self) -> bool;

    fn to_plaintext(&self) -> DisplayHtmlWrapper<Self>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: self,
            plain_text: true,
        }
    }

    fn to_html(&self) -> DisplayHtmlWrapper<Self>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: self,
            plain_text: false,
        }
    }
}

impl<T: DisplayHtml> DisplayHtml for SerOnlyDisplay<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        self.0.fmt(f)
    }

    fn is_empty_str(&self) -> bool {
        self.0.is_empty_str()
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

impl DisplayHtml for &str {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_text(self)
    }

    fn is_empty_str(&self) -> bool {
        self.is_empty()
    }
}

impl DisplayHtml for String {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        f.write_text(self)
    }

    fn is_empty_str(&self) -> bool {
        self.is_empty()
    }
}

impl DisplayHtml for PublicUserInfo {
    fn fmt(&self, f: &mut HtmlFormatter) -> std::fmt::Result {
        f.write_text(
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
            f.write_text("/")?;
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
        f.write_text(" - class ")?;
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

macro_rules! impl_display_html {
    ($($typ:ty),*) => {
        $(impl DisplayHtml for $typ {
            fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
                DisplayHtml::fmt(&self.english, f)?;
                f.write_text(" - ")?;
                DisplayHtml::fmt(&self.xhosa, f)?;

                f.write_text(" (")?;
                f.join_if_non_empty(" ", [
                    &if self.is_informal { "informal" } else { "" },
                    &if self.is_inchoative { "inchoative" } else { "" },
                    &self.transitivity as &dyn DisplayHtml,
                    &if self.is_plural { "plural" } else { "" },
                    &self.part_of_speech,
                    &self.noun_class.as_ref().map(NounClassInHit),
                ])?;
                f.write_text(")")
            }

            fn is_empty_str(&self) -> bool {
                false
            }
        })*
    };
}

impl_display_html!(WordHit, crate::types::ExistingWord);
