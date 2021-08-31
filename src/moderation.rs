use crate::auth::{with_moderator_auth, Auth, DbBase, User};
use crate::database::deletion::{
    ExampleDeletionSuggestion, LinkedWordDeletionSuggestion, WordDeletionSuggestion,
};
use crate::database::existing::ExistingWord;
use crate::database::suggestion::{
    MaybeEdited, SuggestedExample, SuggestedLinkedWord, SuggestedWord,
};
use crate::language::NounClassExt;
use crate::search::{TantivyClient, WordDocument, WordHit};
use crate::serialization::qs_form;
use crate::serialization::OptionMapNounClassExt;
use crate::submit::{edit_suggestion_page, submit_suggestion, WordId, WordSubmission};
use askama::Template;

use crate::auth::ModeratorAccessDb;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use std::collections::HashMap;
use std::sync::Arc;
use warp::{body, Filter, Rejection, Reply};

#[derive(Template, Debug)]
#[template(path = "moderation.askama.html")]
struct ModerationTemplate {
    auth: Auth,
    previous_success: Option<Success>,
    word_suggestions: Vec<SuggestedWord>,
    word_deletions: Vec<WordDeletionSuggestion>,
    word_associated_edits: Vec<(WordHit, WordAssociatedEdits)>,
}

impl ModerationTemplate {
    fn is_empty(&self) -> bool {
        self.word_suggestions.is_empty()
            && self.word_deletions.is_empty()
            && self.word_associated_edits.is_empty()
    }
}

/// Edits that are associated to a word but not of the word itself, e.g examples
#[derive(Default, Debug)]
pub struct WordAssociatedEdits {
    example_suggestions: Vec<SuggestedExample>,
    example_deletion_suggestions: Vec<ExampleDeletionSuggestion>,
    linked_word_suggestions: Vec<SuggestedLinkedWord>,
    linked_word_deletion_suggestions: Vec<LinkedWordDeletionSuggestion>,
}

impl WordAssociatedEdits {
    pub fn fetch_all(db: &impl ModeratorAccessDb) -> Vec<(WordHit, WordAssociatedEdits)> {
        let example_suggestions = SuggestedExample::fetch_all_for_existing_words(db);
        let example_deletions = ExampleDeletionSuggestion::fetch_all(db);
        let linked_word_suggestions = SuggestedLinkedWord::fetch_all_for_existing_words(db);
        let linked_word_deletion_suggestions = LinkedWordDeletionSuggestion::fetch_all(db);

        let mut map: HashMap<WordId, WordAssociatedEdits> = HashMap::new();

        for (id, suggestions) in example_suggestions {
            map.entry(id)
                .or_insert_with(Default::default)
                .example_suggestions = suggestions;
        }

        for (id, deletions) in example_deletions {
            map.entry(id)
                .or_insert_with(Default::default)
                .example_deletion_suggestions = deletions;
        }

        for (id, suggestions) in linked_word_suggestions {
            map.entry(id)
                .or_insert_with(Default::default)
                .linked_word_suggestions = suggestions;
        }

        for (id, deletions) in linked_word_deletion_suggestions {
            map.entry(id)
                .or_insert_with(Default::default)
                .linked_word_deletion_suggestions = deletions;
        }

        let mut vec: Vec<(WordHit, WordAssociatedEdits)> = map
            .into_iter()
            .map(|(id, assoc)| (WordHit::fetch_from_db(db, id.into()).unwrap(), assoc))
            .collect();
        vec.sort_by_key(|(hit, _)| hit.id);
        vec
    }

    fn examples_is_empty(&self) -> bool {
        self.example_suggestions.is_empty() && self.example_deletion_suggestions.is_empty()
    }

    fn linked_words_is_empty(&self) -> bool {
        self.linked_word_suggestions.is_empty() && self.linked_word_deletion_suggestions.is_empty()
    }
}

