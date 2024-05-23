mod i18n;

use ascii::{AsciiChar, AsciiString, IntoAsciiString};
use chrono::{Local, NaiveDate, TimeZone};
use fluent_templates::{Loader, StaticLoader};
use gloo::events::EventListener;
use isixhosa_common::format::{DisplayHtml, HtmlFormatter};
use isixhosa_common::i18n::{I18nInfo, SiteContext, TranslationKey};
use isixhosa_common::i18n_args;
use rand::prelude::*;
use serde::Deserialize;
use std::fmt;
use std::iter;
use std::sync::Arc;
use tinyvec::ArrayVec;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use yew::{function_component, html, Callback, Component, Context, Html, Properties};

const SEED: u64 = 11530789889988543623;
static CSV: &[u8] = include_bytes!("../words.csv");
const WORD_LENGTH: usize = 6;
const GUESSES: usize = 6;

#[wasm_bindgen(module = "/wordle.js")]
extern "C" {
    fn share(text: String);
}

#[derive(Clone, PartialEq)]
struct Key {
    key: AsciiChar,
    state: Option<CharGuessResult>,
}

impl Key {
    fn css(&self) -> &'static str {
        self.state.map(|s| s.into_css()).unwrap_or_default()
    }
}

impl From<AsciiChar> for Key {
    fn from(key: AsciiChar) -> Self {
        Key { key, state: None }
    }
}

#[derive(Properties, PartialEq)]
struct KeyboardProps {
    keys: Vec<Vec<Key>>,
    on_click: Callback<AsciiChar>,
}

#[function_component(Keyboard)]
fn keyboard(props: &KeyboardProps) -> Html {
    props.keys.iter().map(|row| {
        let row = row.iter().map(|key| {
            let on_click = {
                let on_click = props.on_click.clone();
                let key = key.key;
                Callback::from(move |_| {
                    on_click.emit(key);
                })
            };


            if key.key.as_char() == '|' {
                html! {
                    <div class="spacer_key"></div>
                }
            } else {
                let (label, class) = match key.key.as_char() {
                    '\n' => ('↵', "key special_key"),
                    '\x08' => ('⌫', "key special_key"),
                    c => (c, "key"),
                };

                html! {
                    <button class={ class } onclick={ on_click } style={ key.css() }>{ label }</button>
                }
            }

        }).collect::<Html>();

        html! {
            <div class="row">{ row }</div>
        }
    }).collect()
}

enum Message {
    Key(AsciiChar),
    CloseModal,
    Share,
}

struct Game {
    dictionary: Vec<GuessWord>,
    word: GuessWord,
    guesses: ArrayVec<[Guess; GUESSES]>,
    state: GameState,
    keyboard: Vec<Vec<Key>>,
    kbd_listener: Option<EventListener>,
    nth_wordle: u32,
    modal_open: bool,
    shared: bool,
    host: String,
}

impl Game {
    fn evaluate_guess(&self, guess: &Guess) -> Option<[CharGuessResult; WORD_LENGTH]> {
        let guess = &guess.letters;
        let guess_str: AsciiString = guess.iter().map(|l| l.letter).collect();

        if !self.dictionary.iter().any(|word| word.text == guess_str) {
            return None;
        }

        let mut result = [CharGuessResult::Incorrect; WORD_LENGTH];
        let mut remaining_letters: AsciiString = self.word.text.clone();

        let mut removal_cursor = 0;
        for idx in 0..WORD_LENGTH {
            if self.word.text[idx] == guess[idx].letter {
                result[idx] = CharGuessResult::Correct;
                remaining_letters.remove(removal_cursor);
            } else {
                removal_cursor += 1;
            }
        }

        for (idx, res) in result
            .iter_mut()
            .enumerate()
            .filter(|(_, res)| **res == CharGuessResult::Incorrect)
        {
            if let Some(idx) = remaining_letters
                .as_str()
                .find(char::from(guess[idx].letter))
            {
                *res = CharGuessResult::WrongPlace;
                remaining_letters.remove(idx);
            }
        }

        Some(result)
    }

