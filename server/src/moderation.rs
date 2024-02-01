use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::{with_moderator_auth, FullUser};
use crate::database::deletion::{
    ExampleDeletionSuggestion, LinkedWordDeletionSuggestion, WordDeletionSuggestion,
};
use crate::database::submit::{submit_suggestion, WordSubmission};
use crate::database::suggestion::{SuggestedExample, SuggestedLinkedWord, SuggestedWord};
use crate::search::TantivyClient;
use crate::serialization::qs_form;
use crate::submit::edit_suggestion_page;
use crate::{spawn_blocking_child, DebugBoxedExt};
use askama::Template;
use isixhosa_common::auth::{Auth, Permissions};
use isixhosa_common::database::WordId;
use isixhosa_common::database::{DbBase, ModeratorAccessDb, WordOrSuggestionId};
use isixhosa_common::format::DisplayHtml;
use isixhosa_common::types::{ExistingWord, WordHit};
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use tracing::{error, instrument, Span};
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
    #[instrument(
        name = "Fetch all word associated edits",
        fields(relevant_words),
        skip(db)
    )]
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

        Span::current().record("relevant_words", &vec.len());

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
    next_suggestion: Option<u32>,
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
    suggestion_anchor_ord: u32,
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

pub fn moderation(
    db: DbBase,
    tantivy: Arc<TantivyClient>,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let with_tantivy = warp::any().map(move || tantivy.clone());

    let show_all = warp::get()
        .and(warp::any().map(|| None)) // previous_success is None
        .and(with_moderator_auth(db.clone()))
        .and_then(moderation_template);

    let process_one = warp::post()
        .and(with_tantivy.clone())
        .and(warp::body::form::<Action>())
        .and(with_moderator_auth(db.clone()))
        .and_then(process_one);

    let submit_edit = warp::post()
        .and(body::content_length_limit(64 * 1024))
        .and(with_tantivy)
        .and(qs_form())
        .and(with_moderator_auth(db.clone()))
        .and_then(edit_suggestion_form);

    let edit_failed = warp::any()
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: Some(Method::Edit),
                next_suggestion: None,
            })
        }))
        .and(with_moderator_auth(db.clone()))
        .and_then(moderation_template);

    let other_failed = warp::any()
        .and(warp::any().map(|| {
            Some(Success {
                success: false,
                method: None,
                next_suggestion: None,
            })
        }))
        .and(with_moderator_auth(db))
        .and_then(moderation_template);

    let root = warp::path::end().and(show_all.or(process_one).or(other_failed));
    let submit_edit = warp::path("edit")
        .and(warp::path::end())
        .and(submit_edit.or(edit_failed));

    warp::path("moderation")
        .and(root.or(submit_edit))
        .debug_boxed()
}

#[instrument(name = "Display moderation template", skip_all)]
async fn moderation_template(
    previous_success: Option<Success>,
    user: FullUser,
    db: impl ModeratorAccessDb,
) -> Result<impl warp::Reply, Rejection> {
    spawn_blocking_child(move || {
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

#[instrument(
    name = "Process edit suggestion form",
    fields(
        suggestion_id = submission.suggestion_id,
        existing_word_id = submission.existing_id,
    ),
    skip_all,
)]
async fn edit_suggestion_form(
    tantivy: Arc<TantivyClient>,
    submission: WordSubmission,
    user: FullUser,
    db: impl ModeratorAccessDb,
) -> Result<impl Reply, Rejection> {
    let next_suggestion = submission.suggestion_anchor_ord;
    submit_suggestion(submission, tantivy, &user, &db).await;
    moderation_template(
        Some(Success {
            success: true,
            method: Some(Method::Edit),
            next_suggestion,
        }),
        user,
        db,
    )
    .await
}

async fn accept_suggested_word(
    db: &impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || {
        SuggestedWord::fetch_full(&db, suggestion)
            .unwrap()
            .accept_whole_word_suggestion(&db, tantivy);
    })
    .await
    .unwrap();

    true
}

async fn reject_suggested_word(
    db: &impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    suggestion_id: u64,
) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || SuggestedWord::delete(&db, tantivy, suggestion_id))
        .await
        .unwrap()
}

async fn accept_deletion(
    db: &impl ModeratorAccessDb,
    tantivy: Arc<TantivyClient>,
    suggestion: u64,
) -> bool {
    let word_id = WordDeletionSuggestion::fetch_word_id_for_suggestion(db, suggestion);
    Span::current().record("word_id", word_id);
    let db = db.clone();

    spawn_blocking_child(move || ExistingWord::delete(&db, word_id))
        .await
        .unwrap();

    tantivy
        .delete_word(WordOrSuggestionId::existing(word_id))
        .await;

    true
}

async fn reject_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || WordDeletionSuggestion::reject(&db, suggestion))
        .await
        .unwrap();

    true
}

async fn accept_suggested_example(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || {
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
    spawn_blocking_child(move || SuggestedExample::delete(&db, suggestion))
        .await
        .unwrap()
}

async fn accept_example_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || ExampleDeletionSuggestion::accept(&db, suggestion))
        .await
        .unwrap();

    true
}

async fn reject_example_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || ExampleDeletionSuggestion::delete_suggestion(&db, suggestion))
        .await
        .unwrap();

    true
}

async fn accept_linked_word(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || SuggestedLinkedWord::fetch(&db, suggestion).accept(&db))
        .await
        .unwrap();
    true
}

async fn reject_linked_word(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || SuggestedLinkedWord::delete(&db, suggestion))
        .await
        .unwrap();
    true
}

async fn accept_linked_word_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || LinkedWordDeletionSuggestion::accept(&db, suggestion))
        .await
        .unwrap();
    true
}

async fn reject_linked_word_deletion(db: &impl ModeratorAccessDb, suggestion: u64) -> bool {
    let db = db.clone();
    spawn_blocking_child(move || LinkedWordDeletionSuggestion::delete_suggestion(&db, suggestion))
        .await
        .unwrap();
    true
}

#[instrument(name = "Process moderation page action", skip(user, db, tantivy))]
async fn process_one(
    tantivy: Arc<TantivyClient>,
    params: Action,
    user: FullUser,
    db: impl ModeratorAccessDb,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();

    let edit_unsupported = || {
        error!("Got request to edit word or example deletion suggestion, but this makes no sense!");
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
                return edit_suggestion_page(db, user, suggestion, params.suggestion_anchor_ord)
                    .await
                    .map(Reply::into_response)
            }
            Method::Accept => accept_suggested_word(&db, tantivy, suggestion).await,
            Method::Reject => reject_suggested_word(&db, tantivy, suggestion).await,
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

    moderation_template(
        Some(Success {
            success,
            method: Some(params.method),
            next_suggestion: params.suggestion_anchor_ord.checked_sub(1),
        }),
        user,
        db_clone,
    )
    .await
    .map(Reply::into_response)
}
