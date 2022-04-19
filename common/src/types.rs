use std::num::NonZeroU64;
use isixhosa::noun::NounClass;
use serde::{Serialize, Deserialize};
use crate::language::{ConjunctionFollowedBy, NounClassPrefixes, PartOfSpeech, Transitivity, WordLinkType};
use crate::serialization::SerOnlyDisplay;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExistingExample {
    pub example_id: u64,
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExistingLinkedWord {
    pub link_id: u64,
    pub first_word_id: u64,
    pub second_word_id: u64,
    pub link_type: WordLinkType,
    pub other: WordHit,
}

#[derive(Debug, Deserialize)]
pub struct ExistingWord {
    pub word_id: u64,

    pub english: String,
    pub xhosa: String,
    pub part_of_speech: PartOfSpeech,

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
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Deserialize, Serialize)]
pub struct WordHit {
    pub id: u64,
    pub english: String,
    pub xhosa: String,
    pub part_of_speech: SerOnlyDisplay<PartOfSpeech>,
    pub is_plural: bool,
    pub is_inchoative: bool,
    pub is_informal: bool,
    pub transitivity: Option<SerOnlyDisplay<Transitivity>>,
    pub noun_class: Option<NounClassPrefixes>,
    pub is_suggestion: bool,
}

impl WordHit {
    pub fn empty() -> WordHit {
        WordHit {
            id: 0,
            english: String::new(),
            xhosa: String::new(),
            part_of_speech: SerOnlyDisplay(PartOfSpeech::Interjection),
            is_plural: false,
            is_inchoative: false,
            is_informal: false,
            transitivity: None,
            noun_class: None,
            is_suggestion: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublicUserInfo {
    pub id: NonZeroU64,
    pub username: String,
    pub display_name: bool,
}