    fn guess(&mut self) {
        if self.state != GameState::Continue {
            return;
        }

        let result = match self.evaluate_guess(self.current_guess()) {
            Some(r) => r,
            None => return,
        };

        let guess = self.guesses.last_mut().unwrap();
        for (letter, state) in guess.letters.iter_mut().zip(result.iter()) {
            letter.state = Some(*state);

            for row in &mut self.keyboard {
                for key in row {
                    if key.key == letter.letter && key.state.map(|s| s < *state).unwrap_or(true) {
                        key.state = Some(*state);
                    }
                }
            }
        }

        if result.iter().all(|r| *r == CharGuessResult::Correct) {
            self.state = GameState::Won;
            self.modal_open = true;
            return;
        }

        self.state = if self.guesses.len() == GUESSES {
            self.modal_open = true;
            GameState::Lost
        } else {
            self.guesses.push(Guess::default());
            GameState::Continue
        };
    }

    fn current_guess(&self) -> &Guess {
        self.guesses.last().unwrap()
    }

    fn current_guess_mut(&mut self) -> &mut Guess {
        self.guesses.last_mut().unwrap()
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for Game {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        let guesses = self.guesses.len();
        let score = if self.state == GameState::Won {
            guesses.to_string()
        } else {
            "X".to_string()
        };

        f.write_text_with_args(
            &TranslationKey::new("wordle.share-title"),
            &i18n_args!(
                "nth-wordle" => self.nth_wordle + 1,
                "score" => score,
                "guesses" => GUESSES,
            ),
        )?;

        f.write_raw_str("\n")?;

        for guess in self.guesses {
            let result = match self.evaluate_guess(&guess) {
                Some(r) => r,
                None => continue,
            };

            for char in result {
                char.fmt(f)?;
            }

            f.write_raw_str("\n")?;
        }

        f.write_raw_str("https://")?;
        f.write_raw_str(&self.host)?;
        f.write_raw_str("/wordle")
    }
}

#[derive(Properties, PartialEq)]
struct GameProperties {
    i18n_info: I18nInfo<&'static StaticLoader>,
}

impl Component for Game {
    type Message = Message;
    type Properties = GameProperties;