#[derive(Debug)]
struct Success {
    success: bool,
    method: Option<Method>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Method {
    Edit,
    Accept,
    Reject,
}

#[derive(Deserialize, Debug)]
struct Action {
    #[serde(flatten)]
    suggestion: ActionTarget,
    method: Method,
}

#[serde_as]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "suggestion_type", content = "suggestion")]
enum ActionTarget {
    WordDeletion(#[serde_as(as = "DisplayFromStr")] u64),
    Word(#[serde_as(as = "DisplayFromStr")] u64),
    Example(#[serde_as(as = "DisplayFromStr")] u64),
    ExampleDeletion(#[serde_as(as = "DisplayFromStr")] u64),
    LinkedWord(#[serde_as(as = "DisplayFromStr")] u64),
    LinkedWordDeletion(#[serde_as(as = "DisplayFromStr")] u64),
}

// TODO auth
pub fn accept(
    db: DbBase,
    tantivy: Arc<TantivyClient>,
) -> impl Filter<Error = Rejection, Extract: Reply> + Clone {
    let tantivy = warp::any().map(move || tantivy.clone());

    let show_all = warp::get()
        .and(with_moderator_auth(db.clone()))
        .and(warp::any().map(|| None)) // previous_success is None
        .and_then(suggested_words);

    let process_one = warp::post()
        .and(with_moderator_auth(db.clone()))
        .and(tantivy)
        .and(warp::body::form::<Action>())
        .and_then(process_one);

    let submit_edit = warp::post()
        .and(body::content_length_limit(4 * 1024))
        .and(with_moderator_auth(db.clone()))
        .and(qs_form())
        .and_then(edit_suggestion_form);

    let edit_failed = warp::any()
        .and(with_moderator_auth(db.clone()))
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: Some(Method::Edit),
            })
        }))
        .and_then(suggested_words);

    let other_failed = warp::any()
        .and(with_moderator_auth(db))
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: None,
            })
        }))
        .and_then(suggested_words);

    let root = warp::path::end().and(show_all.or(process_one).or(other_failed));
    let submit_edit = warp::path("edit")
        .and(warp::path::end())
        .and(submit_edit.or(edit_failed));

    warp::path("moderation").and(root.or(submit_edit)).boxed()
}

async fn suggested_words(
    user: User,
    db: impl ModeratorAccessDb,
    previous_success: Option<Success>,
) -> Result<impl warp::Reply, Rejection> {
    tokio::task::spawn_blocking(move || {
        Ok(ModerationTemplate {
            auth: user.into(),
            previous_success,
            word_suggestions: SuggestedWord::fetch_all_full(&db),
            word_deletions: WordDeletionSuggestion::fetch_all(&db),
            word_associated_edits: WordAssociatedEdits::fetch_all(&db),
        })
    })
    .await
    .unwrap()
}

async fn edit_suggestion_form(
    user: User,
    db: impl ModeratorAccessDb,
    submission: WordSubmission,
) -> Result<impl Reply, Rejection> {
    submit_suggestion(submission, &db).await;
    suggested_words(
        user,
        db,
        Some(Success {
            success: true,
            method: Some(Method::Edit),
        }),
    )
    .await
}

async fn accept_suggested_word(
    db: &impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> bool {
    let db = db.clone();
    let (word, id) = tokio::task::spawn_blocking(move || {
        let word = SuggestedWord::fetch_full(&db, suggestion).unwrap();
        (word.clone(), word.accept_whole_word_suggestion(&db))
    })
    .await
    .unwrap();

    let document = WordDocument {
        id: id as u64,
        english: word.english.current().clone(),
        xhosa: word.xhosa.current().clone(),
        part_of_speech: *word.part_of_speech.current(),
        is_plural: *word.is_plural.current(),
        noun_class: *word.noun_class.current(),
    };

    if word.word_id.is_none() {
        tantivy.add_new_word(document).await
    } else {
        tantivy.edit_word(document).await
    }

    true
}

async fn reject_suggested_word(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || SuggestedWord::delete(&db, suggestion))
        .await
        .unwrap()
}

