use crate::language::{
    ConjunctionFollowedBy, NounClassPrefixes, PartOfSpeech, Transitivity, WordLinkType,
};
use isixhosa::noun::NounClass;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExistingExample {
    pub example_id: u64,
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
}

#[derive(Debug, Serialize)]
pub struct ExistingLinkedWord {
    pub link_id: u64,
    pub first_word_id: u64,
    pub second_word_id: u64,
    pub link_type: WordLinkType,
    pub other: WordHit,
}

#[derive(Debug)]
pub struct ExistingWord {
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: Option<PartOfSpeech>,

    pub xhosa_tone_markings: String,
    pub infinitive: String,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub transitivity: Option<Transitivity>,
    pub followed_by: Option<ConjunctionFollowedBy>,
    pub noun_class: Option<NounClass>,
    pub note: String,

    pub is_informal: bool,

    pub examples: Vec<ExistingExample>,
    pub linked_words: Vec<ExistingLinkedWord>,
    pub contributors: Vec<PublicUserInfo>,
    pub datasets: Vec<Dataset>,
}

impl ExistingWord {
    /// Returns `true` if the word has any grammatical information specified
    pub fn has_grammatical_information(&self) -> bool {
        self.part_of_speech.is_some()
            || !self.xhosa_tone_markings.is_empty()
            || !self.infinitive.is_empty()
            || self.is_plural
            || self.is_inchoative
            || self.transitivity.is_some()
            || self.followed_by.is_some()
            || self.noun_class.is_some()
            || !self.note.is_empty()
            || self.is_informal
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordHit {
    pub id: u64,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: Option<PartOfSpeech>,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub is_informal: bool,
    pub transitivity: Option<Transitivity>,
    pub noun_class: Option<NounClassPrefixes>,
    pub is_suggestion: bool,
}

impl Hash for WordHit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl PartialEq for WordHit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for WordHit {}

impl WordHit {
    pub fn empty() -> WordHit {
        WordHit {
            id: 0,
            english: String::new(),
            xhosa: String::new(),
            part_of_speech: None,
            is_plural: false,
            is_inchoative: false,
            is_informal: false,
            transitivity: None,
            noun_class: None,
            is_suggestion: false,
        }
    }

    pub fn has_grammatical_information(&self) -> bool {
        self.part_of_speech.is_some()
            || self.is_plural
            || self.is_inchoative
            || self.transitivity.is_some()
            || self.noun_class.is_some()
            || self.is_informal
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicUserInfo {
    pub id: NonZeroU64,
    pub username: String,
    pub display_name: bool,
}

/// An external dataset from which a word in the dictionary is sourced
#[derive(Clone, Debug)]
pub struct Dataset {
    pub id: u64,
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub institution: Option<String>,
    pub url: Option<String>,
}