    fn create(ctx: &Context<Self>) -> Self {
        let list: Vec<_> = csv::Reader::from_reader(CSV)
            .deserialize::<WordRecord>()
            .flatten()
            .collect();

        let process = |word: String, infinitive: String| {
            let (word, infinitive) = (word.trim(), infinitive.trim());
            let word = word.replace("(i)", "i");
            let word = word.replace('?', "");

            iter::once((word, infinitive))
                .filter(|(word, inf)| word.len() == WORD_LENGTH || inf.len() == WORD_LENGTH)
                .map(|(word, infinitive)| {
                    let mut s = if word.len() == WORD_LENGTH {
                        word
                    } else {
                        infinitive.to_owned()
                    };

                    s.make_ascii_uppercase();
                    s
                })
                .filter_map(|s| s.into_ascii_string().ok())
                .next()
        };

        let dictionary: Vec<GuessWord> = list
            .iter()
            .map(|word| (word.word_id, word.xhosa.clone(), word.infinitive.clone()))
            .filter_map(|(word_id, word, infinitive)| {
                process(word, infinitive).map(|text| GuessWord { word_id, text })
            })
            .collect();

        // Targets exclude infinitives and plurals
        let mut targets: Vec<GuessWord> = list
            .into_iter()
            .filter(|word| !word.is_plural)
            .map(|word| (word.word_id, word.xhosa))
            .filter_map(|(word_id, word)| {
                process(word, String::new()).map(|text| GuessWord { word_id, text })
            })
            .collect();

        let mut rng = StdRng::seed_from_u64(SEED);
        targets.sort_unstable_by(|a, b| a.text.cmp(&b.text));
        targets.dedup_by(|a, b| a.text == b.text);
        targets.shuffle(&mut rng);

        log::info!("{}", targets.len());

        let start_date = Local
            .from_local_date(&NaiveDate::from_ymd(2022, 9, 9))
            .unwrap();
        let js_date = js_sys::Date::new_0();
        let naive_date = NaiveDate::from_ymd(
            js_date.get_full_year() as i32,
            js_date.get_month() + 1, // Month starts at 0 in JS
            js_date.get_date(),
        );
        let now = Local.from_local_date(&naive_date).unwrap();

        let nth_wordle = (now - start_date).num_days() as usize;

        let cycle = nth_wordle / targets.len();
        let nth_wordle = nth_wordle % targets.len();

        for _ in 0..cycle {
            targets.shuffle(&mut rng);
        }

        log::info!("{:?}", now - start_date);
        log::info!("{}", nth_wordle);
        log::info!("{:?}", now);

        let word = targets[targets.len() - 1 - nth_wordle].clone();

        let rows = ["QWERTYUIOP", "|ASDFGHJKL|", "\nZXCVBNM\x08"];
        let keys: Vec<Vec<Key>> = rows
            .into_iter()
            .map(|row| row.chars().map(|c| Key::from(AsciiChar::new(c))).collect())
            .collect();

        let mut guesses = ArrayVec::default();

        guesses.push(Guess::default());

        Game {
            dictionary,
            word,
            guesses,
            state: GameState::Continue,
            keyboard: keys,
            kbd_listener: None,
            nth_wordle: nth_wordle as u32,
            modal_open: false,
            shared: false,
            host: ctx.props().i18n_info.ctx.host.clone(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Message) -> bool {
        match msg {
            Message::CloseModal => {
                self.modal_open = false;
                true
            }
            Message::Share => {
                share(self.to_plaintext(&ctx.props().i18n_info).to_string());
                self.shared = true;
                true
            }
            Message::Key(key) => {
                if self.state != GameState::Continue {
                    return false;
                }

                match key.into() {
                    '\x08' => self.current_guess_mut().letters.pop().is_some(),
                    '\n' => {
                        if self.current_guess_mut().letters.len() == WORD_LENGTH {
                            self.guess();
                            true
                        } else {
                            false
                        }
                    }
                    _ => {
                        let guess = &mut self.current_guess_mut().letters;

                        if guess.len() < WORD_LENGTH {
                            guess.push(key.to_ascii_uppercase().into());
                            true
                        } else {
                            false
                        }
                    }
                }
            }
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let guesses_padded = self
            .guesses
            .iter()
            .copied()
            .chain(iter::repeat(Guess::default()))
            .take(GUESSES);

        let guesses = guesses_padded
            .map(|guess| {
                let letters_padded = guess
                    .letters
                    .iter()
                    .copied()
                    .chain(iter::repeat(GuessLetter::default()))
                    .take(WORD_LENGTH);

                let guess = letters_padded.map(|letter| {
                html! { <div class="guess_letter" style={ letter.css() }>{ letter.letter }</div>}
            }).collect::<Html>();

                html! {
                    <div class="guess">{ guess }</div>
                }
            })
            .collect::<Html>();

        let message = if self.state == GameState::Won {
            "wordle.victory"
        } else {
            "wordle.loss"
        };

        let modal_classes = if self.modal_open {
            "modal open"
        } else {
            "modal"
        };

        let shared = if self.shared {
            "wordle.copied"
        } else {
            "empty-string"
        };

        let on_key = ctx.link().callback(Message::Key);
        let on_share = ctx.link().callback(|_| Message::Share);
        let on_close = ctx.link().callback(|_| Message::CloseModal);

        let i18n_info = &ctx.props().i18n_info;
        let t = |key| i18n_info.t(&TranslationKey::new(key));

        html! {
            <>
                <h1>{ t("wordle.name") }</h1>

                <div id="guesses">{ guesses }</div>

                <div id="keyboard">
                    <Keyboard keys={ self.keyboard.clone() } on_click={ on_key }/>
                </div>

                <div id="wordle_modal" class={ modal_classes }>
                    <div class="column_list">
                        <button id="close_modal" class="material-icons" onclick={ on_close }>{ "close" }</button>
                        <p id="result">{ t(message) }</p>

                        <p id="todays_word">
                            { t("wordle.word-was") }{ " " }
                            <a lang={ t("target-language-code") } href={ format!("/word/{}", self.word.word_id) } target="_blank" rel="noreferrer noopener">
                                { format!("{}.", self.word.text.to_ascii_lowercase()) }
                            </a>
                        </p>

                        <button id="share" onclick={ on_share }>
                            { t("wordle.share") }
                            <span class="material-icons">{ "share" }</span>
                        </button>
                        <p id="shared">{ t(shared) }</p>
                    </div>
                </div>
            </>
        }
    }

    fn rendered(&mut self, ctx: &Context<Self>, first_render: bool) {
        if !first_render {
            return;
        }

        let document = gloo::utils::document();

        let callback = ctx
            .link()
            .callback(|c: char| Message::Key(AsciiChar::new(c)));
        let listener = EventListener::new(&document, "keydown", move |event| {
            let event = event.dyn_ref::<web_sys::KeyboardEvent>().unwrap();

            let first_char = event.key().chars().next().unwrap();

            let msg = match &event.key() as &str {
                "Backspace" => '\x08',
                "Enter" => '\n',
                _ if event.key().len() == 1 && first_char.is_ascii_alphabetic() => first_char,
                _ => return,
            };

            callback.emit(msg);
        });

        self.kbd_listener.replace(listener);
    }
}

#[derive(Copy, Clone, PartialEq, Default)]
struct Guess {
    letters: ArrayVec<[GuessLetter; WORD_LENGTH]>,
}

#[derive(Copy, Clone, PartialEq)]
struct GuessLetter {
    state: Option<CharGuessResult>,
    letter: AsciiChar,
}

impl Default for GuessLetter {
    fn default() -> Self {
        GuessLetter {
            state: None,
            letter: AsciiChar::Space,
        }
    }
}

impl GuessLetter {
    fn css(&self) -> &'static str {
        self.state.map(|s| s.into_css()).unwrap_or_default()
    }
}

impl From<AsciiChar> for GuessLetter {
    fn from(letter: AsciiChar) -> Self {
        GuessLetter {
            state: None,
            letter,
        }
    }
}

#[wasm_bindgen(start)]
fn main() {
    wasm_logger::init(wasm_logger::Config::default());
}

#[wasm_bindgen]
pub async fn start_wordle(host: String, lang: &str) {
    let wrap = gloo::utils::document().get_element_by_id("main_wrap");
    log::info!("Loading i18n");
    let loader = i18n::load().await.unwrap();

    let i18n_info = I18nInfo {
        user_language: lang.parse().expect("Invalid locale"),
        ctx: Arc::new(SiteContext {
            site_i18n: loader,
            supported_langs: &[],
            host: host.to_string(),
        }),
    };

    yew::start_app_with_props_in_element::<Game>(wrap.unwrap(), GameProperties { i18n_info });
}

#[derive(Deserialize, Debug)]
struct WordRecord {
    word_id: u64,
    xhosa: String,
    infinitive: String,
    is_plural: bool,
}

#[derive(Debug, Clone)]
struct GuessWord {
    word_id: u64,
    text: AsciiString,
}

#[derive(Eq, PartialEq, Debug, Copy, Clone, Ord, PartialOrd)]
enum CharGuessResult {
    Incorrect = 0,
    WrongPlace = 1,
    Correct = 2,
}

impl CharGuessResult {
    fn into_css(self) -> &'static str {
        use CharGuessResult::*;

        match self {
            Correct => "background-color: green",
            WrongPlace => "background-color: yellow",
            _ => "background-color: dimgray",
        }
    }
}

impl<L: Loader + 'static> DisplayHtml<L> for CharGuessResult {
    fn fmt(&self, f: &mut HtmlFormatter<L>) -> fmt::Result {
        use CharGuessResult::*;

        let emoji = match self {
            Correct => "🟩",
            WrongPlace => "🟨",
            Incorrect => "⬛",
        };

        f.write_raw_str(emoji)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameState {
    Won,
    Continue,
    Lost,
}