async fn accept_deletion(
    db: &impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> bool {
    let word_id = WordDeletionSuggestion::fetch_word_id_for_suggestion(db, suggestion);
    let db = db.clone();
    tokio::task::spawn_blocking(move || ExistingWord::delete(&db, word_id))
        .await
        .unwrap();
    tantivy.delete_word(word_id).await;

    true
}

async fn reject_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || WordDeletionSuggestion::reject(&db, suggestion))
        .await
        .unwrap();

    true
}

async fn accept_suggested_example(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        SuggestedExample::fetch(&db, suggestion)
            .unwrap()
            .accept(&db)
    })
    .await
    .unwrap();

    true
}

async fn reject_suggested_example(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || SuggestedExample::delete(&db, suggestion))
        .await
        .unwrap()
}

async fn accept_example_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || ExampleDeletionSuggestion::accept(&db, suggestion))
        .await
        .unwrap();

    true
}

async fn reject_example_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        ExampleDeletionSuggestion::delete_suggestion(&db, suggestion)
    })
    .await
    .unwrap();

    true
}

async fn accept_linked_word(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || SuggestedLinkedWord::fetch(&db, suggestion).accept(&db))
        .await
        .unwrap();
    true
}

async fn reject_linked_word(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || SuggestedLinkedWord::delete(&db, suggestion))
        .await
        .unwrap();
    true
}

async fn accept_linked_word_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || LinkedWordDeletionSuggestion::accept(&db, suggestion))
        .await
        .unwrap();
    true
}

async fn reject_linked_word_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        LinkedWordDeletionSuggestion::delete_suggestion(&db, suggestion)
    })
    .await
    .unwrap();
    true
}

async fn process_one(
    user: User,
    db: impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    params: Action,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();

    let edit_unsupported = || {
        log::error!(
            "Got request to edit word or example deletion suggestion, but this makes no sense!"
        );
        false
    };

    let success = match params.suggestion {
        ActionTarget::WordDeletion(suggestion) => match params.method {
            Method::Edit => edit_unsupported(),
            Method::Accept => accept_deletion(&db, tantivy, suggestion).await,
            Method::Reject => reject_deletion(&db, suggestion).await,
        },
        ActionTarget::Word(suggestion) => match params.method {
            Method::Edit => {
                return edit_suggestion_page(db, user, suggestion)
                    .await
                    .map(Reply::into_response)
            }
            Method::Accept => accept_suggested_word(&db, tantivy, suggestion).await,
            Method::Reject => reject_suggested_word(&db, suggestion).await,
        },
        ActionTarget::Example(suggestion) => match params.method {
            Method::Edit => todo!("Example standalone editing"),
            Method::Accept => accept_suggested_example(&db, suggestion).await,
            Method::Reject => reject_suggested_example(&db, suggestion).await,
        },
        ActionTarget::ExampleDeletion(suggestion) => match params.method {
            Method::Edit => edit_unsupported(),
            Method::Accept => accept_example_deletion(&db, suggestion).await,
            Method::Reject => reject_example_deletion(&db, suggestion).await,
        },
        ActionTarget::LinkedWord(suggestion) => match params.method {
            Method::Edit => todo!("Linked word standalone editing"),
            Method::Accept => accept_linked_word(&db, suggestion).await,
            Method::Reject => reject_linked_word(&db, suggestion).await,
        },
        ActionTarget::LinkedWordDeletion(suggestion) => match params.method {
            Method::Edit => edit_unsupported(),
            Method::Accept => accept_linked_word_deletion(&db, suggestion).await,
            Method::Reject => reject_linked_word_deletion(&db, suggestion).await,
        },
    };

    suggested_words(
        user,
        db_clone,
        Some(Success {
            success,
            method: Some(params.method),
        }),
    )
    .await
    .map(Reply::into_response)
}
