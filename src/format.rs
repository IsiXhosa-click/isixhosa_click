use crate::database::existing::ExistingWord;
use crate::database::suggestion::{MaybeEdited, SuggestedWord};
use crate::language::NounClassExt;
use crate::search::WordHit;
use crate::serialization::{SerOnlyDisplay, SerializePrimitive};
use askama::{Html, MarkupDisplay};
use isixhosa::noun::NounClass;
use std::borrow::Borrow;
use std::fmt::{self, Display, Formatter};
use crate::language::Transitivity;

pub mod filters {
    use super::*;

    pub fn html<'a, T: DisplayHtml + 'a>(
        v: T,
    ) -> askama::Result<MarkupDisplay<Html, DisplayHtmlWrapper<'a, T>>> {
        let w = DisplayHtmlWrapper {
            val: OwnedOrBorrowed::Owned(v),
            plain_text: false,
        };

        Ok(MarkupDisplay::new_safe(w, Html))
    }
}

enum OwnedOrBorrowed<'a, T> {
    Owned(T),
    Borrowed(&'a T),
}

impl<'a, T> Borrow<T> for OwnedOrBorrowed<'a, T> {
    fn borrow(&self) -> &T {
        match self {
            OwnedOrBorrowed::Owned(v) => v,
            OwnedOrBorrowed::Borrowed(b) => b,
        }
    }
}

fn escape(s: &str) -> MarkupDisplay<Html, &str> {
    MarkupDisplay::new_unsafe(s, Html)
}

pub struct HtmlFormatter<'a, 'b> {
    fmt: &'a mut Formatter<'b>,
    plain_text: bool,
}

impl<'a, 'b> HtmlFormatter<'a, 'b> {
    pub fn write_text(&mut self, text: &str) -> fmt::Result {
        escape(text).fmt(self.fmt)
    }

    pub fn write_noun_class_prefix(&mut self, text: &str, strong: bool) -> fmt::Result {
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
    val: OwnedOrBorrowed<'a, T>,
    plain_text: bool,
}

impl<T: DisplayHtml> Display for DisplayHtmlWrapper<'_, T> {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        let b: &T = self.val.borrow();
        b.fmt(&mut HtmlFormatter {
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
            val: OwnedOrBorrowed::Borrowed(self),
            plain_text: true,
        }
    }

    fn to_html(&self) -> DisplayHtmlWrapper<Self>
    where
        Self: Sized,
    {
        DisplayHtmlWrapper {
            val: OwnedOrBorrowed::Borrowed(self),
            plain_text: false,
        }
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

impl<T: DisplayHtml, P> DisplayHtml for SerializePrimitive<T, P> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        self.val.fmt(f)
    }

    fn is_empty_str(&self) -> bool {
        self.val.is_empty_str()
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

trait TextIfBoolIn {
    fn into_maybe_edited(self) -> MaybeEdited<bool>;
}

impl TextIfBoolIn for bool {
    fn into_maybe_edited(self) -> MaybeEdited<bool> {
        MaybeEdited::Old(self)
    }
}

impl TextIfBoolIn for MaybeEdited<bool> {
    fn into_maybe_edited(self) -> MaybeEdited<bool> {
        self
    }
}

fn text_if_bool<T: TextIfBoolIn>(
    yes: &'static str,
    no: &'static str,
    b: T,
) -> MaybeEdited<&'static str> {
    match b.into_maybe_edited() {
        MaybeEdited::Edited { new, old } => MaybeEdited::Edited {
            new: if new { yes } else { no },
            old: if old { yes } else { no },
        },
        MaybeEdited::New(b) if b => MaybeEdited::New(yes),
        MaybeEdited::Old(b) if b => MaybeEdited::Old(yes),
        _ => MaybeEdited::Old(""),
    }
}

impl<T: DisplayHtml> DisplayHtml for MaybeEdited<T> {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        match self {
            MaybeEdited::Edited { new, old } => {
                f.fmt.write_str("<ins>")?;
                if new.is_empty_str() {
                    f.fmt.write_str("[Removed]")?;
                } else {
                    new.fmt(f)?;
                }
                f.fmt.write_str("</ins> ")?;

                f.fmt.write_str("<del>")?;
                if old.is_empty_str() {
                    f.fmt.write_str("[None]")?;
                } else {
                    old.fmt(f)?;
                }
                f.fmt.write_str("</del>")
            }
            MaybeEdited::Old(old) => old.fmt(f),
            MaybeEdited::New(new) => {
                f.fmt.write_str("<ins>")?;
                new.fmt(f)?;
                f.fmt.write_str("</ins>")
            }
        }
    }

    fn is_empty_str(&self) -> bool {
        match self {
            MaybeEdited::Edited { new, old } => new.is_empty_str() && old.is_empty_str(),
            MaybeEdited::Old(v) => v.is_empty_str(),
            MaybeEdited::New(v) => v.is_empty_str(),
        }
    }
}

impl DisplayHtml for NounClass {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        let prefixes = self.to_prefixes();

        f.write_noun_class_prefix(prefixes.singular, prefixes.selected_singular)?;
        if let Some(plural) = prefixes.plural {
            f.write_text("/")?;
            f.write_noun_class_prefix(plural, !prefixes.selected_singular)?;
        }

        Ok(())
    }

    fn is_empty_str(&self) -> bool {
        false
    }
}

impl DisplayHtml for SuggestedWord {
    fn fmt(&self, f: &mut HtmlFormatter) -> fmt::Result {
        DisplayHtml::fmt(&self.english, f)?;
        f.write_text(" - ")?;
        DisplayHtml::fmt(&self.xhosa, f)?;

        f.write_text(" (")?;
        f.join_if_non_empty(" ", [
            &text_if_bool("inchoative", "non-inchoative", self.is_inchoative),
            &self.transitivity.map(|x| x.map(|x| Transitivity::explicit_moderation_page(&x))) as &dyn DisplayHtml,
            &text_if_bool("plural", "singular", self.is_plural),
            &self.part_of_speech,
            &self.noun_class,
        ])?;
        f.write_text(")")
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
                    &text_if_bool("inchoative", "non-inchoative", self.is_inchoative),
                    &self.transitivity as &dyn DisplayHtml,
                    &text_if_bool("plural", "singular", self.is_plural),
                    &self.part_of_speech,
                    &self.noun_class,
                ])?;
                f.write_text(")")
            }

            fn is_empty_str(&self) -> bool {
                false
            }
        })*
    };
}

impl_display_html!(WordHit, ExistingWord);
