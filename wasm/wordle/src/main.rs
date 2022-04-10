use serde::Deserialize;
use std::fmt::{self, Display, Formatter, Write};
use std::iter;
use ascii::{AsciiChar, AsciiString, IntoAsciiString};
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use gloo::events::EventListener;
use rand::prelude::*;
use tinyvec::ArrayVec;
use wasm_bindgen::JsCast;
use yew::{Callback, Component, Context, function_component, Html, html, Properties};
use wasm_bindgen::prelude::*;

const SEED: u64 = 11530789889988543623;
static CSV: &[u8] = include_bytes!("../words.csv");
const WORD_LENGTH: usize = 5;
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
        Key {
            key,
            state: None,
        }
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
                let key = key.key.clone();
                Callback::from(move |_| {
                    on_click.emit(key.clone());
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

        for (idx, res) in result.iter_mut().enumerate().filter(|(_, res)| **res == CharGuessResult::Incorrect) {
            if let Some(idx) = remaining_letters.as_str().find(char::from(guess[idx].letter)) {
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

impl Display for Game {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let guesses = self.guesses.len();
        let score = if self.state == GameState::Won {
            &guesses as &dyn Display
        } else {
            &"X" as &dyn Display
        };

        write!(f, "IsiXhosa Wordle {} {}/{}\n", self.nth_wordle + 1, score, GUESSES)?;

        for guess in self.guesses {
            let result = match self.evaluate_guess(&guess) {
                Some(r) => r,
                None => continue,
            };

            for char in result {
                write!(f, "{}", char)?;
            }

            write!(f, "\n")?;
        }

        write!(f, "https://isixhosa.click/wordle/")
    }
}

impl Component for Game {
    type Message = Message;
    type Properties = ();

    fn create(_ctx: &Context<Self>) -> Self {
        let list: Vec<_> = csv::Reader::from_reader(CSV)
            .deserialize::<WordRecord>()
            .flatten()
            .collect();

        let process = |word: String, infinitive: String| {
            iter::once((word, infinitive))
                .filter(|(word, inf)| word.len() == WORD_LENGTH || inf.len() == WORD_LENGTH)
                .map(|(word, infinitive)| {
                    let mut s = if word.len() == WORD_LENGTH {
                        word
                    } else {
                        infinitive
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
            .filter_map(|(word_id, word, infinitive)| process(word, infinitive).map(|text| GuessWord { word_id, text }))
            .collect();

        // Targets exclude infinitives and plurals
        let mut targets: Vec<GuessWord> = list
            .into_iter()
            .filter(|word| !word.is_plural)
            .map(|word| (word.word_id, word.xhosa))
            .filter_map(|(word_id, word)| process(word, String::new()).map(|text| GuessWord { word_id, text }))
            .collect();

        let mut rng = StdRng::seed_from_u64(SEED);
        targets.sort_unstable_by(|a, b| a.text.cmp(&b.text));
        targets.dedup_by(|a, b| a.text == b.text);
        targets.shuffle(&mut rng);

        let start_date = Local.from_utc_date(&NaiveDate::from_ymd(2022, 3, 29));
        let since_epoch = (js_sys::Date::now() / 1000.0) as i64;

        let now = NaiveDateTime::from_timestamp(since_epoch, 0);
        let now = Local.from_local_datetime(&now).unwrap().date();
        let nth_wordle = (now - start_date).num_days() as usize;
        let word = targets[targets.len() - 1 - nth_wordle].clone();

        let rows = ["QWERTYUIOP", "|ASDFGHJKL|", "\nZXCVBNM\x08"];
        let keys: Vec<Vec<Key>> = rows.into_iter().map(|row| {
            row.chars().map(|c| Key::from(AsciiChar::new(c))).collect()
        }).collect();

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
            shared: false
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Message) -> bool {
        match msg {
            Message::CloseModal => {
                self.modal_open = false;
                true
            }
            Message::Share => {
                share(format!("{}", self));
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
                    },
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
        let guesses_padded = self.guesses
            .iter()
            .copied()
            .chain(iter::repeat(Guess::default()))
            .take(GUESSES);

        let guesses = guesses_padded.map(|guess| {
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
        }).collect::<Html>();

        let message = if self.state == GameState::Won {
            "Congratulations! A new wordle will be available tomorrow."
        } else {
            "Unlucky... try again tomorrow."
        };

        let modal_classes = if self.modal_open {
            "modal open"
        } else {
            "modal"
        };

        let shared = if self.shared {
            "Copied!"
        } else {
            ""
        };

        let on_key = ctx.link().callback(|key| Message::Key(key));
        let on_share = ctx.link().callback(|_| Message::Share);
        let on_close = ctx.link().callback(|_| Message::CloseModal);

        html! {
            <>
                <h1>{ "Xhosa Wordle (beta)" }</h1>

                <div id="guesses">{ guesses }</div>

                <div id="keyboard">
                    <Keyboard keys={ self.keyboard.clone() } on_click={ on_key }/>
                </div>

                <div id="wordle_modal" class={ modal_classes }>
                    <div class="column_list">
                        <button id="close_modal" class="material-icons" onclick={ on_close }>{ "close" }</button>
                        <p id="result">{ message }</p>

                        <p id="todays_word">
                            { "Today's word was " }
                            <a href={ format!("/word/{}", self.word.word_id) } target="_blank" rel="noreferrer noopener">
                                { format!("{}.", self.word.text.to_ascii_lowercase()) }
                            </a>
                        </p>

                        <button id="share" onclick={ on_share }>
                            { "Share with your friends" }
                            <span class="material-icons">{ "share" }</span>
                        </button>
                        <p id="shared">{{ shared }}</p>
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

        let callback = ctx.link().callback(|c: char| Message::Key(AsciiChar::new(c)));
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

fn main() {
    wasm_logger::init(wasm_logger::Config::default());
    let wrap = gloo::utils::document().get_element_by_id("main_wrap");
    yew::start_app_in_element::<Game>(wrap.unwrap());
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

impl Display for CharGuessResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use CharGuessResult::*;

        let emoji = match self {
            Correct => '🟩',
            WrongPlace => '🟨',
            Incorrect => '⬛',
        };

        f.write_char(emoji)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum GameState {
    Won,
    Continue,
    Lost,
}

#[derive(Copy, Clone, Debug)]
struct GuessResult {
    typ: GameState,
    result: [CharGuessResult; WORD_LENGTH],
}

impl Display for GuessResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for c in self.result {
            write!(f, "{}", c)?;
        }

        match self.typ {
            GameState::Won => write!(f, "You have won the game!"),
            GameState::Lost => write!(f, "You have lost the game, better luck next time!"),
            GameState::Continue => Ok(()),
        }
    }
}
